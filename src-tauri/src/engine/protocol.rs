//! ExifTool `-stay_open` wire format: how arguments are written to stdin and how replies are
//! delimited and interpreted (T5).

use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// Arguments applied to every command (`-common_args`).
pub const COMMON_ARGS: &[&str] = &["-ec", "-charset", "filename=UTF8", "-charset", "UTF8", "-api", "LargeFileSupport=1", "-api", "WindowsLongPath=1"];

pub const ENGINE_STOPPED: &str = "The metadata engine stopped unexpectedly.";

/// Length of the `-TAG=` / `-TAG+=` / `-TAG#=` prefix if `arg` is a tag assignment.
fn assignment_prefix(arg: &str) -> Option<usize> {
    let b = arg.as_bytes();
    if b.first() != Some(&b'-') {
        return None;
    }
    let mut i = 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'-' | b'_' | b':' | b'*' | b'?')) {
        i += 1;
    }
    if i == 1 {
        return None;
    }
    if i < b.len() && b[i] == b'#' {
        i += 1;
    }
    if i < b.len() && matches!(b[i], b'+' | b'-' | b'<') {
        i += 1;
    }
    (i < b.len() && b[i] == b'=').then_some(i + 1)
}

/// Encodes one argument as an argfile line. Tag values are C-escaped and the engine runs with
/// `-ec`, which reverses the escaping when writing, so newlines, tabs and backslashes survive.
/// Everything else (paths, `@`, `$`) passes through untouched. ExifTool's `#[CSTR]` lines are
/// not used: they leave a stray backslash before `$` and `@`.
fn encode_arg(arg: &str) -> String {
    let mut s = String::with_capacity(arg.len() + 8);
    let (head, value) = match assignment_prefix(arg) {
        Some(n) => (&arg[..n], &arg[n..]),
        None => (arg, ""),
    };
    for c in head.chars() {
        s.push(if c.is_control() { ' ' } else { c });
    }
    for c in value.chars() {
        match c {
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if c.is_control() => {}
            _ => s.push(c),
        }
    }
    s.push('\n');
    s
}

/// The full stdin payload for one command: its arguments, then a marker echoed on stderr and
/// `-executeN`, which makes ExifTool print the same `{readyN}` marker on stdout.
pub fn command_payload(args: &[String], n: u64) -> (String, String) {
    let marker = format!("{{ready{n}}}");
    let mut payload: String = args.iter().map(|a| encode_arg(a)).collect();
    payload.push_str("-echo4\n");
    payload.push_str(&marker);
    payload.push('\n');
    payload.push_str(&format!("-execute{n}\n"));
    (payload, marker)
}

/// Collects output lines until `marker`. With a timeout, returns what arrived so far when it
/// expires. Late markers from earlier commands are skipped.
pub fn collect_until(rx: &Receiver<String>, marker: &str, timeout: Option<Duration>) -> Result<String, String> {
    let mut out = String::new();
    loop {
        let line = match timeout {
            None => rx.recv().map_err(|_| ENGINE_STOPPED.to_string())?,
            Some(t) => match rx.recv_timeout(t) {
                Ok(l) => l,
                Err(RecvTimeoutError::Timeout) => return Ok(out),
                Err(RecvTimeoutError::Disconnected) => return Err(ENGINE_STOPPED.into()),
            },
        };
        let trimmed = line.trim();
        if trimmed == marker {
            return Ok(out);
        }
        if trimmed.starts_with("{ready") && trimmed.ends_with('}') {
            continue;
        }
        out.push_str(&line);
        out.push('\n');
    }
}

/// Path as an ExifTool argument (forward slashes are fine on Windows too).
pub fn path_arg(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// Splits ExifTool stderr into (warnings, errors). Unprefixed lines count as warnings.
pub fn split_messages(stderr: &str) -> (Vec<String>, Vec<String>) {
    let mut warnings = vec![];
    let mut errors = vec![];
    for line in stderr.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(rest) = line.strip_prefix("Error:") {
            errors.push(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Warning:") {
            warnings.push(rest.trim().to_string());
        } else {
            warnings.push(line.to_string());
        }
    }
    (warnings, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argument_encoding() {
        // Paths and options pass through untouched (backslashes, @, $).
        assert_eq!(encode_arg(r"C:\Photos\$5 @home\a.jpg"), "C:\\Photos\\$5 @home\\a.jpg\n");
        assert_eq!(encode_arg("@"), "@\n");
        assert_eq!(encode_arg("-tagsFromFile"), "-tagsFromFile\n");
        // Tag values are C-escaped (reversed by -ec).
        assert_eq!(encode_arg("-XMP-dc:Description=line1\nline2"), "-XMP-dc:Description=line1\\nline2\n");
        assert_eq!(encode_arg("-Title=a\\b $x @y"), "-Title=a\\\\b $x @y\n");
        assert_eq!(encode_arg("-DateTimeOriginal+=0:0:1 2:0:0"), "-DateTimeOriginal+=0:0:1 2:0:0\n");
        assert_eq!(encode_arg("-GPS:GPSAltitudeRef#=0"), "-GPS:GPSAltitudeRef#=0\n");
        assert_eq!(encode_arg("-Title=emoji 📷 ü"), "-Title=emoji 📷 ü\n");
        assert_eq!(assignment_prefix("--IFD0:Make"), None);
        assert_eq!(assignment_prefix("-all="), Some(5));
    }

    #[test]
    fn payload_is_delimited_by_marker() {
        let (payload, marker) = command_payload(&["-ver".into()], 7);
        assert_eq!(marker, "{ready7}");
        assert_eq!(payload, "-ver\n-echo4\n{ready7}\n-execute7\n");
    }

    #[test]
    fn collects_until_marker_and_skips_stale_ones() {
        let (tx, rx) = std::sync::mpsc::channel();
        for l in ["a", "{ready3}", "b", "{ready4}", "c"] {
            tx.send(l.to_string()).unwrap();
        }
        assert_eq!(collect_until(&rx, "{ready4}", None).unwrap(), "a\nb\n");
        assert_eq!(collect_until(&rx, "{ready5}", Some(Duration::from_millis(10))).unwrap(), "c\n");
    }

    #[test]
    fn splits_messages() {
        let (w, e) = split_messages("Warning: minor thing\nError: bad file\n\n");
        assert_eq!(w, vec!["minor thing"]);
        assert_eq!(e, vec!["bad file"]);
    }
}
