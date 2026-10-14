//! Server lifecycle: JVM command line, server.properties / eula.txt sync, start / stop / restart,
//! console commands (stdin first, RCON as fallback) and install / version-switch jobs.
//!
//! Every lifecycle operation for one server runs under `AppState::op_lock(id)` so a scheduled
//! restart, a crash restart and a button click can never interleave.

use super::{backup, install, mcproto, props, proc};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::{manager_log, notify, now_ms, AppState, TaskReporter};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Aikar's G1 flags (mcflags.emc.gs) – the community standard for Minecraft servers.
fn aikar(max_mb: u32) -> Vec<&'static str> {
    let big = max_mb >= 12 * 1024;
    let mut v = vec![
        "-XX:+UseG1GC",
        "-XX:+ParallelRefProcEnabled",
        "-XX:MaxGCPauseMillis=200",
        "-XX:+UnlockExperimentalVMOptions",
        "-XX:+DisableExplicitGC",
        "-XX:+AlwaysPreTouch",
    ];
    if big {
        v.extend(["-XX:G1NewSizePercent=40", "-XX:G1MaxNewSizePercent=50", "-XX:G1HeapRegionSize=16M", "-XX:G1ReservePercent=15", "-XX:InitiatingHeapOccupancyPercent=20"]);
    } else {
        v.extend(["-XX:G1NewSizePercent=30", "-XX:G1MaxNewSizePercent=40", "-XX:G1HeapRegionSize=8M", "-XX:G1ReservePercent=20", "-XX:InitiatingHeapOccupancyPercent=15"]);
    }
    v.extend([
        "-XX:G1HeapWastePercent=5",
        "-XX:G1MixedGCCountTarget=4",
        "-XX:G1MixedGCLiveThresholdPercent=90",
        "-XX:G1RSetUpdatingPauseTimePercent=5",
        "-XX:SurvivorRatio=32",
        "-XX:+PerfDisableSharedMem",
        "-XX:MaxTenuringThreshold=1",
        "-Dusing.aikars.flags=https://mcflags.emc.gs",
        "-Daikars.new.flags=true",
    ]);
    v
}

pub fn gc_flags(preset: &str, max_mb: u32, java_major: u32) -> Vec<String> {
    let v: Vec<&str> = match preset {
        "aikar" => aikar(max_mb),
        "g1" => vec!["-XX:+UseG1GC", "-XX:MaxGCPauseMillis=200", "-XX:+ParallelRefProcEnabled", "-XX:+DisableExplicitGC"],
        "zgc" if java_major >= 17 => vec!["-XX:+UseZGC", "-XX:+ZGenerational", "-XX:+AlwaysPreTouch", "-XX:+DisableExplicitGC"],
        "zgc" => vec!["-XX:+UseG1GC", "-XX:MaxGCPauseMillis=200"],
        _ => vec![],
    };
    let mut out: Vec<String> = vec![];
    if !v.is_empty() {
        // Flags retired in newer JDKs must not stop the server from booting.
        out.push("-XX:+IgnoreUnrecognizedVMOptions".into());
    }
    out.extend(v.into_iter().map(String::from));
    out
}

/// Split a free-form argument string, keeping "quoted values" together (quotes removed).
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut in_q = false;
    let mut any = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_q = !in_q;
                any = true;
            }
            c if c.is_whitespace() && !in_q => {
                if !cur.is_empty() || any {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() || any {
        out.push(cur);
    }
    out
}

/// Everything after `java`: memory, GC, marker, encoding, the launch target and server args.
pub fn build_args(inst: &ServerInstance) -> Vec<String> {
    let j = &inst.java;
    let max = j.max_mem_mb.max(512);
    let min = j.min_mem_mb.clamp(256, max);
    let java_major = inst.installed.as_ref().map(|i| i.java_major).unwrap_or(21);
    let mut a = vec![format!("-Xms{min}M"), format!("-Xmx{max}M")];
    a.extend(gc_flags(&j.gc_preset, max, java_major));
    a.push(format!("{MARKER_PROP}{}", inst.id));
    // Console output goes to a file: force UTF-8 so names and chat with accents stay readable,
    // and keep JLine from trying to drive a terminal that isn't there.
    a.extend(
        [
            "-Dfile.encoding=UTF-8",
            "-Dstdout.encoding=UTF-8",
            "-Dstderr.encoding=UTF-8",
            "-Dstdin.encoding=UTF-8",
            "-Dterminal.jline=false",
            "-Dterminal.ansi=false",
            "-Dlog4j2.formatMsgNoLookups=true",
        ]
        .map(String::from),
    );
    a.extend(split_args(&j.jvm_args).into_iter().filter(|x| !x.starts_with(MARKER_PROP)));
    if let Some(st) = &inst.installed {
        if st.target.kind == "args" {
            a.push(format!("@{}", st.target.path));
        } else {
            a.push("-jar".into());
            a.push(st.target.path.clone());
        }
    }
    a.extend(split_args(&j.server_args));
    a
}

fn quote(a: &str) -> String {
    if a.contains(' ') {
        format!("\"{a}\"")
    } else {
        a.to_string()
    }
}

pub fn launch_line(inst: &ServerInstance, java: &str) -> String {
    let mut s = quote(java);
    for a in build_args(inst) {
        s.push(' ');
        s.push_str(&quote(&a));
    }
    s
}

/// The properties the profile manages on top of the user's server.properties values.
pub fn network_overlay(inst: &ServerInstance) -> BTreeMap<String, String> {
    let n = &inst.network;
    let mut m = BTreeMap::new();
    m.insert("server-port".into(), n.port.to_string());
    m.insert("server-ip".into(), n.bind_ip.trim().to_string());
    m.insert("enable-rcon".into(), n.rcon_enabled.to_string());
    m.insert("rcon.port".into(), n.rcon_port.to_string());
    m.insert("rcon.password".into(), n.rcon_password.clone());
    m.insert("broadcast-rcon-to-ops".into(), "false".into());
    m.insert("enable-query".into(), n.query_enabled.to_string());
    m.insert("query.port".into(), (if n.query_port == 0 { n.port } else { n.query_port }).to_string());
    m
}

/// Write server.properties (profile values + network overlay, keeping comments and unknown keys)
/// and eula.txt when the user accepted the EULA in the manager.
pub fn sync_files(inst: &ServerInstance) -> AppResult<()> {
    std::fs::create_dir_all(&inst.install_dir)?;
    let mut values = inst.properties.clone();
    values.extend(network_overlay(inst));
    props::merge_into(&inst.properties_path(), &values)?;
    if inst.eula_accepted {
        let eula = inst.install_dir.join("eula.txt");
        let cur = std::fs::read_to_string(&eula).unwrap_or_default();
        if !cur.lines().any(|l| l.trim().eq_ignore_ascii_case("eula=true")) {
            std::fs::write(
                eula,
                format!(
                    "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\n#Accepted in Minecraft Server Manager on {}\neula=true\n",
                    chrono::Local::now().format("%Y-%m-%d %H:%M")
                ),
            )?;
        }
    }
    Ok(())
}

/// Read server.properties into a profile (used when importing an existing server).
pub fn read_files_into(inst: &mut ServerInstance) {
    let Ok(doc) = props::read(&inst.properties_path()) else { return };
    let map = doc.map();
    if map.is_empty() {
        return;
    }
    let n = &mut inst.network;
    let g = |k: &str| map.get(k).cloned().unwrap_or_default();
    if let Ok(p) = g("server-port").parse() {
        n.port = p;
    }
    n.bind_ip = g("server-ip");
    if let Ok(p) = g("rcon.port").parse() {
        n.rcon_port = p;
    }
    if !g("rcon.password").is_empty() {
        n.rcon_password = g("rcon.password");
    }
    n.query_enabled = g("enable-query") == "true";
    if let Ok(p) = g("query.port").parse() {
        n.query_port = p;
    }
    let eula = std::fs::read_to_string(inst.install_dir.join("eula.txt")).unwrap_or_default();
    inst.eula_accepted = eula.lines().any(|l| l.trim().eq_ignore_ascii_case("eula=true"));
    inst.properties = map;
}

pub fn random_password(len: usize) -> String {
    let a = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789";
    let mut out = String::new();
    while out.len() < len {
        for b in uuid::Uuid::new_v4().as_bytes() {
            if out.len() < len {
                out.push(a[*b as usize % a.len()] as char);
            }
        }
    }
    out
}

/// PID of this profile's running server.
pub fn running_pid(inst: &ServerInstance) -> Option<u32> {
    proc::find_server(&inst.id)
}

pub fn validate_for_start(inst: &ServerInstance) -> AppResult<Vec<String>> {
    if !inst.eula_accepted {
        return Err(AppError::msg("Accept the Minecraft EULA first (Server Settings → EULA) – the server refuses to start without it."));
    }
    let mut warnings = vec![];
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_mb = sys.total_memory() / 1_048_576;
    if total_mb > 0 && inst.java.max_mem_mb as u64 > total_mb * 85 / 100 {
        warnings.push(format!("The server may use {} MB but this PC has {total_mb} MB – Windows will start swapping.", inst.java.max_mem_mb));
    }
    if inst.network.rcon_enabled && inst.network.rcon_password.trim().is_empty() {
        return Err(AppError::msg("RCON is on but has no password – set one under Server Settings → Network."));
    }
    Ok(warnings)
}

pub async fn start(state: &AppState, app: &AppHandle, id: &str) -> AppResult<()> {
    let lock = state.op_lock(id).await;
    let _g = lock.lock().await;
    start_locked(state, app, id).await
}

async fn start_locked(state: &AppState, app: &AppHandle, id: &str) -> AppResult<()> {
    let inst = state.instance(id).await?;
    if !inst.is_installed() {
        return Err(AppError::msg("The server software isn't installed yet. Click Install first."));
    }
    if let Some(pid) = running_pid(&inst) {
        state
            .with_runtime(id, |r| {
                r.pid = Some(pid);
                r.state = Some(ServerState::Running);
                r.started_at.get_or_insert(now_ms());
            })
            .await;
        state.emit_status(app, id).await;
        return Ok(());
    }
    for w in validate_for_start(&inst)? {
        notify(app, id, "warn", w);
    }

    let (tcp, _) = proc::port_owners().await;
    for (port, what) in [(Some(inst.network.port), "Minecraft"), (inst.network.rcon_enabled.then_some(inst.network.rcon_port), "RCON")] {
        let Some(port) = port else { continue };
        if let Some(pid) = tcp.get(&port) {
            let mut sys = sysinfo::System::new();
            let name = proc::process_name(&mut sys, *pid).unwrap_or_else(|| "another program".into());
            return Err(AppError::msg(format!("TCP port {port} ({what}) is already used by {name} (PID {pid}). Pick a different port under Server Settings → Network.")));
        }
    }

    let required = inst.installed.as_ref().map(|i| i.java_major).filter(|m| *m > 0).unwrap_or(21);
    state.set_busy(app, id, Some("Preparing Java")).await;
    let r = TaskReporter::new(app, "java", &format!("Java {required} for {}", inst.name), Some(id));
    let java = install::java_for(state, &inst, required, Some(&r)).await;
    state.set_busy(app, id, None).await;
    let java = match java {
        Ok(j) => {
            r.done_silent();
            j
        }
        Err(e) => {
            r.fail(&e.to_string());
            return Err(e);
        }
    };

    sync_files(&inst)?;
    let removed = super::mods::apply_pending_removals(&inst);
    if removed > 0 {
        manager_log(app, id, format!("Removed {removed} replaced mod/plugin file(s) that were in use."));
    }
    let args = build_args(&inst);
    let child = proc::spawn_java(&java, &args, &inst.install_dir, &inst.console_log(), &inst.java.priority)?;
    let pid = child.id();
    if !inst.java.affinity.is_empty() {
        let _ = proc::apply_priority_affinity(pid, &inst.java.priority, &inst.java.affinity);
    }
    let mut child = child;
    if let Some(stdin) = child.stdin.take() {
        state.stdins.lock().await.insert(id.to_string(), stdin);
    }
    // Reap the child on a plain thread and remember its exit code for crash reports.
    let exits = state.exits.clone();
    let id2 = id.to_string();
    std::thread::spawn(move || {
        if let Ok(st) = child.wait() {
            if let Ok(mut m) = exits.lock() {
                m.insert(id2, st.code().map(|c| c as i64).unwrap_or(-1));
            }
        }
    });
    state
        .with_runtime(id, |r| {
            r.pid = Some(pid);
            r.state = Some(ServerState::Starting);
            r.started_at = Some(now_ms());
            r.ready_at = None;
            r.boot_secs = None;
            r.intentional_stop = false;
            r.pending_restart = false;
            r.restart_at = None;
            r.countdown = None;
            r.online.clear();
            r.log_offset = 0;
            r.log_partial.clear();
            r.log_ready = false;
            r.running_version = None;
            r.tps = None;
            r.mspt = None;
            r.last_exit_code = None;
        })
        .await;
    manager_log(app, id, format!("Launching {} with Java at {}", inst.installed.as_ref().map(|i| format!("{} {}", software_label(&i.software), i.mc_version)).unwrap_or_default(), java.display()));
    manager_log(app, id, format!("> {}", launch_line(&inst, &java.to_string_lossy())));
    if inst.network.upnp {
        let (app2, i2) = (app.clone(), inst.clone());
        tauri::async_runtime::spawn(async move {
            match super::upnp::map_server(&i2).await {
                Ok(lines) => manager_log(&app2, &i2.id, format!("UPnP: {}", lines.join(", "))),
                Err(e) => notify(&app2, &i2.id, "warn", format!("UPnP port forwarding failed: {e}")),
            }
        });
    }
    state.emit_status(app, id).await;
    Ok(())
}

fn rcon_host(inst: &ServerInstance) -> String {
    let b = inst.network.bind_ip.trim();
    if b.is_empty() || b == "0.0.0.0" {
        "127.0.0.1".into()
    } else {
        b.to_string()
    }
}

/// One-shot RCON command.
pub async fn rcon(inst: &ServerInstance, command: &str) -> AppResult<String> {
    if !inst.network.rcon_enabled {
        return Err(AppError::msg("RCON is turned off for this server."));
    }
    let mut c = mcproto::Rcon::connect(&rcon_host(inst), inst.network.rcon_port, &inst.network.rcon_password).await?;
    c.cmd(command).await
}

/// Send a console command. Goes to the server's stdin when the manager launched it (output shows
/// in the console as usual); otherwise over RCON, whose reply is returned and echoed.
pub async fn send_command(state: &AppState, app: &AppHandle, id: &str, command: &str) -> AppResult<Option<String>> {
    let command = command.trim().trim_start_matches('/').to_string();
    if command.is_empty() {
        return Ok(None);
    }
    {
        let mut map = state.stdins.lock().await;
        if let Some(stdin) = map.get_mut(id) {
            let ok = stdin.write_all(format!("{command}\n").as_bytes()).and_then(|_| stdin.flush()).is_ok();
            if ok {
                return Ok(None);
            }
            // Pipe broken (server exited or was restarted elsewhere) – fall back to RCON.
            map.remove(id);
        }
    }
    let inst = state.instance(id).await?;
    let reply = rcon(&inst, &command).await.map_err(|e| AppError::msg(format!("Couldn't send the command: {e}")))?;
    let clean = mcproto::strip_codes(&reply);
    let mut lines = vec![format!("> {command}")];
    lines.extend(clean.lines().map(String::from));
    let _ = app.emit("log-lines", LogBatch { id: id.to_string(), source: "rcon".into(), lines, ts: now_ms() });
    Ok(Some(clean))
}

/// Broadcast a message in-game (`say`).
pub async fn say(state: &AppState, app: &AppHandle, id: &str, text: &str) {
    let _ = send_command(state, app, id, &format!("say {text}")).await;
}

pub async fn stop(state: &AppState, app: &AppHandle, id: &str, force: bool) -> AppResult<()> {
    let lock = state.op_lock(id).await;
    let _g = lock.lock().await;
    stop_locked(state, app, id, force).await
}

async fn stop_locked(state: &AppState, app: &AppHandle, id: &str, force: bool) -> AppResult<()> {
    let inst = state.instance(id).await?;
    let Some(pid) = running_pid(&inst) else {
        state
            .with_runtime(id, |r| {
                r.pid = None;
                r.state = Some(ServerState::Stopped);
                r.restart_at = None;
                r.countdown = None;
                r.online.clear();
            })
            .await;
        state.stdins.lock().await.remove(id);
        state.emit_status(app, id).await;
        return Ok(());
    };
    state
        .with_runtime(id, |r| {
            r.intentional_stop = true;
            r.restart_at = None;
            r.countdown = None;
            r.state = Some(ServerState::Stopping);
        })
        .await;
    state.emit_status(app, id).await;

    let timeout = inst.automation.stop_timeout_secs.clamp(10, 900) as u64;
    let mut exited = false;
    if !force {
        manager_log(app, id, "Saving the world and shutting down (stop)…");
        let sent = send_command(state, app, id, "stop").await.is_ok();
        if sent {
            let mut sys = sysinfo::System::new();
            for _ in 0..timeout * 2 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if !proc::pid_alive(&mut sys, pid) {
                    exited = true;
                    break;
                }
            }
            if !exited {
                manager_log(app, id, format!("Server did not exit within {timeout}s – forcing it to close."));
                notify(app, id, "warn", format!("{} didn't shut down within {timeout}s and was force-closed.", inst.name));
            }
        } else {
            manager_log(app, id, "The server can't be reached (no console pipe and RCON failed) – forcing it to close.");
        }
    }
    if !exited {
        let _ = proc::kill_tree(pid).await;
        tokio::time::sleep(Duration::from_millis(600)).await;
    }
    state.stdins.lock().await.remove(id);
    let code = state.exits.lock().ok().and_then(|mut m| m.remove(id));
    state
        .with_runtime(id, |r| {
            r.pid = None;
            r.state = Some(ServerState::Stopped);
            r.started_at = None;
            r.ready_at = None;
            r.online.clear();
            r.pending_restart = false;
            r.tps = None;
            r.mspt = None;
            r.last_exit_code = code;
        })
        .await;
    manager_log(app, id, if exited { "Server stopped cleanly." } else { "Server process terminated." });
    state.emit_status(app, id).await;

    if inst.automation.backup_on_stop && inst.world_dir().exists() {
        state.set_busy(app, id, Some("Backing up")).await;
        let res = backup::create(state, &inst, "After stop", "auto").await;
        state.set_busy(app, id, None).await;
        match res {
            Ok(b) => {
                state.with_runtime(id, |r| r.last_backup = b.created_at).await;
                let _ = app.emit("backup-created", &b);
            }
            Err(e) => notify(app, id, "warn", format!("Backup after stop failed: {e}")),
        }
    }
    Ok(())
}

pub async fn restart(state: &AppState, app: &AppHandle, id: &str) -> AppResult<()> {
    let lock = state.op_lock(id).await;
    let _g = lock.lock().await;
    state
        .with_runtime(id, |r| {
            r.state = Some(ServerState::Restarting);
            r.crash_loop = false;
        })
        .await;
    state.emit_status(app, id).await;
    stop_locked(state, app, id, false).await?;
    start_locked(state, app, id).await
}

/// Start a restart/stop countdown with in-game warnings (handled by the supervisor). With nobody
/// online – or no warnings configured – the action runs right away.
pub async fn begin_countdown(state: &AppState, app: &AppHandle, id: &str, action: &str, secs: Option<u32>) -> AppResult<()> {
    let inst = state.instance(id).await?;
    let lead = secs.unwrap_or_else(|| inst.automation.restart_warnings.iter().copied().max().unwrap_or(0));
    let (players, running) = state.with_runtime(id, |r| (r.online.len(), r.state == Some(ServerState::Running))).await;
    if !running || players == 0 || lead == 0 {
        return match action {
            "stop" => stop(state, app, id, false).await,
            // A build update installs (stopping and starting the server itself) – a plain restart
            // would keep the old build and the next update check would restart it again.
            "update" => install_job(state, app, id).await,
            _ => restart(state, app, id).await,
        };
    }
    let at = now_ms() + lead as i64 * 1000;
    state
        .with_runtime(id, |r| {
            r.countdown = Some(NextSchedule { at, action: action.to_string() });
            r.fired_schedule_keys.insert(format!("cd-{at}-{lead}"));
        })
        .await;
    let msg = warning_text(&inst, action);
    say(state, app, id, &msg.replace("{time}", &human_secs(lead))).await;
    manager_log(app, id, format!("{} in {lead}s – warning players.", if action == "stop" { "Stopping" } else { "Restarting" }));
    state.emit_status(app, id).await;
    Ok(())
}

/// The in-game warning for a countdown ("Server restarting in {time}" / "… stopping …").
pub fn warning_text(inst: &ServerInstance, action: &str) -> String {
    let m = &inst.automation.warning_message;
    if action == "stop" {
        m.replace("restarting", "stopping").replace("Restarting", "Stopping")
    } else {
        m.clone()
    }
}

pub fn human_secs(s: u32) -> String {
    match s {
        s if s >= 3600 && s % 3600 == 0 => format!("{} hour{}", s / 3600, if s / 3600 == 1 { "" } else { "s" }),
        s if s >= 60 && s % 60 == 0 => format!("{} minute{}", s / 60, if s / 60 == 1 { "" } else { "s" }),
        s => format!("{s} second{}", if s == 1 { "" } else { "s" }),
    }
}

/// Install the profile's wanted software/version (first install or a version switch). Stops a
/// running server (with a backup) and starts it again afterwards.
pub async fn install_job(state: &AppState, app: &AppHandle, id: &str) -> AppResult<()> {
    let lock = state.op_lock(id).await;
    let _g = lock.lock().await;
    let inst = state.instance(id).await?;
    let was_running = running_pid(&inst).is_some();
    if was_running {
        manager_log(app, id, "Stopping the server to install the new version…");
        stop_locked(state, app, id, false).await?;
    }
    let first = inst.installed.is_none() || !inst.is_installed();
    if !first && inst.automation.backup_before_update && !(was_running && inst.automation.backup_on_stop) && inst.world_dir().exists() {
        state.set_busy(app, id, Some("Backing up")).await;
        if let Ok(b) = backup::create(state, &inst, "Before version change", "auto").await {
            state.with_runtime(id, |r| r.last_backup = b.created_at).await;
            let _ = app.emit("backup-created", &b);
        }
        state.set_busy(app, id, None).await;
    }
    state.with_runtime(id, |r| r.state = Some(if first { ServerState::Installing } else { ServerState::Updating })).await;
    state.emit_status(app, id).await;
    let title = format!(
        "{} {} {}",
        if first { "Installing" } else { "Switching to" },
        software_label(&inst.software),
        if inst.mc_version.is_empty() { "(latest)".to_string() } else { inst.mc_version.clone() }
    );
    let r = TaskReporter::new(app, "install", &title, Some(id));
    let res = install::install(state, &inst, &r).await;
    state.with_runtime(id, |rt| rt.state = None).await;
    match res {
        Ok(st) => {
            let summary = format!(
                "{} {}{} installed",
                software_label(&st.software),
                st.mc_version,
                if st.loader_version.is_empty() { String::new() } else { format!(" ({})", st.loader_version) }
            );
            {
                let mut list = state.instances.write().await;
                if let Some(i) = list.iter_mut().find(|i| i.id == id) {
                    i.installed = Some(st.clone());
                    if i.mc_version.is_empty() {
                        i.mc_version = st.mc_version.clone();
                    }
                }
            }
            state.persist_instances().await?;
            let inst = state.instance(id).await?;
            let _ = sync_files(&inst);
            state.with_runtime(id, |rt| rt.update = None).await;
            r.done(&summary);
            manager_log(app, id, &summary);
            let _ = app.emit("instances-changed", ());
            state.emit_status(app, id).await;
            if was_running {
                start_locked(state, app, id).await?;
            }
            Ok(())
        }
        Err(e) => {
            r.fail(&e.to_string());
            state.emit_status(app, id).await;
            Err(e)
        }
    }
}

/// Config / text files a user may want to edit, relative to the server folder.
pub fn config_candidates(inst: &ServerInstance) -> Vec<PathBuf> {
    let root = &inst.install_dir;
    let mut out = vec![];
    let exts = ["properties", "yml", "yaml", "toml", "json", "json5", "conf", "cfg", "txt", "ini", "hocon", "snbt"];
    let skip_names = ["usercache.json", "version_history.json", "eula.txt"];
    let ok = |p: &std::path::Path| {
        let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        exts.contains(&ext.as_str()) && !skip_names.contains(&n.as_str()) && std::fs::metadata(p).map(|m| m.len() < 2_000_000).unwrap_or(false)
    };
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_file() && ok(&p) {
                out.push(p);
            }
        }
    }
    for sub in ["config", "plugins", "defaultconfigs", "world/serverconfig", "kubejs/config"] {
        let d = root.join(sub);
        if !d.exists() {
            continue;
        }
        for e in walkdir::WalkDir::new(&d).max_depth(4).into_iter().flatten() {
            let p = e.path();
            if e.file_type().is_file() && ok(p) && !p.to_string_lossy().contains("_history") {
                out.push(p.to_path_buf());
            }
            if out.len() > 2000 {
                break;
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_java_command_line() {
        let mut inst = ServerInstance { id: "abc".into(), ..Default::default() };
        inst.installed = Some(InstallState { software: "paper".into(), java_major: 21, target: LaunchTarget { kind: "jar".into(), path: "paper-1.21.8-60.jar".into() }, ..Default::default() });
        inst.java.jvm_args = "-Dfoo=\"a b\" -Dmcsm.id=evil".into();
        let a = build_args(&inst);
        assert_eq!(a[0], "-Xms2048M");
        assert_eq!(a[1], "-Xmx4096M");
        assert!(a.contains(&"-XX:+UseG1GC".to_string()));
        assert!(a.contains(&"-Dmcsm.id=abc".to_string()));
        assert!(!a.contains(&"-Dmcsm.id=evil".to_string()));
        assert!(a.contains(&"-Dfoo=a b".to_string()));
        let j = a.iter().position(|x| x == "-jar").unwrap();
        assert_eq!(a[j + 1], "paper-1.21.8-60.jar");
        assert_eq!(a.last().unwrap(), "nogui");
        inst.installed.as_mut().unwrap().target = LaunchTarget { kind: "args".into(), path: "libraries/net/neoforged/neoforge/21.1.200/win_args.txt".into() };
        inst.java.gc_preset = "none".into();
        let a = build_args(&inst);
        assert!(a.contains(&"@libraries/net/neoforged/neoforge/21.1.200/win_args.txt".to_string()));
        assert!(!a.iter().any(|x| x.starts_with("-XX")));
    }

    #[test]
    fn syncs_properties_and_eula() {
        let dir = std::env::temp_dir().join(format!("mcsm-sync-{}", uuid::Uuid::new_v4()));
        let mut inst = ServerInstance { install_dir: dir.clone(), ..Default::default() };
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(inst.properties_path(), "#comment\nmotd=Old\nspawn-protection=16\n").unwrap();
        inst.properties.insert("motd".into(), "§aNew".into());
        inst.network.port = 25570;
        inst.network.rcon_password = "pw".into();
        sync_files(&inst).unwrap();
        assert!(!dir.join("eula.txt").exists(), "EULA is only written once accepted");
        inst.eula_accepted = true;
        sync_files(&inst).unwrap();
        let text = std::fs::read_to_string(inst.properties_path()).unwrap();
        assert!(text.starts_with("#comment\n"));
        assert!(text.contains("motd=\\u00A7aNew"));
        assert!(text.contains("spawn-protection=16"));
        assert!(text.contains("server-port=25570"));
        assert!(text.contains("enable-rcon=true"));
        assert!(std::fs::read_to_string(dir.join("eula.txt")).unwrap().contains("eula=true"));
        let mut back = ServerInstance { install_dir: dir.clone(), ..Default::default() };
        read_files_into(&mut back);
        assert_eq!(back.network.port, 25570);
        assert_eq!(back.properties.get("motd").unwrap(), "§aNew");
        assert!(back.eula_accepted);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn humanises_countdowns() {
        assert_eq!(human_secs(300), "5 minutes");
        assert_eq!(human_secs(60), "1 minute");
        assert_eq!(human_secs(10), "10 seconds");
        assert_eq!(split_args("a \"b c\" d"), vec!["a", "b c", "d"]);
    }
}
