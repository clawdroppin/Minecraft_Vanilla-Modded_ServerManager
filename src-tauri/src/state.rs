use crate::error::{AppError, AppResult};
use crate::models::*;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock};

/// Volatile, per-instance runtime bookkeeping (never persisted).
#[derive(Debug, Default)]
pub struct Runtime {
    pub pid: Option<u32>,
    pub state: Option<ServerState>,
    pub started_at: Option<i64>,
    pub ready_at: Option<i64>,
    pub boot_secs: Option<f64>,
    pub intentional_stop: bool,
    pub crash_times: Vec<i64>,
    pub crash_loop: bool,
    /// Short label of a long operation in progress (backup, install …) – shown in the UI.
    pub busy: Option<String>,
    /// Settings changed while running → they take effect on the next restart.
    pub pending_restart: bool,
    pub running_version: Option<String>,
    pub last_backup: i64,
    /// Someone was online since the last interval backup.
    pub played_since_backup: bool,
    pub last_update_check: i64,
    pub update: Option<UpdateInfo>,
    pub last_exit_code: Option<i64>,
    pub fired_schedule_keys: HashSet<String>,
    pub next_schedule: Option<NextSchedule>,
    /// Pending countdown (restart/stop with in-game warnings).
    pub countdown: Option<NextSchedule>,
    /// Auto-restart scheduled after a crash (ms timestamp).
    pub restart_at: Option<i64>,
    /// Console log tail position.
    pub log_offset: u64,
    pub log_partial: String,
    /// Players online (name -> info) from the log and RCON `list`.
    pub online: BTreeMap<String, OnlinePlayer>,
    pub uuids: HashMap<String, String>,
    pub ips: HashMap<String, String>,
    pub max_players: Option<u32>,
    pub tps: Option<f64>,
    pub mspt: Option<f64>,
    /// Last time somebody was online (for "restart when empty").
    pub last_player_seen: i64,
    /// The log reported "Done (…)! For help…".
    pub log_ready: bool,
    /// Lines collected while a backup waits for "Saved the game".
    pub watch: Vec<String>,
    pub watching: bool,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub portable: bool,
    pub settings: RwLock<AppSettings>,
    pub instances: RwLock<Vec<ServerInstance>>,
    pub runtime: Mutex<HashMap<String, Runtime>>,
    /// stdin pipes of servers this manager launched (commands go here first).
    pub stdins: Mutex<HashMap<String, std::process::ChildStdin>>,
    /// Exit codes of children we launched (filled by their waiter threads).
    pub exits: Arc<std::sync::Mutex<HashMap<String, i64>>>,
    pub http: reqwest::Client,
    /// Cancellation flags for long-running tasks, keyed by task id.
    pub cancels: Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>,
    /// Version catalogs and other slow lookups: key -> (fetched at, JSON).
    pub cache: Mutex<HashMap<String, (i64, serde_json::Value)>>,
    /// Serialises lifecycle operations per server (start/stop/restart/install never overlap).
    pub op_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// One Java download at a time.
    pub java_lock: Mutex<()>,
}

impl AppState {
    pub fn load(data_dir: PathBuf, portable: bool) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        let mut settings: AppSettings = load_json_safe(&data_dir.join("settings.json"));
        let mut instances: Vec<ServerInstance> = load_json_safe(&data_dir.join("instances.json"));
        // A portable folder may have been moved or opened from another drive letter: re-root every
        // stored path that pointed inside the old data folder.
        if let Some(old) = settings.last_data_dir.clone().filter(|o| o != &data_dir) {
            let fix = |p: &mut PathBuf| {
                if let Some(n) = crate::paths::rebase(p, &old, &data_dir) {
                    *p = n;
                }
            };
            for d in [&mut settings.java_dir, &mut settings.default_install_root, &mut settings.backup_dir].into_iter().flatten() {
                fix(d);
            }
            for i in instances.iter_mut() {
                fix(&mut i.install_dir);
            }
            let _ = write_json_atomic(&data_dir.join("instances.json"), &instances);
        }
        if settings.last_data_dir.as_ref() != Some(&data_dir) {
            settings.last_data_dir = Some(data_dir.clone());
            let _ = write_json_atomic(&data_dir.join("settings.json"), &settings);
        }
        let http = reqwest::Client::builder()
            .user_agent(concat!("Minecraft-Server-Manager/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(15))
            // A stalled transfer (no bytes for a minute) fails instead of hanging forever.
            .read_timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("http client");
        Self {
            data_dir,
            portable,
            settings: RwLock::new(settings),
            instances: RwLock::new(instances),
            runtime: Mutex::new(HashMap::new()),
            stdins: Mutex::new(HashMap::new()),
            exits: Arc::new(std::sync::Mutex::new(HashMap::new())),
            http,
            cancels: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
            op_locks: Mutex::new(HashMap::new()),
            java_lock: Mutex::new(()),
        }
    }

    pub async fn persist_instances(&self) -> AppResult<()> {
        let list = self.instances.read().await.clone();
        write_json_atomic(&self.data_dir.join("instances.json"), &list)
    }

    pub async fn persist_settings(&self) -> AppResult<()> {
        let s = self.settings.read().await.clone();
        write_json_atomic(&self.data_dir.join("settings.json"), &s)
    }

    pub async fn instance(&self, id: &str) -> AppResult<ServerInstance> {
        self.instances
            .read()
            .await
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or_else(|| AppError::InstanceNotFound(id.to_string()))
    }

    pub async fn op_lock(&self, id: &str) -> Arc<Mutex<()>> {
        self.op_locks.lock().await.entry(id.to_string()).or_default().clone()
    }

    /// Where managed Java runtimes live.
    pub async fn java_dir(&self) -> PathBuf {
        self.settings.read().await.java_dir.clone().unwrap_or_else(|| self.data_dir.join("java"))
    }

    /// Cached JSON lookup (`ttl_ms`); falls back to a stale value (memory, then disk) when offline.
    pub async fn cached_json(&self, key: &str, url: &str, ttl_ms: i64) -> AppResult<serde_json::Value> {
        if let Some((t, v)) = self.cache.lock().await.get(key).cloned() {
            if now_ms() - t < ttl_ms {
                return Ok(v);
            }
        }
        let safe: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
        let disk = self.data_dir.join("cache").join(format!("{safe}.json"));
        let fetched: AppResult<serde_json::Value> = async {
            let r = self.http.get(url).timeout(std::time::Duration::from_secs(25)).send().await?.error_for_status()?;
            Ok(r.json::<serde_json::Value>().await?)
        }
        .await;
        match fetched {
            Ok(v) => {
                self.cache.lock().await.insert(key.to_string(), (now_ms(), v.clone()));
                if let Some(p) = disk.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                let _ = std::fs::write(&disk, serde_json::to_vec(&v).unwrap_or_default());
                Ok(v)
            }
            Err(e) => {
                if let Some((_, v)) = self.cache.lock().await.get(key).cloned() {
                    return Ok(v);
                }
                if let Some(v) = read_json::<serde_json::Value>(&disk) {
                    return Ok(v);
                }
                Err(e)
            }
        }
    }

    /// Drop a cached lookup so the next call fetches fresh data.
    pub async fn invalidate(&self, prefix: &str) {
        self.cache.lock().await.retain(|k, _| !k.starts_with(prefix));
    }

    /// Where a server's backups go: `<server folder>ackups` by default, or
    /// `<override>\<server id>` when a global backup folder is set in App settings.
    pub async fn backup_dir(&self, inst: &ServerInstance) -> PathBuf {
        match self.settings.read().await.backup_dir.clone() {
            Some(root) => root.join(&inst.id),
            None => inst.install_dir.join("backups"),
        }
    }

    pub async fn default_install_root(&self) -> PathBuf {
        self.settings
            .read()
            .await
            .default_install_root
            .clone()
            .unwrap_or_else(|| self.data_dir.join("servers"))
    }

    pub async fn with_runtime<R>(&self, id: &str, f: impl FnOnce(&mut Runtime) -> R) -> R {
        let mut map = self.runtime.lock().await;
        f(map.entry(id.to_string()).or_default())
    }

    pub async fn status(&self, inst: &ServerInstance) -> ServerStatus {
        let has_stdin = self.stdins.lock().await.contains_key(&inst.id);
        let map = self.runtime.lock().await;
        let rt = map.get(&inst.id);
        let installed = inst.is_installed();
        let state = match rt.and_then(|r| r.state) {
            Some(s) => s,
            None if !installed => ServerState::NotInstalled,
            None => ServerState::Stopped,
        };
        let state = if matches!(state, ServerState::Stopped | ServerState::Crashed) && !installed {
            ServerState::NotInstalled
        } else {
            state
        };
        let live = matches!(state, ServerState::Running | ServerState::Starting);
        ServerStatus {
            id: inst.id.clone(),
            state,
            pid: rt.and_then(|r| r.pid),
            started_at: rt.and_then(|r| r.started_at),
            ready_at: rt.and_then(|r| r.ready_at),
            boot_secs: rt.and_then(|r| r.boot_secs),
            running_version: rt.and_then(|r| r.running_version.clone()),
            crash_count: rt.map(|r| r.crash_times.len() as u32).unwrap_or(0),
            crash_loop: rt.map(|r| r.crash_loop).unwrap_or(false),
            pending_restart: rt.map(|r| r.pending_restart).unwrap_or(false),
            players: rt.map(|r| r.online.values().cloned().collect()).unwrap_or_default(),
            max_players: rt.and_then(|r| r.max_players),
            tps: rt.and_then(|r| r.tps),
            mspt: rt.and_then(|r| r.mspt),
            next_schedule: rt.and_then(|r| r.next_schedule.clone()),
            last_backup: rt.map(|r| r.last_backup).filter(|t| *t > 0),
            last_exit_code: rt.and_then(|r| r.last_exit_code),
            busy: rt.and_then(|r| r.busy.clone()),
            update: rt.and_then(|r| r.update.clone()),
            can_command: live && (has_stdin || inst.network.rcon_enabled),
            countdown: rt.and_then(|r| r.countdown.clone()),
        }
    }

    pub async fn emit_status(&self, app: &AppHandle, id: &str) {
        if let Ok(inst) = self.instance(id).await {
            let st = self.status(&inst).await;
            let _ = app.emit("server-status", st);
        }
    }

    pub async fn set_busy(&self, app: &AppHandle, id: &str, label: Option<&str>) {
        self.with_runtime(id, |r| r.busy = label.map(|s| s.to_string())).await;
        self.emit_status(app, id).await;
    }
}

/// Emit a toast-style event for one server (and forward it to the webhook when configured).
pub fn notify(app: &AppHandle, id: &str, level: &str, message: impl Into<String>) {
    let message = message.into();
    let _ = app.emit(
        "server-event",
        ServerEvent { id: id.to_string(), level: level.to_string(), message: message.clone(), ts: now_ms() },
    );
    use tauri::Manager;
    if let Some(state) = app.try_state::<Arc<AppState>>() {
        let state = state.inner().clone();
        let (id, level) = (id.to_string(), level.to_string());
        tauri::async_runtime::spawn(async move {
            let s = state.settings.read().await.clone();
            if s.webhook_url.trim().is_empty() || !s.webhook_levels.iter().any(|l| l == &level) {
                return;
            }
            let name = state.instance(&id).await.map(|i| i.name).unwrap_or_default();
            let _ = send_webhook(&state.http, s.webhook_url.trim(), &level, &name, &message).await;
        });
    }
}

/// POST a message in a shape Discord (`content`) and Slack (`text`) both accept.
pub async fn send_webhook(http: &reqwest::Client, url: &str, level: &str, server: &str, message: &str) -> AppResult<()> {
    let icon = match level {
        "success" => "\u{1F7E2}",
        "error" => "\u{1F534}",
        "warn" => "\u{1F7E0}",
        _ => "\u{1F535}",
    };
    let text = if server.is_empty() { format!("{icon} {message}") } else { format!("{icon} **{server}** \u{2013} {message}") };
    let body = serde_json::json!({ "content": text, "text": text, "username": "Minecraft Server Manager" });
    let r = http.post(url).json(&body).timeout(std::time::Duration::from_secs(10)).send().await?;
    if !r.status().is_success() {
        return Err(AppError::msg(format!("Webhook returned HTTP {}", r.status())));
    }
    Ok(())
}

/// Emit a manager line into a server's console stream.
pub fn manager_log(app: &AppHandle, id: &str, line: impl Into<String>) {
    let _ = app.emit(
        "log-lines",
        LogBatch { id: id.to_string(), source: "manager".into(), lines: vec![line.into()], ts: now_ms() },
    );
}

/// Small helper to stream progress for long running jobs to the UI.
#[derive(Clone)]
pub struct TaskReporter {
    pub app: AppHandle,
    pub task_id: String,
    pub instance_id: Option<String>,
    pub kind: String,
    pub title: String,
    /// When set, raw output lines are not forwarded to the log stream.
    pub quiet: bool,
}

impl TaskReporter {
    pub fn new(app: &AppHandle, kind: &str, title: &str, instance_id: Option<&str>) -> Self {
        Self {
            app: app.clone(),
            task_id: uuid::Uuid::new_v4().to_string(),
            instance_id: instance_id.map(|s| s.to_string()),
            kind: kind.to_string(),
            title: title.to_string(),
            quiet: false,
        }
    }
    fn send(&self, message: &str, progress: Option<f64>, done: bool, error: Option<String>) {
        let _ = self.app.emit(
            "task",
            TaskEvent {
                task_id: self.task_id.clone(),
                instance_id: self.instance_id.clone(),
                kind: self.kind.clone(),
                title: self.title.clone(),
                message: message.to_string(),
                progress,
                done,
                error,
                silent: false,
            },
        );
    }
    pub fn progress(&self, message: &str, progress: Option<f64>) {
        self.send(message, progress, false, None)
    }
    pub fn done(&self, message: &str) {
        self.send(message, Some(1.0), true, None)
    }
    /// Finish without a toast (the task may never have shown anything).
    pub fn done_silent(&self) {
        let _ = self.app.emit(
            "task",
            TaskEvent {
                task_id: self.task_id.clone(),
                instance_id: self.instance_id.clone(),
                kind: self.kind.clone(),
                title: self.title.clone(),
                message: String::new(),
                progress: Some(1.0),
                done: true,
                error: None,
                silent: true,
            },
        );
    }
    pub fn fail(&self, error: &str) {
        self.send(error, None, true, Some(error.to_string()))
    }
    pub fn log(&self, line: &str) {
        if self.quiet {
            return;
        }
        let _ = self.app.emit(
            "log-lines",
            LogBatch {
                id: self.instance_id.clone().unwrap_or_default(),
                source: self.kind.clone(),
                lines: vec![line.to_string()],
                ts: chrono::Utc::now().timestamp_millis(),
            },
        );
    }
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Load a JSON file without ever losing data: a file that exists but can't be parsed is kept as
/// `<name>.corrupt-<timestamp>` and the last good `.bak` copy is used instead.
pub fn load_json_safe<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    if !path.exists() {
        return read_json(&path.with_extension("json.bak")).unwrap_or_default();
    }
    if let Some(v) = read_json(path) {
        return v;
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let corrupt = path.with_extension(format!("json.corrupt-{stamp}"));
    let _ = std::fs::rename(path, &corrupt);
    eprintln!("{} was unreadable; kept as {} and restored from backup", path.display(), corrupt.display());
    read_json(&path.with_extension("json.bak")).unwrap_or_default()
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Keep the previous good version as a one-step backup.
    if path.exists() && read_json::<serde_json::Value>(path).is_some() {
        let _ = std::fs::copy(path, path.with_extension("json.bak"));
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_file_is_preserved_and_backup_used() {
        let dir = std::env::temp_dir().join(format!("mcsm-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("instances.json");
        write_json_atomic(&p, &vec![1, 2, 3]).unwrap();
        write_json_atomic(&p, &vec![4, 5]).unwrap(); // .bak now holds [1,2,3]
        std::fs::write(&p, "{ not json").unwrap();
        let v: Vec<i32> = load_json_safe(&p);
        assert_eq!(v, vec![1, 2, 3]);
        let kept = std::fs::read_dir(&dir).unwrap().flatten().any(|e| e.file_name().to_string_lossy().contains("corrupt-"));
        assert!(kept, "corrupt file must be kept");
        let _ = std::fs::remove_dir_all(dir);
    }
}
