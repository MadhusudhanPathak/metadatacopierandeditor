//! Shared app state: the engine (started in the background), the metadata cache, the journal,
//! the latest Preview/Review plans, the place database and the run flags.

use crate::engine::{Engine, Launch, extract};
use crate::geo;
use crate::metadata::cache::MetaCache;
use crate::ops::journal::Journal;
use crate::planning::copy::CopyPlan;
use crate::planning::edit::EditPlan;
use crate::settings::{self, Settings};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

pub struct AppState {
    /// App data folder (engine, journal, settings).
    pub base: PathBuf,
    /// None while the engine is starting; then the engine or why it failed.
    engine: Mutex<Option<Result<Arc<Engine>, String>>>,
    engine_ready: Condvar,
    engine_dir: Mutex<Option<PathBuf>>,
    places: OnceLock<Result<geo::Db, String>>,
    pub meta: MetaCache,
    pub journal: Journal,
    /// Only the latest plan of each flow can be applied.
    copy_plan: Mutex<Option<CopyPlan>>,
    edit_plan: Mutex<Option<EditPlan>>,
    /// Set by the user's Cancel; checked between items.
    pub cancel: AtomicBool,
    busy: AtomicBool,
    pub settings: Mutex<Settings>,
}

/// Holds the "an operation is running" flag; released when dropped.
pub struct BusyGuard(Arc<AppState>);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::SeqCst);
    }
}

impl AppState {
    pub fn new(base: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&base);
        Self {
            journal: Journal::new(&base),
            settings: Mutex::new(settings::load(&base)),
            base,
            engine: Mutex::new(None),
            engine_ready: Condvar::new(),
            engine_dir: Mutex::new(None),
            places: OnceLock::new(),
            meta: MetaCache::default(),
            copy_plan: Mutex::new(None),
            edit_plan: Mutex::new(None),
            cancel: AtomicBool::new(false),
            busy: AtomicBool::new(false),
        }
    }

    /// Unpacks (first launch) and starts ExifTool in the background so the UI shows at once.
    pub fn start_engine(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || {
            let res = (|| -> Result<Arc<Engine>, String> {
                let dir = extract::ensure_engine(&me.base)?;
                *me.engine_dir.lock().unwrap() = Some(dir.clone());
                let engine = Arc::new(Engine::new(Launch::for_dir(&dir)));
                engine.version()?;
                // Warm the writable-tag list so the first Preview is fast.
                let _ = engine.writable_tags();
                Ok(engine)
            })();
            *me.engine.lock().unwrap() = Some(res);
            me.engine_ready.notify_all();
        });
    }

    /// The engine, waiting for it to finish starting if necessary.
    pub fn engine(&self) -> Result<Arc<Engine>, String> {
        let mut g = self.engine.lock().unwrap();
        loop {
            if let Some(res) = g.as_ref() {
                return res.clone();
            }
            g = self.engine_ready.wait(g).unwrap();
        }
    }

    /// None while starting, otherwise whether the engine is usable.
    pub fn engine_status(&self) -> Option<Result<(), String>> {
        self.engine.lock().unwrap().as_ref().map(|r| r.as_ref().map(|_| ()).map_err(Clone::clone))
    }

    /// The offline place database from the engine folder, loaded on first use.
    pub fn places(&self) -> Result<&geo::Db, String> {
        self.engine()?;
        let dir = self.engine_dir.lock().unwrap().clone().ok_or("The place list isn't ready yet.")?;
        self.places.get_or_init(|| geo::Db::load(&geo::db_path(&dir))).as_ref().map_err(Clone::clone)
    }

    /// Claims the single operation slot (Apply, Undo or recovery).
    pub fn begin_operation(self: &Arc<Self>) -> Result<BusyGuard, String> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err("Another operation is still running. Wait for it to finish first.".into());
        }
        Ok(BusyGuard(self.clone()))
    }

    /// Makes `plan` the one `run_copy` will accept.
    pub fn set_copy_plan(&self, plan: CopyPlan) {
        *self.copy_plan.lock().unwrap() = Some(plan);
    }

    /// The latest Copy plan, if `plan_id` still matches it.
    pub fn copy_plan(&self, plan_id: &str) -> Option<CopyPlan> {
        self.copy_plan.lock().unwrap().as_ref().filter(|p| p.plan_id == plan_id).cloned()
    }

    /// Makes `plan` the one `run_edit` will accept.
    pub fn set_edit_plan(&self, plan: EditPlan) {
        *self.edit_plan.lock().unwrap() = Some(plan);
    }

    /// The latest Edit plan, if `plan_id` still matches it.
    pub fn edit_plan(&self, plan_id: &str) -> Option<EditPlan> {
        self.edit_plan.lock().unwrap().as_ref().filter(|p| p.plan_id == plan_id).cloned()
    }

    /// Stops the engine process (on app exit).
    pub fn shutdown(&self) {
        if let Some(Ok(e)) = self.engine.lock().unwrap().as_ref() {
            e.shutdown();
        }
    }
}
