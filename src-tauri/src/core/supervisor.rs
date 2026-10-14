//! The single background loop: process tracking (servers survive manager restarts and are
//! re-adopted by their `-Dmcsm.id` marker), state transitions, crash detection & auto-restart,
//! telemetry, console tail with player/chat parsing, RCON metrics (player list, TPS),
//! countdown restarts with in-game warnings, schedules, interval backups, build-update checks
//! and the new-Minecraft-release watcher.

use super::{backup, mcproto, server, versions};
use crate::models::*;
use crate::state::{manager_log, notify, now_ms, AppState};
use chrono::{Datelike, Local, NaiveTime, TimeZone};
use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use sysinfo::{Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager};

const NAME: &str = r"([A-Za-z0-9_.*\-]{1,32})";
static READY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"Done \(([0-9.,]+)s\)! For help, type"#).unwrap());
static VERSION_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Starting minecraft server version (.+?)\s*$").unwrap());
static UUID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"UUID of player {NAME} is ([0-9a-fA-F-]{{32,36}})")).unwrap());
static LOGIN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\]: {NAME}\[/([^\]]+)\] logged in with entity id")).unwrap());
static JOIN_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\]: {NAME} joined the game")).unwrap());
static LEFT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\]: {NAME} left the game")).unwrap());
static CHAT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\]: (?:\[Not Secure\] )?<{NAME}> (.*)$")).unwrap());
static ADV_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"\]: {NAME} has (?:made the advancement|completed the challenge|reached the goal) \[(.+)\]")).unwrap());
static STOPPING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\]: Stopping (?:the )?server").unwrap());
static FATAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(FAILED TO BIND TO PORT|You need to agree to the EULA|UnsupportedClassVersionError|Could not reserve enough space|java\.lang\.OutOfMemoryError|Incompatible mods found|Mod resolution failed|Error: Unable to access jarfile|Error: Could not find or load main class|requires version \d+ of '[^']*' \(java\))").unwrap()
});
static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap());

const CRASH_WINDOW_MS: i64 = 15 * 60 * 1000;
const MAX_LINES_PER_TICK: usize = 500;
const METRICS_EVERY_MS: i64 = 10_000;

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state: Arc<AppState> = app.state::<Arc<AppState>>().inner().clone();
        let mut sup = Supervisor::new();
        sup.boot(&state, &app).await;
        loop {
            let interval = state.settings.read().await.telemetry_interval_ms.clamp(500, 5000);
            let t0 = Instant::now();
            sup.tick(&state, &app).await;
            tokio::time::sleep(Duration::from_millis(interval).saturating_sub(t0.elapsed())).await;
        }
    });
}

struct Supervisor {
    sys: System,
    nets: Networks,
    last_tick: Instant,
    last_status: HashMap<String, String>,
    over_mem: HashMap<String, u32>,
    since_scan: u32,
    /// (pid, profile id) of every manager-launched server.
    tracked: Vec<(Pid, String)>,
    /// Persistent RCON connections for metrics, and when a failed one may be retried.
    rcon: HashMap<String, mcproto::Rcon>,
    rcon_retry: HashMap<String, i64>,
    last_metrics: HashMap<String, i64>,
    no_tps: HashSet<String>,
    last_version_check: i64,
}

impl Supervisor {
    fn new() -> Self {
        Self {
            sys: System::new(),
            nets: Networks::new_with_refreshed_list(),
            last_tick: Instant::now(),
            last_status: HashMap::new(),
            over_mem: HashMap::new(),
            since_scan: u32::MAX,
            tracked: vec![],
            rcon: HashMap::new(),
            rcon_retry: HashMap::new(),
            last_metrics: HashMap::new(),
            no_tps: HashSet::new(),
            last_version_check: 0,
        }
    }

    async fn boot(&mut self, state: &Arc<AppState>, app: &AppHandle) {
        let instances = state.instances.read().await.clone();
        for inst in &instances {
            let last_bk = backup::list(&state.backup_dir(inst).await).first().map(|b| b.created_at).unwrap_or(0);
            state
                .with_runtime(&inst.id, |r| {
                    r.last_backup = last_bk;
                    // First update check a minute after launch.
                    r.last_update_check = now_ms() - (inst.automation.update_check_mins as i64 * 60_000) + 60_000;
                })
                .await;
        }
        self.last_version_check = now_ms() - 3_600_000;
        self.sys.refresh_cpu_usage();
        self.refresh_procs();
        tokio::time::sleep(Duration::from_millis(300)).await;
        for inst in instances.into_iter().filter(|i| i.automation.auto_start && i.is_installed()) {
            if self.tracked.iter().any(|(_, id)| *id == inst.id) {
                continue;
            }
            let (st, app2) = (state.clone(), app.clone());
            tauri::async_runtime::spawn(async move {
                manager_log(&app2, &inst.id, "Auto-starting with the manager…");
                if let Err(e) = server::start(&st, &app2, &inst.id).await {
                    notify(&app2, &inst.id, "error", format!("Auto-start of {} failed: {e}", inst.name));
                }
            });
        }
    }

    /// Full discovery (java processes + their command lines) every 3 ticks; CPU/memory/disk only
    /// for tracked servers.
    fn refresh_procs(&mut self) {
        if self.since_scan >= 3 {
            self.tracked = super::proc::scan_java(&mut self.sys).into_iter().map(|p| (Pid::from_u32(p.pid), p.id)).collect();
            self.since_scan = 0;
        }
        self.since_scan += 1;
        if !self.tracked.is_empty() {
            let pids: Vec<Pid> = self.tracked.iter().map(|(p, _)| *p).collect();
            self.sys.refresh_processes_specifics(ProcessesToUpdate::Some(&pids), true, ProcessRefreshKind::nothing().with_cpu().with_memory().with_disk_usage());
            let sys = &self.sys;
            self.tracked.retain(|(p, _)| sys.process(*p).is_some());
        }
    }

    async fn tick(&mut self, state: &Arc<AppState>, app: &AppHandle) {
        let elapsed = self.last_tick.elapsed().as_secs_f64().max(0.2);
        self.last_tick = Instant::now();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        let any_starting = state.runtime.lock().await.values().any(|r| matches!(r.state, Some(ServerState::Starting) | Some(ServerState::Stopping)));
        if any_starting {
            self.since_scan = u32::MAX;
        }
        self.refresh_procs();
        self.nets.refresh(true);
        let cores = self.sys.cpus().len().max(1);
        let (rx, tx) = self.nets.iter().fold((0u64, 0u64), |(r, t), (_, d)| (r + d.received(), t + d.transmitted()));
        let system = SystemSample {
            cpu: self.sys.global_cpu_usage(),
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            rx_bps: (rx as f64 / elapsed) as u64,
            tx_bps: (tx as f64 / elapsed) as u64,
            cores,
            per_core: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
        };
        let now = now_ms();
        let instances = state.instances.read().await.clone();
        let mut samples = vec![];
        for inst in &instances {
            let pid = self.tracked.iter().find(|(_, id)| *id == inst.id).map(|(p, _)| p.as_u32());
            if let Some(p) = pid.and_then(|p| self.sys.process(Pid::from_u32(p))) {
                let du = p.disk_usage();
                let (players, tps) = state.with_runtime(&inst.id, |r| (r.online.len() as u32, r.tps)).await;
                let s = ProcSample {
                    id: inst.id.clone(),
                    cpu: p.cpu_usage() / cores as f32,
                    mem: p.memory(),
                    virt: p.virtual_memory(),
                    read_bps: (du.read_bytes as f64 / elapsed) as u64,
                    write_bps: (du.written_bytes as f64 / elapsed) as u64,
                    uptime: p.run_time(),
                    players,
                    tps,
                };
                self.resource_limits(state, app, inst, &s).await;
                samples.push(s);
            } else {
                self.over_mem.remove(&inst.id);
            }
            self.lifecycle(state, app, inst, pid, now).await;
            self.tail_log(state, app, inst).await;
            self.metrics(state, app, inst, now).await;
            self.countdown(state, app, inst, now).await;
            self.automation(state, app, inst, pid.is_some(), now).await;

            let st = state.status(inst).await;
            let key = serde_json::to_string(&st).unwrap_or_default();
            if self.last_status.get(&inst.id) != Some(&key) {
                self.last_status.insert(inst.id.clone(), key);
                let _ = app.emit("server-status", st);
            }
        }
        let _ = app.emit("telemetry", TelemetryEvent { ts: now, system, servers: samples });
        self.version_watch(state, app, now).await;
    }

    async fn lifecycle(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, pid: Option<u32>, now: i64) {
        let lock = state.op_lock(&inst.id).await;
        let Ok(_guard) = lock.try_lock() else { return };
        let alive = pid.is_some();
        let start_time = pid.and_then(|p| self.sys.process(Pid::from_u32(p))).map(|p| p.start_time() as i64 * 1000);
        let exit_code = if alive { None } else { state.exits.lock().ok().and_then(|m| m.get(&inst.id).copied()) };

        enum Ev {
            None,
            Adopted,
            Ready(Option<f64>),
            Crashed { restart_in: Option<u32>, looped: bool, code: Option<i64> },
            Stopped,
        }
        let ev = state
            .with_runtime(&inst.id, |r| {
                let cur = r.state.unwrap_or(ServerState::Stopped);
                match (alive, cur) {
                    (true, ServerState::Starting) => {
                        r.pid = pid;
                        let slow = r.started_at.map(|t| now - t > 600_000).unwrap_or(false);
                        if r.log_ready || slow {
                            r.state = Some(ServerState::Running);
                            r.ready_at = Some(now);
                            return Ev::Ready(r.boot_secs);
                        }
                        Ev::None
                    }
                    (true, ServerState::Running | ServerState::Stopping) => {
                        r.pid = pid;
                        Ev::None
                    }
                    (true, _) => {
                        // Started before the manager (re)opened: adopt it.
                        r.pid = pid;
                        r.state = Some(ServerState::Running);
                        r.started_at = start_time.or(Some(now));
                        r.ready_at = Some(now);
                        r.intentional_stop = false;
                        r.restart_at = None;
                        r.log_offset = 0;
                        r.log_partial.clear();
                        Ev::Adopted
                    }
                    (false, ServerState::Starting) if r.started_at.map(|t| now - t < 10_000).unwrap_or(false) => Ev::None,
                    (false, ServerState::Starting | ServerState::Running | ServerState::Stopping) => {
                        r.pid = None;
                        r.started_at = None;
                        r.ready_at = None;
                        r.online.clear();
                        r.tps = None;
                        r.mspt = None;
                        r.last_exit_code = exit_code;
                        if r.intentional_stop || cur == ServerState::Stopping {
                            r.state = Some(ServerState::Stopped);
                            r.countdown = None;
                            return Ev::Stopped;
                        }
                        r.state = Some(ServerState::Crashed);
                        r.crash_times.retain(|t| now - t < CRASH_WINDOW_MS);
                        r.crash_times.push(now);
                        let looped = inst.automation.crash_limit > 0 && r.crash_times.len() as u32 > inst.automation.crash_limit;
                        r.crash_loop = looped;
                        let restart_in = (inst.automation.auto_restart && !looped).then_some(inst.automation.restart_delay_secs);
                        if let Some(d) = restart_in {
                            r.restart_at = Some(now + d as i64 * 1000);
                        }
                        Ev::Crashed { restart_in, looped, code: exit_code }
                    }
                    _ => Ev::None,
                }
            })
            .await;
        match ev {
            Ev::Adopted => {
                manager_log(app, &inst.id, format!("Found the server already running (PID {}) – monitoring it again. Commands go over RCON.", pid.unwrap_or(0)));
                let (version, boot) = scan_log_meta(&inst.console_log());
                state
                    .with_runtime(&inst.id, |r| {
                        r.running_version = version;
                        r.boot_secs = boot;
                        r.log_ready = true;
                    })
                    .await;
                self.last_metrics.remove(&inst.id);
                self.no_tps.remove(&inst.id);
                self.rcon_retry.remove(&inst.id);
            }
            Ev::Ready(secs) => {
                let addr = format!("port {}", inst.network.port);
                manager_log(app, &inst.id, format!("Server is online on {addr}{}.", secs.map(|s| format!(" – started in {s:.1}s")).unwrap_or_default()));
                notify(app, &inst.id, "success", format!("{} is online", inst.name));
                absorb_properties(state, app, &inst.id).await;
                self.last_metrics.remove(&inst.id);
                // The software may have changed since the last run (e.g. old vanilla → Paper).
                self.no_tps.remove(&inst.id);
                self.rcon_retry.remove(&inst.id);
            }
            Ev::Crashed { restart_in, looped, code } => {
                let code_s = code.map(|c| format!(" (exit code {c})")).unwrap_or_default();
                let msg = match (restart_in, looped) {
                    (_, true) => format!("{} keeps crashing{code_s} – auto-restart paused. Check the console and Health.", inst.name),
                    (Some(d), _) => format!("{} stopped unexpectedly{code_s} – restarting in {d}s", inst.name),
                    (None, _) => format!("{} stopped unexpectedly{code_s}", inst.name),
                };
                manager_log(app, &inst.id, &msg);
                notify(app, &inst.id, "error", msg);
                state.stdins.lock().await.remove(&inst.id);
                self.rcon.remove(&inst.id);
            }
            Ev::Stopped => {
                state.stdins.lock().await.remove(&inst.id);
                self.rcon.remove(&inst.id);
                manager_log(app, &inst.id, "Server stopped.");
            }
            Ev::None => {}
        }
        if !alive {
            if let Ok(mut m) = state.exits.lock() {
                m.remove(&inst.id);
            }
        }

        let due = state
            .with_runtime(&inst.id, |r| match r.restart_at {
                Some(t) if t <= now && !alive => {
                    r.restart_at = None;
                    true
                }
                _ => false,
            })
            .await;
        if due {
            let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
            tauri::async_runtime::spawn(async move {
                manager_log(&app2, &id, "Auto-restarting after crash…");
                if let Err(e) = server::start(&st, &app2, &id).await {
                    notify(&app2, &id, "error", format!("Auto-restart failed: {e}"));
                }
            });
        }
    }

    async fn resource_limits(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, s: &ProcSample) {
        let a = &inst.automation;
        let (running, has_countdown) = state.with_runtime(&inst.id, |r| (r.state == Some(ServerState::Running), r.countdown.is_some())).await;
        if !running || has_countdown {
            return;
        }
        let mut reason = None;
        if a.memory_limit_mb > 0 && s.mem > a.memory_limit_mb as u64 * 1_048_576 {
            let n = self.over_mem.entry(inst.id.clone()).or_insert(0);
            *n += 1;
            if *n >= 30 {
                reason = Some(format!("memory above {} MB for 30 samples", a.memory_limit_mb));
            }
        } else {
            self.over_mem.remove(&inst.id);
        }
        if a.max_uptime_hours > 0 && s.uptime >= a.max_uptime_hours as u64 * 3600 {
            reason = Some(format!("uptime reached {} h", a.max_uptime_hours));
        }
        if let Some(why) = reason {
            self.over_mem.remove(&inst.id);
            notify(app, &inst.id, "warn", format!("Restarting {}: {why}", inst.name));
            manager_log(app, &inst.id, format!("Automatic restart: {why}"));
            let _ = server::begin_countdown(state, app, &inst.id, "restart", None).await;
        }
    }

    async fn tail_log(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance) {
        let live = state.with_runtime(&inst.id, |r| r.state.map(|s| s.is_live() || s == ServerState::Restarting).unwrap_or(false)).await;
        if !live {
            return;
        }
        let path = inst.console_log();
        let Ok(meta) = std::fs::metadata(&path) else { return };
        let len = meta.len();
        let offset = state.with_runtime(&inst.id, |r| r.log_offset).await;
        let start = if len < offset { 0 } else { offset };
        if len == start {
            return;
        }
        let start = start.max(len.saturating_sub(2 * 1_048_576));
        let mut buf = vec![];
        let ok = std::fs::File::open(&path)
            .and_then(|mut f| {
                f.seek(SeekFrom::Start(start))?;
                f.take(len - start).read_to_end(&mut buf)
            })
            .is_ok();
        if !ok {
            return;
        }
        let text = String::from_utf8_lossy(&buf).into_owned();
        let mut events: Vec<(&'static str, String, String)> = vec![];
        let mut fatal = None;
        let lines = state
            .with_runtime(&inst.id, |r| {
                r.log_offset = len;
                let mut all = std::mem::take(&mut r.log_partial);
                all.push_str(&text);
                let mut lines: Vec<String> = all.split('\n').map(|l| ANSI_RE.replace_all(l.trim_end_matches('\r'), "").into_owned()).collect();
                r.log_partial = lines.pop().unwrap_or_default();
                let now = now_ms();
                for l in &lines {
                    if r.watching {
                        r.watch.push(l.clone());
                    }
                    if let Some(c) = READY_RE.captures(l) {
                        r.log_ready = true;
                        r.boot_secs = c[1].replace(',', ".").parse().ok();
                        continue;
                    }
                    if r.running_version.is_none() {
                        if let Some(c) = VERSION_RE.captures(l) {
                            r.running_version = Some(c[1].to_string());
                            continue;
                        }
                    }
                    if let Some(c) = UUID_RE.captures(l) {
                        r.uuids.insert(c[1].to_string(), c[2].to_string());
                    } else if let Some(c) = LOGIN_RE.captures(l) {
                        let ip = c[2].rsplit_once(':').map(|(h, _)| h.to_string()).unwrap_or_else(|| c[2].to_string());
                        r.ips.insert(c[1].to_string(), ip);
                    } else if let Some(c) = JOIN_RE.captures(l) {
                        let name = c[1].to_string();
                        if !r.online.contains_key(&name) {
                            r.online.insert(name.clone(), OnlinePlayer { name: name.clone(), uuid: r.uuids.get(&name).cloned(), ip: r.ips.get(&name).cloned(), joined_at: now });
                            r.played_since_backup = true;
                            r.last_player_seen = now;
                            events.push(("join", name, String::new()));
                        }
                    } else if let Some(c) = LEFT_RE.captures(l) {
                        let name = c[1].to_string();
                        if r.online.remove(&name).is_some() {
                            r.last_player_seen = now;
                            events.push(("leave", name, String::new()));
                        }
                    } else if let Some(c) = CHAT_RE.captures(l) {
                        events.push(("chat", c[1].to_string(), c[2].to_string()));
                    } else if let Some(c) = ADV_RE.captures(l) {
                        events.push(("advancement", c[1].to_string(), c[2].to_string()));
                    } else if STOPPING_RE.is_match(l) {
                        // `stop` typed in the console or by an operator in-game: not a crash.
                        r.intentional_stop = true;
                    }
                    if FATAL_RE.is_match(l) && (fatal.is_none() || l.contains("(java)")) {
                        fatal = Some(l.clone());
                    }
                }
                lines
            })
            .await;
        if lines.is_empty() {
            return;
        }
        let shown: Vec<String> = if lines.len() > MAX_LINES_PER_TICK {
            let mut v = vec![format!("… {} earlier lines skipped (open the log file for everything)", lines.len() - MAX_LINES_PER_TICK)];
            v.extend(lines[lines.len() - MAX_LINES_PER_TICK..].iter().cloned());
            v
        } else {
            lines
        };
        let _ = app.emit("log-lines", LogBatch { id: inst.id.clone(), source: "server".into(), lines: shown, ts: now_ms() });
        for (kind, name, text) in events {
            let _ = app.emit("player-event", PlayerEvent { id: inst.id.clone(), name: name.clone(), kind: kind.into(), text: text.clone(), ts: now_ms() });
            match kind {
                "join" => notify(app, &inst.id, "info", format!("{name} joined {}", inst.name)),
                "leave" => notify(app, &inst.id, "info", format!("{name} left {}", inst.name)),
                _ => {}
            }
        }
        if let Some(l) = fatal {
            let hint = if l.contains("BIND TO PORT") {
                " – another program uses the port. Change it under Server Settings → Network."
            } else if l.contains("EULA") {
                " – accept the EULA under Server Settings."
            } else if l.contains("UnsupportedClassVersionError") || l.contains("(java)") {
                " – the Java version is too old for this server. Use automatic Java under Java & Memory."
            } else if l.contains("reserve enough space") || l.contains("OutOfMemory") {
                " – lower or raise the memory under Java & Memory."
            } else if l.contains("mods") || l.contains("Mod resolution") {
                " – a mod is missing a dependency or doesn't match this version. Check the console."
            } else {
                ""
            };
            notify(app, &inst.id, "error", format!("Server error: {}{hint}", l.trim().chars().take(220).collect::<String>()));
        }
    }

    /// Every 10 s while running: RCON `list` (authoritative player list, also for adopted
    /// servers) and a TPS query over one persistent connection. Short timeouts keep a hung
    /// server from stalling the loop.
    async fn metrics(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, now: i64) {
        if !inst.network.rcon_enabled {
            return;
        }
        let running = state.with_runtime(&inst.id, |r| r.state == Some(ServerState::Running) && r.log_ready).await;
        if !running {
            self.rcon.remove(&inst.id);
            return;
        }
        if now - self.last_metrics.get(&inst.id).copied().unwrap_or(0) < METRICS_EVERY_MS {
            return;
        }
        if self.rcon_retry.get(&inst.id).map(|t| now < *t).unwrap_or(false) {
            return;
        }
        self.last_metrics.insert(inst.id.clone(), now);
        let conn = match self.rcon.remove(&inst.id) {
            Some(c) => Some(c),
            None => {
                let host = if inst.network.bind_ip.trim().is_empty() || inst.network.bind_ip.trim() == "0.0.0.0" { "127.0.0.1".to_string() } else { inst.network.bind_ip.trim().to_string() };
                match mcproto::Rcon::connect(&host, inst.network.rcon_port, &inst.network.rcon_password).await {
                    Ok(c) => Some(c),
                    Err(_) => {
                        self.rcon_retry.insert(inst.id.clone(), now + 30_000);
                        None
                    }
                }
            }
        };
        let Some(mut conn) = conn else { return };
        let tps_cmd = if self.no_tps.contains(&inst.id) {
            None
        } else {
            Some(match inst.active_software() {
                "paper" | "purpur" | "folia" => "tps",
                "forge" => "forge tps",
                "neoforge" => "neoforge tps",
                _ => "tick query",
            })
        };
        let list = tokio::time::timeout(Duration::from_secs(3), conn.cmd("list")).await;
        let tps = match tps_cmd {
            Some(c) => tokio::time::timeout(Duration::from_secs(3), conn.cmd(c)).await.ok().and_then(|r| r.ok()),
            None => None,
        };
        let Ok(Ok(list)) = list else {
            // Connection dropped – reconnect on the next round.
            return;
        };
        self.rcon.insert(inst.id.clone(), conn);
        if let Some(t) = &tps {
            let low = mcproto::strip_codes(t).to_lowercase();
            if low.contains("unknown") || low.contains("incorrect argument") {
                self.no_tps.insert(inst.id.clone());
            }
        }
        let (tps_v, mspt_v) = tps.as_deref().map(mcproto::parse_tps).unwrap_or((None, None));
        let mut events = vec![];
        if let Some((_, max, names)) = mcproto::parse_list(&list) {
            state
                .with_runtime(&inst.id, |r| {
                    r.max_players = Some(max);
                    if tps_v.is_some() {
                        r.tps = tps_v;
                    }
                    if mspt_v.is_some() {
                        r.mspt = mspt_v;
                    }
                    let current: HashSet<&String> = names.iter().collect();
                    let gone: Vec<String> = r.online.keys().filter(|k| !current.contains(k)).cloned().collect();
                    for g in gone {
                        r.online.remove(&g);
                        events.push(("leave", g));
                    }
                    for n in &names {
                        if !r.online.contains_key(n) {
                            r.online.insert(n.clone(), OnlinePlayer { name: n.clone(), uuid: r.uuids.get(n).cloned(), ip: r.ips.get(n).cloned(), joined_at: now });
                            events.push(("join", n.clone()));
                        }
                    }
                    if !names.is_empty() {
                        r.played_since_backup = true;
                        r.last_player_seen = now;
                    }
                })
                .await;
        }
        for (kind, name) in events {
            let _ = app.emit("player-event", PlayerEvent { id: inst.id.clone(), name, kind: kind.into(), text: String::new(), ts: now });
        }
    }

    /// Restart/stop countdowns: broadcast warnings at the configured marks, then act.
    async fn countdown(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, now: i64) {
        let Some(cd) = state.with_runtime(&inst.id, |r| r.countdown.clone()).await else { return };
        let left_ms = cd.at - now;
        if left_ms > 0 {
            let left = (left_ms + 999) / 1000;
            for w in &inst.automation.restart_warnings {
                let w = *w as i64;
                // Fire once as the countdown crosses each mark (ticks are at most 5 s apart).
                if left <= w && left > w - 6 {
                    let key = format!("cd-{}-{w}", cd.at);
                    let fresh = state.with_runtime(&inst.id, |r| r.fired_schedule_keys.insert(key)).await;
                    if fresh {
                        let what = server::warning_text(inst, &cd.action);
                        server::say(state, app, &inst.id, &what.replace("{time}", &server::human_secs(w as u32))).await;
                    }
                }
            }
            return;
        }
        state.with_runtime(&inst.id, |r| r.countdown = None).await;
        let (st, app2, id, action) = (state.clone(), app.clone(), inst.id.clone(), cd.action.clone());
        tauri::async_runtime::spawn(async move {
            let res = match action.as_str() {
                "stop" => server::stop(&st, &app2, &id, false).await,
                "update" => server::install_job(&st, &app2, &id).await,
                _ => server::restart(&st, &app2, &id).await,
            };
            if let Err(e) = res {
                notify(&app2, &id, "error", format!("Scheduled {action} failed: {e}"));
            }
        });
    }

    async fn automation(&mut self, state: &Arc<AppState>, app: &AppHandle, inst: &ServerInstance, alive: bool, now: i64) {
        let a = &inst.automation;
        let local = Local::now();
        let mut next: Option<NextSchedule> = None;
        let mut fire = vec![];
        for (idx, e) in a.schedules.iter().enumerate().filter(|(_, e)| e.enabled) {
            let Ok(t) = NaiveTime::parse_from_str(e.time.trim(), "%H:%M") else { continue };
            let day_ok = |wd: u32| e.days.is_empty() || e.days.contains(&(wd as u8));
            // Restarts/stops with warnings start early so the action itself lands on time.
            let lead = if matches!(e.action.as_str(), "restart" | "stop") { a.restart_warnings.iter().copied().max().unwrap_or(0) as i64 } else { 0 };
            for d in 0..8 {
                let date = local.date_naive() + chrono::Days::new(d);
                let Some(dt) = Local.from_local_datetime(&date.and_time(t)).single() else { continue };
                if !day_ok(dt.weekday().num_days_from_sunday()) {
                    continue;
                }
                let at = dt.timestamp_millis();
                let trigger = at - lead * 1000;
                if trigger <= now && now - trigger < 60_000 {
                    let key = format!("{}-{idx}-{}-{}", dt.format("%Y%m%d"), e.time, e.action);
                    if state.with_runtime(&inst.id, |r| r.fired_schedule_keys.insert(key)).await {
                        fire.push((e.action.clone(), e.command.clone(), at));
                    }
                }
                if at > now {
                    if next.as_ref().map(|n| at < n.at).unwrap_or(true) {
                        next = Some(NextSchedule { at, action: e.action.clone() });
                    }
                    break;
                }
            }
        }
        state
            .with_runtime(&inst.id, |r| {
                r.next_schedule = next;
                if r.fired_schedule_keys.len() > 300 {
                    r.fired_schedule_keys.clear();
                }
            })
            .await;
        for (action, command, at) in fire {
            let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
            manager_log(app, &inst.id, format!("Scheduled {action}{}", if command.is_empty() { String::new() } else { format!(": {command}") }));
            tauri::async_runtime::spawn(async move {
                let res = match action.as_str() {
                    "restart" | "stop" => {
                        let secs = ((at - now_ms()) / 1000).max(0) as u32;
                        let players = st.with_runtime(&id, |r| r.online.len()).await;
                        if players == 0 {
                            // Nobody to warn: act at the scheduled minute.
                            tokio::time::sleep(Duration::from_secs(secs as u64)).await;
                        }
                        server::begin_countdown(&st, &app2, &id, &action, Some(secs)).await
                    }
                    "start" => server::start(&st, &app2, &id).await,
                    "backup" => run_backup(&st, &app2, &id, "Scheduled", "auto").await,
                    "command" => server::send_command(&st, &app2, &id, &command).await.map(|_| ()),
                    "say" => server::send_command(&st, &app2, &id, &format!("say {command}")).await.map(|_| ()),
                    _ => Ok(()),
                };
                if let Err(e) = res {
                    notify(&app2, &id, "error", format!("Scheduled {action} failed: {e}"));
                }
            });
        }

        // Interval backups while running (skipped when nobody played since the last one).
        if alive && a.backup_interval_mins > 0 {
            let due = state
                .with_runtime(&inst.id, |r| {
                    if r.last_backup == 0 {
                        r.last_backup = now;
                    }
                    let interval_hit = now - r.last_backup >= a.backup_interval_mins as i64 * 60_000;
                    let idle = a.backup_only_if_players && !r.played_since_backup && r.online.is_empty();
                    if interval_hit && idle {
                        r.last_backup = now; // nothing changed – check again next interval
                        return false;
                    }
                    let due = r.state == Some(ServerState::Running) && r.busy.is_none() && interval_hit;
                    if due {
                        r.last_backup = now;
                        r.played_since_backup = !r.online.is_empty();
                    }
                    due
                })
                .await;
            if due {
                let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = run_backup(&st, &app2, &id, "Interval", "auto").await {
                        notify(&app2, &id, "warn", format!("Automatic backup failed: {e}"));
                    }
                });
            }
        }

        // Restart when empty for a while after people played.
        if alive && a.empty_restart_mins > 0 {
            let due = state
                .with_runtime(&inst.id, |r| {
                    let due = r.state == Some(ServerState::Running) && r.online.is_empty() && r.last_player_seen > 0 && now - r.last_player_seen > a.empty_restart_mins as i64 * 60_000 && r.countdown.is_none();
                    if due {
                        r.last_player_seen = 0;
                    }
                    due
                })
                .await;
            if due {
                manager_log(app, &inst.id, format!("Nobody online for {} minutes – restarting to free memory.", a.empty_restart_mins));
                let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
                tauri::async_runtime::spawn(async move {
                    let _ = server::restart(&st, &app2, &id).await;
                });
            }
        }

        // Build / loader update checks.
        if a.update_check_mins > 0 && inst.installed.is_some() {
            let due = state
                .with_runtime(&inst.id, |r| {
                    let due = now - r.last_update_check >= a.update_check_mins as i64 * 60_000 && r.busy.is_none();
                    if due {
                        r.last_update_check = now;
                    }
                    due
                })
                .await;
            if due {
                let (st, app2, id) = (state.clone(), app.clone(), inst.id.clone());
                tauri::async_runtime::spawn(async move {
                    let _ = check_update(&st, &app2, &id, true).await;
                });
            }
        }
    }

    /// Announce new Minecraft releases (and snapshots, if wanted) once.
    async fn version_watch(&mut self, state: &Arc<AppState>, app: &AppHandle, now: i64) {
        let s = state.settings.read().await.clone();
        if s.version_check_mins == 0 || now - self.last_version_check < s.version_check_mins as i64 * 60_000 {
            return;
        }
        self.last_version_check = now;
        let (st, app2) = (state.clone(), app.clone());
        tauri::async_runtime::spawn(async move {
            st.invalidate("mojang-manifest").await;
            let Ok(m) = versions::manifest(&st).await else { return };
            let release = m["latest"]["release"].as_str().unwrap_or_default().to_string();
            let snapshot = m["latest"]["snapshot"].as_str().unwrap_or_default().to_string();
            let mut settings = st.settings.write().await;
            let mut changed = false;
            if !release.is_empty() && settings.last_seen_release != release {
                if !settings.last_seen_release.is_empty() {
                    notify(&app2, "", "success", format!("Minecraft {release} is out! Switch a server to it under Version & Software."));
                    let _ = app2.emit("mc-release", serde_json::json!({ "kind": "release", "id": release }));
                }
                settings.last_seen_release = release;
                changed = true;
            }
            if !snapshot.is_empty() && settings.last_seen_snapshot != snapshot {
                if settings.notify_snapshots && !settings.last_seen_snapshot.is_empty() && snapshot != settings.last_seen_release {
                    notify(&app2, "", "info", format!("New Minecraft snapshot: {snapshot}"));
                    let _ = app2.emit("mc-release", serde_json::json!({ "kind": "snapshot", "id": snapshot }));
                }
                settings.last_seen_snapshot = snapshot;
                changed = true;
            }
            drop(settings);
            if changed {
                let _ = st.persist_settings().await;
            }
        });
    }
}

/// After a start the server has written its full server.properties (defaults for every key):
/// take them into the profile so the settings view shows real values.
async fn absorb_properties(state: &Arc<AppState>, app: &AppHandle, id: &str) {
    let Ok(inst) = state.instance(id).await else { return };
    let Ok(doc) = super::props::read(&inst.properties_path()) else { return };
    let mut merged: BTreeMap<String, String> = doc.map();
    for (k, v) in &inst.properties {
        merged.insert(k.clone(), v.clone());
    }
    for k in server::network_overlay(&inst).keys() {
        merged.remove(k);
    }
    let mut changed = false;
    {
        let mut list = state.instances.write().await;
        if let Some(i) = list.iter_mut().find(|i| i.id == id) {
            if i.properties != merged {
                i.properties = merged;
                changed = true;
            }
        }
    }
    if changed {
        let _ = state.persist_instances().await;
        let _ = app.emit("instances-changed", ());
    }
}

/// Version and boot time from a console log written so far (for adopted servers).
fn scan_log_meta(path: &std::path::Path) -> (Option<String>, Option<f64>) {
    let text = std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let version = text.lines().find_map(|l| VERSION_RE.captures(l).map(|c| c[1].to_string()));
    let boot = text.lines().find_map(|l| READY_RE.captures(l).and_then(|c| c[1].replace(',', ".").parse().ok()));
    (version, boot)
}

/// Backup with the save pipeline paused while it runs (save-off / save-all flush / save-on), the
/// way the Minecraft wiki recommends for live servers.
pub async fn run_backup(state: &Arc<AppState>, app: &AppHandle, id: &str, label: &str, kind: &str) -> crate::error::AppResult<()> {
    let inst = state.instance(id).await?;
    state.set_busy(app, id, Some("Backing up")).await;
    let live = state.with_runtime(id, |r| r.state == Some(ServerState::Running)).await;
    let mut paused = false;
    if live {
        state
            .with_runtime(id, |r| {
                r.watch.clear();
                r.watching = true;
            })
            .await;
        if server::send_command(state, app, id, "save-off").await.is_ok() {
            paused = true;
            let reply = server::send_command(state, app, id, "save-all flush").await.ok().flatten().unwrap_or_default();
            let saved = |t: &str| t.contains("Saved the game") || t.contains("Saved the world") || t.contains("All dimensions are saved");
            if !saved(&reply) {
                for _ in 0..120 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if state.with_runtime(id, |r| r.watch.iter().any(|l| saved(l))).await {
                        break;
                    }
                }
            }
        }
        state
            .with_runtime(id, |r| {
                r.watching = false;
                r.watch.clear();
            })
            .await;
    }
    let res = backup::create(state, &inst, label, kind).await;
    if paused {
        let _ = server::send_command(state, app, id, "save-on").await;
    }
    state.set_busy(app, id, None).await;
    let b = res?;
    state.with_runtime(id, |r| r.last_backup = b.created_at).await;
    let _ = app.emit("backup-created", &b);
    Ok(())
}

/// Look for a newer build / loader for the installed Minecraft version, and newer Minecraft
/// releases the software supports. With "auto-update builds" on, newer builds are installed
/// (with a countdown when players are online).
pub async fn check_update(state: &Arc<AppState>, app: &AppHandle, id: &str, background: bool) -> crate::error::AppResult<Option<UpdateInfo>> {
    let inst = state.instance(id).await?;
    let Some(st) = inst.installed.clone() else { return Ok(None) };
    if !background {
        state.invalidate("fill-").await;
        state.invalidate("purpur").await;
        state.invalidate("fabric").await;
        state.invalidate("quilt").await;
        state.invalidate("forge").await;
        state.invalidate("neoforge").await;
        state.invalidate("mojang-manifest").await;
    }
    let mut info = None;
    if st.software != "vanilla" {
        if let Ok(latest) = versions::default_loader(state, &st.software, &st.mc_version).await {
            let newer = match st.software.as_str() {
                "paper" | "folia" | "purpur" => latest.parse::<i64>().unwrap_or(0) > st.loader_version.parse::<i64>().unwrap_or(i64::MAX),
                _ => !latest.is_empty() && latest != st.loader_version,
            };
            if newer {
                info = Some(UpdateInfo {
                    kind: "build".into(),
                    current: st.loader_version.clone(),
                    latest: latest.clone(),
                    label: format!("{} {} → {} for Minecraft {}", software_label(&st.software), if st.software == "paper" || st.software == "purpur" || st.software == "folia" { format!("build {}", st.loader_version) } else { st.loader_version.clone() }, latest, st.mc_version),
                });
            }
        }
    }
    // Modded servers are pinned to their mods' / modpack's version – don't nag them about releases.
    if info.is_none() && matches!(st.software.as_str(), "vanilla" | "paper" | "purpur" | "folia") {
        if let Ok(list) = versions::game_versions(state, &st.software).await {
            let release_install = list.versions.iter().find(|v| v.id == st.mc_version).map(|v| v.kind == "release").unwrap_or(true);
            if release_install {
                if let Some(newest) = list.versions.iter().find(|v| v.kind == "release") {
                    if newest.id != st.mc_version && versions::is_newer(state, &newest.id, &st.mc_version).await == Some(true) {
                        info = Some(UpdateInfo {
                            kind: "release".into(),
                            current: st.mc_version.clone(),
                            latest: newest.id.clone(),
                            label: format!("Minecraft {} is available for {} (you run {}). Check that your mods/plugins support it first.", newest.id, software_label(&st.software), st.mc_version),
                        });
                    }
                }
            }
        }
    }
    let prev = state.with_runtime(id, |r| std::mem::replace(&mut r.update, info.clone())).await;
    state.emit_status(app, id).await;
    if let Some(u) = &info {
        let fresh = prev.as_ref().map(|p| p.latest != u.latest).unwrap_or(true);
        if u.kind == "build" && inst.automation.auto_update_builds {
            if fresh || !background {
                notify(app, id, "info", format!("Updating {}: {}", inst.name, u.label));
            }
            {
                let mut list = state.instances.write().await;
                if let Some(i) = list.iter_mut().find(|i| i.id == id) {
                    // Pin the new build so the install picks exactly it.
                    i.loader_version = u.latest.clone();
                }
            }
            state.persist_instances().await?;
            let _ = app.emit("instances-changed", ());
            let running = server::running_pid(&inst).is_some();
            if running {
                server::begin_countdown(state, app, id, "update", None).await?;
            } else {
                server::install_job(state, app, id).await?;
            }
        } else if fresh && background {
            notify(app, id, "info", format!("{}: {}", inst.name, u.label));
        }
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_log_lines_of_every_flavour() {
        // Vanilla / Fabric
        assert!(READY_RE.is_match(r#"[12:00:01] [Server thread/INFO]: Done (4.512s)! For help, type "help""#));
        assert_eq!(&READY_RE.captures(r#"[12:00:01 INFO]: Done (12,3s)! For help, type "help""#).unwrap()[1], "12,3");
        assert_eq!(&VERSION_RE.captures("[12:00:00] [Server thread/INFO]: Starting minecraft server version 26.3").unwrap()[1], "26.3");
        // Paper console format
        assert_eq!(&JOIN_RE.captures("[12:01:00 INFO]: Steve joined the game").unwrap()[1], "Steve");
        // Forge format
        assert_eq!(&LEFT_RE.captures("[12:02:00] [Server thread/INFO] [minecraft/MinecraftServer]: Alex left the game").unwrap()[1], "Alex");
        let c = LOGIN_RE.captures("[12:01:00] [Server thread/INFO]: Steve[/192.168.1.20:51234] logged in with entity id 123 at (0.5, 64.0, 0.5)").unwrap();
        assert_eq!((&c[1], &c[2]), ("Steve", "192.168.1.20:51234"));
        let c = UUID_RE.captures("[12:00:59] [User Authenticator #1/INFO]: UUID of player Steve is 8667ba71-b85a-4004-af54-457a9734eed7").unwrap();
        assert_eq!(&c[2], "8667ba71-b85a-4004-af54-457a9734eed7");
        let c = CHAT_RE.captures("[12:03:00] [Server thread/INFO]: [Not Secure] <Steve> hello there").unwrap();
        assert_eq!((&c[1], &c[2]), ("Steve", "hello there"));
        // Chat can't fake a join
        assert!(JOIN_RE.captures("[12:03:00] [Server thread/INFO]: <Steve> Bob joined the game").is_none());
        assert!(STOPPING_RE.is_match("[12:10:00] [Server thread/INFO]: Stopping the server"));
        assert!(STOPPING_RE.is_match("[12:10:00 INFO]: Stopping server"));
        assert!(FATAL_RE.is_match("[12:00:00] [Server thread/WARN]: **** FAILED TO BIND TO PORT!"));
        let c = ADV_RE.captures("[12:04:00] [Server thread/INFO]: Alex has made the advancement [Stone Age]").unwrap();
        assert_eq!(&c[2], "Stone Age");
        assert_eq!(ANSI_RE.replace_all("\x1b[0;32mhi\x1b[m", ""), "hi");
    }
}
