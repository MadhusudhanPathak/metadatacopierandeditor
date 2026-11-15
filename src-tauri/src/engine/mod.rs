//! The bundled ExifTool engine (spec 3.2): one persistent `-stay_open True -@ -` process that
//! receives arguments on stdin (see `protocol`). A dead process is restarted once per command.

pub mod extract;
mod protocol;

pub use protocol::{path_arg, split_messages};

use crate::metadata::tags::TagMap;
use protocol::{COMMON_ARGS, ENGINE_STOPPED, collect_until, command_payload};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// How long to wait for the stderr marker after stdout finished. ExifTool flushes stderr
/// (including the `-echo4` marker) before printing `{readyN}`, so this is only a safety net.
const STDERR_GRACE: Duration = Duration::from_secs(2);
/// Polls (25 ms apart) for a clean exit after `-stay_open False` before killing the process.
const SHUTDOWN_POLLS: u32 = 20;

/// How to start ExifTool on this platform.
#[derive(Clone, Debug)]
pub struct Launch {
    /// Perl interpreter (Windows: bundled perl.exe; Android: libperl.so from nativeLibraryDir).
    pub perl: PathBuf,
    pub script: PathBuf,
    pub cwd: PathBuf,
    /// Extra environment (e.g. PERL5LIB on Android).
    pub env: Vec<(String, String)>,
}

impl Launch {
    /// Launch settings for an engine unpacked into `dir`.
    #[cfg(windows)]
    pub fn for_dir(dir: &Path) -> Self {
        Self { perl: dir.join("perl.exe"), script: dir.join("exiftool.pl"), cwd: dir.to_path_buf(), env: vec![] }
    }

    /// Launch settings for an engine unpacked into `dir`. Android: perl from the app's native
    /// library dir with ExifTool's `lib/` extracted next to the script (see DECISIONS.md).
    #[cfg(not(windows))]
    pub fn for_dir(dir: &Path) -> Self {
        Self {
            perl: dir.join("perl"),
            script: dir.join("exiftool"),
            cwd: dir.to_path_buf(),
            env: vec![("PERL5LIB".into(), dir.join("lib").to_string_lossy().into_owned())],
        }
    }
}

pub struct Output {
    pub stdout: String,
    pub stderr: String,
}

struct Proc {
    child: Child,
    stdin: ChildStdin,
    out: Receiver<String>,
    err: Receiver<String>,
}

/// A persistent ExifTool process. Commands are serialised; the process starts lazily.
pub struct Engine {
    launch: Launch,
    proc: Mutex<Option<Proc>>,
    seq: AtomicU64,
    writable: OnceLock<HashSet<String>>,
}

impl Engine {
    pub fn new(launch: Launch) -> Self {
        Self { launch, proc: Mutex::new(None), seq: AtomicU64::new(1), writable: OnceLock::new() }
    }

    fn spawn(&self) -> Result<Proc, String> {
        let mut cmd = Command::new(&self.launch.perl);
        cmd.arg(&self.launch.script)
            .args(["-stay_open", "True", "-@", "-", "-common_args"])
            .args(COMMON_ARGS)
            .current_dir(&self.launch.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_remove("PERL5LIB")
            .env_remove("PERLLIB")
            .env_remove("PERL5OPT")
            .env_remove("EXIFTOOL_HOME");
        for (k, v) in &self.launch.env {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| format!("Could not start the metadata engine: {e}"))?;
        let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            return Err("Could not connect to the metadata engine.".into());
        };
        Ok(Proc { child, stdin, out: spawn_reader(stdout), err: spawn_reader(stderr) })
    }

    /// Runs one ExifTool command and returns its stdout and stderr. If the process died, it is
    /// restarted and the command retried once (reads are idempotent and writes always target a
    /// fresh temp file, so a retry is safe).
    pub fn run(&self, args: &[String]) -> Result<Output, String> {
        let mut guard = self.proc.lock().unwrap_or_else(|p| p.into_inner());
        let mut last_err = String::new();
        for _ in 0..2 {
            let proc = match guard.as_mut() {
                Some(p) => p,
                None => guard.insert(self.spawn()?),
            };
            match self.run_on(proc, args) {
                Ok(o) => return Ok(o),
                Err(e) => {
                    if let Some(mut dead) = guard.take() {
                        let _ = dead.child.kill();
                    }
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    fn run_on(&self, p: &mut Proc, args: &[String]) -> Result<Output, String> {
        let n = self.seq.fetch_add(1, Ordering::SeqCst);
        let (payload, marker) = command_payload(args, n);
        p.stdin.write_all(payload.as_bytes()).map_err(|_| ENGINE_STOPPED.to_string())?;
        p.stdin.flush().map_err(|_| ENGINE_STOPPED.to_string())?;
        let stdout = collect_until(&p.out, &marker, None)?;
        let stderr = collect_until(&p.err, &marker, Some(STDERR_GRACE))?;
        Ok(Output { stdout, stderr })
    }

    /// Reads metadata of files as tag maps keyed `Group0:Group1:Tag` (numeric values, `-n`).
    pub fn read(&self, files: &[&Path], extra: &[&str]) -> Result<Vec<TagMap>, String> {
        if files.is_empty() {
            return Ok(vec![]);
        }
        let mut args: Vec<String> = ["-json", "-a", "-G0:1", "-n"].iter().chain(extra).map(|s| s.to_string()).collect();
        args.extend(files.iter().map(|f| path_arg(f)));
        let out = self.run(&args)?;
        let text = out.stdout.trim();
        if text.is_empty() {
            let msg = out.stderr.trim();
            return Err(if msg.is_empty() { "The file could not be read.".into() } else { msg.to_string() });
        }
        let parsed: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("Unexpected engine output: {e}"))?;
        match parsed {
            serde_json::Value::Array(items) => Ok(items.into_iter().map(TagMap::from_json).collect()),
            _ => Err("Unexpected engine output.".into()),
        }
    }

    /// Reads one file; errors if the engine returned nothing for it.
    pub fn read_one(&self, file: &Path, extra: &[&str]) -> Result<TagMap, String> {
        self.read(&[file], extra)?.into_iter().next().ok_or_else(|| "The file could not be read.".to_string())
    }

    /// Lower-case names of every writable tag (`exiftool -listw`), cached for the session.
    /// Empty if the list couldn't be read; callers then treat every tag as writable.
    pub fn writable_tags(&self) -> &HashSet<String> {
        self.writable.get_or_init(|| {
            let Ok(out) = self.run(&["-listw".into()]) else { return HashSet::new() };
            out.stdout.lines().skip(1).flat_map(str::split_whitespace).map(str::to_ascii_lowercase).collect()
        })
    }

    /// ExifTool version string, e.g. `13.63`.
    pub fn version(&self) -> Result<String, String> {
        Ok(self.run(&["-ver".into()])?.stdout.trim().to_string())
    }

    /// Asks the process to exit, killing it if it doesn't within about half a second.
    pub fn shutdown(&self) {
        let mut guard = self.proc.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(mut p) = guard.take() {
            let _ = p.stdin.write_all(b"-stay_open\nFalse\n");
            let _ = p.stdin.flush();
            for _ in 0..SHUTDOWN_POLLS {
                if let Ok(Some(_)) = p.child.try_wait() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            let _ = p.child.kill();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Forwards lines from a pipe to a channel on a background thread (lossy UTF-8, no line ending).
fn spawn_reader<R: std::io::Read + Send + 'static>(r: R) -> Receiver<String> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(r);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}
