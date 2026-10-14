//! Every `#[tauri::command]`. Thin wrappers that delegate to `core::*`.

use crate::core::{backup, diagnostics, install, java, mcproto, mods, network, players, proc, props, server, sources, supervisor, versions, worlds};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::{manager_log, now_ms, AppState, TaskReporter};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type St<'a> = State<'a, Arc<AppState>>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| AppError::msg(e.to_string()))?
}

/// Run a long job in the background; errors become toasts. Returns immediately.
fn background<F>(app: &AppHandle, id: &str, what: &'static str, fut: F)
where
    F: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    let (app2, id) = (app.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        if let Err(e) = fut.await {
            crate::state::notify(&app2, &id, "error", format!("{what} failed: {e}"));
        }
    });
}

fn require_stopped(inst: &ServerInstance, what: &str) -> AppResult<()> {
    if server::running_pid(inst).is_some() {
        return Err(AppError::msg(format!("Stop the server first – {what}.")));
    }
    Ok(())
}

fn ms(t: std::time::SystemTime) -> i64 {
    chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()
}

// ---------------------------------------------------------------------------------------- app

#[tauri::command]
pub async fn get_app_info(state: St<'_>) -> AppResult<AppInfo> {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: state.data_dir.clone(),
        portable: state.portable,
        cores: sys.cpus().len(),
        total_memory: sys.total_memory(),
        os: sysinfo::System::long_os_version().unwrap_or_else(|| "Windows".into()),
        hostname: sysinfo::System::host_name().unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn get_settings(state: St<'_>) -> AppResult<AppSettings> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
pub async fn save_settings(state: St<'_>, settings: AppSettings) -> AppResult<()> {
    let mut s = settings;
    s.last_data_dir = Some(state.data_dir.clone());
    {
        // Watcher bookkeeping is owned by the backend.
        let cur = state.settings.read().await;
        s.last_seen_release = cur.last_seen_release.clone();
        s.last_seen_snapshot = cur.last_seen_snapshot.clone();
    }
    *state.settings.write().await = s;
    state.persist_settings().await
}

#[tauri::command]
pub async fn test_webhook(state: St<'_>, url: String) -> AppResult<()> {
    crate::state::send_webhook(&state.http, url.trim(), "success", "", "Test message from Minecraft Server Manager \u{2013} notifications work!").await
}

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn window_hide(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

#[tauri::command]
pub async fn cancel_task(state: St<'_>, task_id: String) -> AppResult<()> {
    if let Some(f) = state.cancels.lock().await.get(&task_id) {
        f.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderProbe {
    exists: bool,
    writable: bool,
    empty: bool,
    /// An existing Minecraft server lives there.
    detected: Option<InstallState>,
    free_bytes: Option<u64>,
}

#[tauri::command]
pub async fn probe_folder(path: String) -> AppResult<FolderProbe> {
    blocking(move || {
        let p = PathBuf::from(&path);
        let exists = p.exists();
        let empty = !exists || std::fs::read_dir(&p).map(|mut r| r.next().is_none()).unwrap_or(true);
        let detected = if exists { install::detect_existing(&p) } else { None };
        let probe_dir = if exists { p.clone() } else { p.ancestors().find(|a| a.exists()).map(|a| a.to_path_buf()).unwrap_or(p.clone()) };
        let t = probe_dir.join(format!(".mcsm-{}", uuid::Uuid::new_v4()));
        let writable = std::fs::write(&t, b"").is_ok();
        let _ = std::fs::remove_file(&t);
        let disks = sysinfo::Disks::new_with_refreshed_list();
        let n = proc::norm(&probe_dir);
        let free_bytes = disks.iter().filter(|d| n.starts_with(&proc::norm(d.mount_point()))).max_by_key(|d| d.mount_point().as_os_str().len()).map(|d| d.available_space());
        Ok(FolderProbe { exists, writable, empty, detected, free_bytes })
    })
    .await
}

// ---------------------------------------------------------------------------------- versions

#[tauri::command]
pub async fn game_versions(state: St<'_>, software: String, refresh: bool) -> AppResult<versions::VersionList> {
    if refresh {
        state.invalidate("mojang-manifest").await;
        state.invalidate(&format!("{software}")).await;
        state.invalidate("fill-").await;
    }
    versions::game_versions(&state, &software).await
}

#[tauri::command]
pub async fn loader_versions(state: St<'_>, software: String, mc: String) -> AppResult<Vec<versions::LoaderVersion>> {
    versions::loader_versions(&state, &software, &mc).await
}

#[tauri::command]
pub async fn version_meta(state: St<'_>, mc: String) -> AppResult<versions::VersionMeta> {
    versions::version_meta(&state, &mc).await
}

// ---------------------------------------------------------------------------------- instances

#[tauri::command]
pub async fn list_instances(state: St<'_>) -> AppResult<Vec<ServerInstance>> {
    Ok(state.instances.read().await.clone())
}

#[tauri::command]
pub async fn get_statuses(state: St<'_>) -> AppResult<Vec<ServerStatus>> {
    let list = state.instances.read().await.clone();
    let mut out = vec![];
    for i in &list {
        out.push(state.status(i).await);
    }
    Ok(out)
}

#[tauri::command]
pub async fn suggest_ports(state: St<'_>, exclude_id: Option<String>) -> AppResult<(u16, u16)> {
    let list = state.instances.read().await.clone();
    Ok(network::suggest_ports(&list, exclude_id.as_deref()).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInput {
    name: String,
    install_dir: Option<PathBuf>,
    color: Option<String>,
    software: String,
    mc_version: String,
    loader_version: String,
    port: Option<u16>,
    max_mem_mb: Option<u32>,
    eula_accepted: bool,
    install_now: bool,
    template_id: Option<String>,
    #[serde(default)]
    properties: BTreeMap<String, String>,
    modpack: Option<ModpackRef>,
}

/// Folders must be absolute – a relative path would resolve against wherever the app runs.
fn abs_dir(p: PathBuf) -> AppResult<PathBuf> {
    let s = p.to_string_lossy().trim().to_string();
    let p = PathBuf::from(&s);
    if !p.is_absolute() {
        return Err(AppError::msg(format!("“{s}” isn't a full folder path – use something like D:\\MinecraftServers\\Survival.")));
    }
    Ok(p)
}

fn slug(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() {
        "server".into()
    } else {
        s.chars().take(40).collect()
    }
}

async fn unique_dir(state: &AppState, name: &str) -> PathBuf {
    let root = state.default_install_root().await;
    let base = slug(name);
    let mut p = root.join(&base);
    let mut n = 2;
    let used: Vec<String> = state.instances.read().await.iter().map(|i| proc::norm(&i.install_dir)).collect();
    while p.exists() && std::fs::read_dir(&p).map(|mut r| r.next().is_some()).unwrap_or(false) || used.contains(&proc::norm(&p)) {
        p = root.join(format!("{base}-{n}"));
        n += 1;
    }
    p
}

async fn add_instance(state: &AppState, inst: ServerInstance) -> AppResult<ServerInstance> {
    {
        let mut list = state.instances.write().await;
        if list.iter().any(|i| proc::norm(&i.install_dir) == proc::norm(&inst.install_dir)) {
            return Err(AppError::msg("Another server profile already uses that folder."));
        }
        list.push(inst.clone());
    }
    state.persist_instances().await?;
    Ok(inst)
}

async fn replace_instance(state: &AppState, inst: &ServerInstance) -> AppResult<()> {
    {
        let mut list = state.instances.write().await;
        if let Some(slot) = list.iter_mut().find(|i| i.id == inst.id) {
            *slot = inst.clone();
        }
    }
    state.persist_instances().await
}

/// Sensible starting values for a fresh server.properties.
fn default_properties() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for (k, v) in [
        ("motd", "§aA Minecraft Server §7– §fpowered by §bMSM"),
        ("max-players", "20"),
        ("difficulty", "normal"),
        ("gamemode", "survival"),
        ("view-distance", "10"),
        ("simulation-distance", "10"),
        ("online-mode", "true"),
        ("white-list", "false"),
        ("enforce-whitelist", "false"),
        ("pvp", "true"),
        ("spawn-protection", "16"),
        ("level-name", "world"),
    ] {
        m.insert(k.to_string(), v.to_string());
    }
    m
}

#[tauri::command]
pub async fn create_instance(app: AppHandle, state: St<'_>, input: CreateInput) -> AppResult<ServerInstance> {
    let name = if input.name.trim().is_empty() { "Minecraft Server".to_string() } else { input.name.trim().to_string() };
    let mut inst = match &input.template_id {
        Some(t) => {
            let mut i = state.instance(t).await?;
            i.installed = None;
            i.modpack = None;
            i
        }
        None => ServerInstance::default(),
    };
    inst.id = uuid::Uuid::new_v4().to_string();
    inst.name = name.clone();
    inst.created_at = now_ms();
    inst.color = input.color.unwrap_or_else(|| "grass".into());
    inst.install_dir = match input.install_dir {
        Some(d) if !d.as_os_str().is_empty() => abs_dir(d)?,
        _ => unique_dir(&state, &name).await,
    };
    inst.software = input.software.trim().to_lowercase();
    inst.mc_version = input.mc_version.trim().to_string();
    inst.loader_version = input.loader_version.trim().to_string();
    inst.eula_accepted = input.eula_accepted;
    inst.modpack = input.modpack.clone();
    if let Some(m) = input.max_mem_mb {
        inst.java.max_mem_mb = m.clamp(512, 1024 * 1024);
        inst.java.min_mem_mb = inst.java.min_mem_mb.min(inst.java.max_mem_mb);
    }
    let list = state.instances.read().await.clone();
    let (port, rcon) = network::suggest_ports(&list, None).await;
    inst.network.port = input.port.filter(|p| *p > 0).unwrap_or(port);
    inst.network.rcon_port = if input.port.is_some() { inst.network.port.saturating_add(10) } else { rcon };
    inst.network.rcon_password = server::random_password(24);
    if input.template_id.is_none() {
        inst.properties = default_properties();
    }
    for (k, v) in input.properties {
        inst.properties.insert(k, v);
    }
    std::fs::create_dir_all(&inst.install_dir)?;
    // A server already in that folder keeps its worlds and settings.
    if let Some(found) = install::detect_existing(&inst.install_dir) {
        let mut probe = inst.clone();
        server::read_files_into(&mut probe);
        let overlay = server::network_overlay(&inst);
        inst.properties.extend(probe.properties.into_iter().filter(|(k, _)| !overlay.contains_key(k)));
        if inst.software == found.software && (inst.mc_version.is_empty() || inst.mc_version == found.mc_version) {
            inst.installed = Some(found);
        }
    }
    let _ = server::sync_files(&inst);
    let inst = add_instance(&state, inst).await?;
    if let Some(mp) = input.modpack {
        let (st, app2, id) = (state.inner().clone(), app.clone(), inst.id.clone());
        background(&app, &inst.id, "Modpack install", async move { install_modpack_job(&st, &app2, &id, &mp.version_id, false).await });
    } else if input.install_now && !inst.is_installed() {
        let (st, app2, id) = (state.inner().clone(), app.clone(), inst.id.clone());
        background(&app, &inst.id, "Install", async move { server::install_job(&st, &app2, &id).await });
    }
    state.emit_status(&app, &inst.id).await;
    Ok(inst)
}

/// New server from a Modrinth modpack: download the .mrpack, set software/version from it,
/// install the loader, then the pack's server files and overrides.
async fn install_modpack_job(state: &Arc<AppState>, app: &AppHandle, id: &str, version_id: &str, replace_mods: bool) -> AppResult<()> {
    let inst = state.instance(id).await?;
    let r = TaskReporter::new(app, "modpack", &format!("Modpack for {}", inst.name), Some(id));
    let res: AppResult<()> = async {
        r.progress("Downloading the modpack…", None);
        tokio::fs::create_dir_all(inst.meta_dir()).await?;
        let (pack, _) = sources::fetch_mrpack(state, version_id, &inst.meta_dir(), &r).await?;
        let p2 = pack.clone();
        let (_, info) = blocking(move || sources::read_mrpack(&p2)).await?;
        {
            let mut list = state.instances.write().await;
            if let Some(i) = list.iter_mut().find(|i| i.id == id) {
                i.software = info.software.clone();
                i.mc_version = info.mc_version.clone();
                i.loader_version = info.loader_version.clone();
                if let Some(mp) = i.modpack.as_mut() {
                    if mp.version_name.is_empty() {
                        mp.version_name = info.version_name.clone();
                    }
                }
                // Modpacks need room.
                if i.java.max_mem_mb < 6144 {
                    i.java.max_mem_mb = 6144;
                }
            }
        }
        state.persist_instances().await?;
        let _ = app.emit("instances-changed", ());
        r.progress(&format!("Installing {} {} for the pack…", software_label(&info.software), info.mc_version), None);
        server::install_job(state, app, id).await?;
        let inst = state.instance(id).await?;
        if replace_mods {
            // Mods of the previous setup would clash with the pack's (duplicates, wrong loader).
            let dir = inst.install_dir.join("mods");
            let has_jars = std::fs::read_dir(&dir).map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().contains(".jar"))).unwrap_or(false);
            if has_jars {
                let (root, d2) = (inst.install_dir.clone(), dir.clone());
                let (to, n) = blocking(move || backup::quarantine_contents(&root, &d2, "mods-before-modpack")).await?;
                manager_log(app, id, format!("{n} previous mod file(s) moved to {}", to.display()));
            }
        }
        let n = sources::apply_mrpack(state, &inst, &pack, &r).await?;
        let _ = tokio::fs::remove_file(&pack).await;
        r.done(&format!("{} installed – {n} files", info.name));
        let _ = app.emit("instances-changed", ());
        Ok(())
    }
    .await;
    if let Err(e) = &res {
        r.fail(&e.to_string());
    }
    res
}

#[tauri::command]
pub async fn update_instance(app: AppHandle, state: St<'_>, instance: ServerInstance) -> AppResult<ServerInstance> {
    let old = state.instance(&instance.id).await?;
    let mut inst = instance;
    inst.install_dir = abs_dir(inst.install_dir)?;
    if inst.network.port == 0 {
        return Err(AppError::msg("The server port can't be 0."));
    }
    if inst.network.rcon_enabled && inst.network.rcon_port == inst.network.port {
        return Err(AppError::msg("RCON needs its own port (not the game port)."));
    }
    if inst.java.max_mem_mb < 512 {
        return Err(AppError::msg("Give the server at least 512 MB of memory."));
    }
    // The installed state belongs to the installer, not the editor.
    inst.installed = old.installed.clone();
    let running = server::running_pid(&old);
    if proc::norm(&old.install_dir) != proc::norm(&inst.install_dir) && running.is_some() {
        return Err(AppError::msg("Stop the server before changing its folder."));
    }
    if inst.network.rcon_enabled && inst.network.rcon_password.trim().is_empty() {
        inst.network.rcon_password = server::random_password(24);
    }
    replace_instance(&state, &inst).await?;
    let restart_needed = old.properties != inst.properties
        || old.network != inst.network && (old.network.port != inst.network.port || old.network.bind_ip != inst.network.bind_ip || old.network.rcon_enabled != inst.network.rcon_enabled || old.network.rcon_port != inst.network.rcon_port || old.network.rcon_password != inst.network.rcon_password || old.network.query_enabled != inst.network.query_enabled || old.network.query_port != inst.network.query_port)
        || old.java.min_mem_mb != inst.java.min_mem_mb
        || old.java.max_mem_mb != inst.java.max_mem_mb
        || old.java.gc_preset != inst.java.gc_preset
        || old.java.jvm_args != inst.java.jvm_args
        || old.java.server_args != inst.java.server_args
        || old.java.path != inst.java.path;
    match running {
        Some(pid) => {
            if old.java.priority != inst.java.priority || old.java.affinity != inst.java.affinity {
                let _ = proc::apply_priority_affinity(pid, &inst.java.priority, &inst.java.affinity);
            }
            if restart_needed {
                state.with_runtime(&inst.id, |r| r.pending_restart = true).await;
            }
            // server.properties can be written while running – the server only reads it at start.
            let i2 = inst.clone();
            blocking(move || server::sync_files(&i2)).await?;
        }
        None => {
            let i2 = inst.clone();
            blocking(move || server::sync_files(&i2)).await?;
        }
    }
    state.emit_status(&app, &inst.id).await;
    let _ = app.emit("instances-changed", ());
    Ok(inst)
}

#[tauri::command]
pub async fn reorder_instances(state: St<'_>, ids: Vec<String>) -> AppResult<()> {
    {
        let mut list = state.instances.write().await;
        list.sort_by_key(|i| ids.iter().position(|x| x == &i.id).unwrap_or(usize::MAX));
    }
    state.persist_instances().await
}

#[tauri::command]
pub async fn delete_instance(app: AppHandle, state: St<'_>, id: String, delete_files: bool) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    if server::running_pid(&inst).is_some() {
        server::stop(&state, &app, &id, false).await?;
    }
    state.instances.write().await.retain(|i| i.id != id);
    state.persist_instances().await?;
    state.runtime.lock().await.remove(&id);
    if delete_files && inst.install_dir.exists() {
        // Keep backups: they live inside the folder by default, so move them out first.
        let dir = inst.install_dir.clone();
        let keep = state.data_dir.join("deleted-server-backups").join(&id);
        blocking(move || {
            let bk = dir.join("backups");
            if bk.exists() && std::fs::read_dir(&bk).map(|mut r| r.next().is_some()).unwrap_or(false) {
                std::fs::create_dir_all(keep.parent().unwrap())?;
                // A rename fails across drives (server on D:, app data on C:) – copy instead, and
                // never delete the folder while its backups haven't been saved.
                if std::fs::rename(&bk, &keep).is_err() {
                    copy_dir(&bk, &keep).map_err(|e| AppError::msg(format!("Couldn't keep the backups ({e}) – nothing was deleted.")))?;
                }
            }
            std::fs::remove_dir_all(&dir)?;
            Ok(())
        })
        .await?;
    }
    Ok(())
}

fn copy_dir(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    for e in walkdir::WalkDir::new(src).into_iter() {
        let e = e.map_err(std::io::Error::other)?;
        let out = dest.join(e.path().strip_prefix(src).unwrap());
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            std::fs::copy(e.path(), &out)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn clone_instance(app: AppHandle, state: St<'_>, id: String, name: String, copy_worlds: bool) -> AppResult<ServerInstance> {
    let src = state.instance(&id).await?;
    let mut inst = src.clone();
    inst.id = uuid::Uuid::new_v4().to_string();
    inst.name = name.trim().to_string();
    inst.created_at = now_ms();
    inst.install_dir = unique_dir(&state, &inst.name).await;
    let list = state.instances.read().await.clone();
    let (port, rcon) = network::suggest_ports(&list, None).await;
    inst.network.port = port;
    inst.network.rcon_port = rcon;
    inst.network.rcon_password = server::random_password(24);
    inst.network.upnp = false;
    let (s2, d2) = (src.clone(), inst.clone());
    blocking(move || {
        // Everything except worlds (optional), logs, backups and the manager's own state.
        let skip_top = ["logs", "crash-reports", "backups", META_DIR, "cache", "debug"];
        let worlds: Vec<String> = backup::world_dirs(&s2).into_iter().map(|(n, _)| n).collect();
        for e in walkdir::WalkDir::new(&s2.install_dir).min_depth(1).into_iter().filter_entry(|e| {
            let rel = e.path().strip_prefix(&s2.install_dir).unwrap();
            let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
            !skip_top.contains(&top.as_str()) && (copy_worlds || !worlds.contains(&top))
        }) {
            let Ok(e) = e else { continue };
            let rel = e.path().strip_prefix(&s2.install_dir).unwrap();
            let out = d2.install_dir.join(rel);
            if e.file_type().is_dir() {
                std::fs::create_dir_all(&out)?;
            } else if e.file_type().is_file() && e.file_name() != "session.lock" {
                std::fs::create_dir_all(out.parent().unwrap())?;
                std::fs::copy(e.path(), &out)?;
            }
        }
        if let Some(st) = &s2.installed {
            let _ = crate::state::write_json_atomic(&d2.meta_dir().join("install.json"), st);
        }
        server::sync_files(&d2)
    })
    .await?;
    let inst = add_instance(&state, inst).await?;
    state.emit_status(&app, &inst.id).await;
    Ok(inst)
}

#[tauri::command]
pub async fn import_existing(app: AppHandle, state: St<'_>, path: PathBuf, name: Option<String>) -> AppResult<ServerInstance> {
    let path = abs_dir(path)?;
    let p2 = path.clone();
    let found = blocking(move || Ok(install::detect_existing(&p2))).await?.ok_or_else(|| {
        AppError::msg("No Minecraft server found in that folder (looked for server.jar, paper/purpur/folia jars, Fabric/Quilt launchers and Forge/NeoForge libraries).")
    })?;
    let mut inst = ServerInstance {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: now_ms(),
        install_dir: path.clone(),
        software: found.software.clone(),
        mc_version: found.mc_version.clone(),
        loader_version: found.loader_version.clone(),
        ..Default::default()
    };
    server::read_files_into(&mut inst);
    let mut found = found;
    if found.java_major == 0 && !found.mc_version.is_empty() {
        found.java_major = versions::version_meta(&state, &found.mc_version).await.map(|m| m.java_major).unwrap_or_else(|_| versions::java_guess(&found.mc_version));
    }
    if found.java_major == 0 {
        found.java_major = 21;
    }
    inst.installed = Some(found);
    inst.name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported server".into()));
    for k in server::network_overlay(&inst).keys() {
        inst.properties.remove(k);
    }
    let list = state.instances.read().await.clone();
    if list.iter().any(|i| i.tcp_ports().contains(&inst.network.port)) {
        let (p, r) = network::suggest_ports(&list, None).await;
        inst.network.port = p;
        inst.network.rcon_port = r;
    }
    if inst.network.rcon_password.is_empty() {
        inst.network.rcon_password = server::random_password(24);
    }
    let inst = add_instance(&state, inst).await?;
    state.emit_status(&app, &inst.id).await;
    Ok(inst)
}

// ---------------------------------------------------------------------------------- lifecycle

#[tauri::command]
pub async fn install_server(app: AppHandle, state: St<'_>, id: String) -> AppResult<()> {
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    background(&app, &id, "Install", async move { server::install_job(&st, &app2, &id2).await });
    Ok(())
}

/// Switch software / Minecraft version / loader. Downgrades of an existing world need consent.
#[tauri::command]
pub async fn change_version(app: AppHandle, state: St<'_>, id: String, software: String, mc_version: String, loader_version: String, allow_downgrade: bool) -> AppResult<()> {
    let mut inst = state.instance(&id).await?;
    if let Some(cur) = &inst.installed {
        if !allow_downgrade && !mc_version.is_empty() && inst.world_dir().join("level.dat").exists() && versions::is_newer(&state, &cur.mc_version, &mc_version).await == Some(true) {
            return Err(AppError::msg(format!(
                "Minecraft {mc_version} is older than {} – opening a world in an older version can corrupt it. Confirm the downgrade to continue (a backup is made first).",
                cur.mc_version
            )));
        }
    }
    inst.software = software.trim().to_lowercase();
    inst.mc_version = mc_version.trim().to_string();
    inst.loader_version = loader_version.trim().to_string();
    replace_instance(&state, &inst).await?;
    let _ = app.emit("instances-changed", ());
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    background(&app, &id, "Version change", async move { server::install_job(&st, &app2, &id2).await });
    Ok(())
}

#[tauri::command]
pub async fn check_update(app: AppHandle, state: St<'_>, id: String) -> AppResult<Option<UpdateInfo>> {
    let st = state.inner().clone();
    supervisor::check_update(&st, &app, &id, false).await
}

#[tauri::command]
pub async fn start_server(app: AppHandle, state: St<'_>, id: String) -> AppResult<()> {
    state.with_runtime(&id, |r| r.crash_loop = false).await;
    server::start(&state, &app, &id).await
}

#[tauri::command]
pub async fn stop_server(app: AppHandle, state: St<'_>, id: String, force: bool) -> AppResult<()> {
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    // Graceful stops take a while (world save) – return at once, status events follow.
    background(&app, &id, "Stop", async move { server::stop(&st, &app2, &id2, force).await });
    Ok(())
}

#[tauri::command]
pub async fn restart_server(app: AppHandle, state: St<'_>, id: String) -> AppResult<()> {
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    background(&app, &id, "Restart", async move { server::restart(&st, &app2, &id2).await });
    Ok(())
}

/// Restart or stop after warning players in-game for `secs` seconds.
#[tauri::command]
pub async fn start_countdown(app: AppHandle, state: St<'_>, id: String, action: String, secs: u32) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let lead = secs.max(5);
    let at = now_ms() + lead as i64 * 1000;
    state
        .with_runtime(&id, |r| {
            r.countdown = Some(NextSchedule { at, action: action.clone() });
            r.fired_schedule_keys.insert(format!("cd-{at}-{lead}"));
        })
        .await;
    let msg = server::warning_text(&inst, &action);
    server::say(&state, &app, &id, &msg.replace("{time}", &server::human_secs(lead))).await;
    state.emit_status(&app, &id).await;
    Ok(())
}

#[tauri::command]
pub async fn cancel_countdown(app: AppHandle, state: St<'_>, id: String) -> AppResult<()> {
    let had = state.with_runtime(&id, |r| r.countdown.take()).await;
    if let Some(cd) = had {
        let what = match cd.action.as_str() {
            "stop" => "Shutdown",
            "update" => "Update",
            _ => "Restart",
        };
        server::say(&state, &app, &id, &format!("{what} cancelled.")).await;
        manager_log(&app, &id, "Countdown cancelled.");
    }
    state.emit_status(&app, &id).await;
    Ok(())
}

#[tauri::command]
pub async fn send_command(app: AppHandle, state: St<'_>, id: String, command: String) -> AppResult<Option<String>> {
    server::send_command(&state, &app, &id, &command).await
}

#[tauri::command]
pub async fn get_launch_line(state: St<'_>, id: String) -> AppResult<String> {
    let inst = state.instance(&id).await?;
    let java = diagnostics::resolve_java(&state, &inst).await.map(|p| p.display().to_string()).unwrap_or_else(|| "java".into());
    Ok(server::launch_line(&inst, &java))
}

#[tauri::command]
pub async fn preview_launch_line(state: St<'_>, instance: ServerInstance) -> AppResult<String> {
    let java = diagnostics::resolve_java(&state, &instance).await.map(|p| p.display().to_string()).unwrap_or_else(|| "java".into());
    Ok(server::launch_line(&instance, &java))
}

#[tauri::command]
pub async fn clear_crash_state(app: AppHandle, state: St<'_>, id: String) -> AppResult<()> {
    state
        .with_runtime(&id, |r| {
            r.crash_loop = false;
            r.crash_times.clear();
            r.restart_at = None;
            if r.state == Some(ServerState::Crashed) {
                r.state = Some(ServerState::Stopped);
            }
        })
        .await;
    state.emit_status(&app, &id).await;
    Ok(())
}

// ----------------------------------------------------------------------------------------- java

#[tauri::command]
pub async fn list_java(state: St<'_>) -> AppResult<Vec<java::JavaRuntime>> {
    let dir = state.java_dir().await;
    blocking(move || Ok(java::detect(&dir))).await
}

#[tauri::command]
pub async fn probe_java(path: PathBuf) -> AppResult<java::JavaRuntime> {
    java::probe_exe(&path).await
}

#[tauri::command]
pub async fn install_java(app: AppHandle, state: St<'_>, feature: u32) -> AppResult<()> {
    let st = state.inner().clone();
    let app2 = app.clone();
    background(&app, "", "Java download", async move {
        let r = TaskReporter::new(&app2, "java", &format!("Downloading Java {feature}"), None);
        match java::install_temurin(&st, feature, Some(&r)).await {
            Ok(p) => {
                r.done(&format!("Java {feature} installed – {}", p.display()));
                Ok(())
            }
            Err(e) => {
                r.fail(&e.to_string());
                Err(e)
            }
        }
    });
    Ok(())
}

// -------------------------------------------------------------------------------- config files

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntry {
    rel: String,
    size: u64,
    modified: i64,
}

fn resolve_rel(inst: &ServerInstance, rel: &str) -> AppResult<PathBuf> {
    let rel = rel.replace('\\', "/");
    if rel.split('/').any(|c| c == "..") || rel.starts_with('/') || rel.contains(':') {
        return Err(AppError::msg("That path is outside the server folder."));
    }
    let p = inst.install_dir.join(&rel);
    if !proc::is_within(&p, &inst.install_dir) {
        return Err(AppError::msg("That path is outside the server folder."));
    }
    Ok(p)
}

#[tauri::command]
pub async fn list_config_files(state: St<'_>, id: String) -> AppResult<Vec<ConfigEntry>> {
    let inst = state.instance(&id).await?;
    blocking(move || {
        Ok(server::config_candidates(&inst)
            .into_iter()
            .filter_map(|p| {
                let m = std::fs::metadata(&p).ok()?;
                Some(ConfigEntry {
                    rel: p.strip_prefix(&inst.install_dir).ok()?.to_string_lossy().replace('\\', "/"),
                    size: m.len(),
                    modified: m.modified().map(ms).unwrap_or(0),
                })
            })
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn read_config(state: St<'_>, id: String, rel: String) -> AppResult<ConfigFile> {
    let inst = state.instance(&id).await?;
    let path = resolve_rel(&inst, &rel)?;
    blocking(move || {
        let meta = std::fs::metadata(&path).ok();
        let text = match std::fs::read(&path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(_) => String::new(),
        };
        Ok(ConfigFile { exists: meta.is_some(), modified: meta.and_then(|m| m.modified().ok()).map(ms), text, path })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    restart_needed: bool,
    instance: ServerInstance,
}

/// Save a config file (keeping the previous 30 versions in `.mcsm/history`). Hand edits of
/// server.properties flow back into the profile.
#[tauri::command]
pub async fn write_config(app: AppHandle, state: St<'_>, id: String, rel: String, text: String) -> AppResult<WriteResult> {
    let mut inst = state.instance(&id).await?;
    let path = resolve_rel(&inst, &rel)?;
    let hist = inst.meta_dir().join("history");
    let (p2, t2, r2) = (path.clone(), text.clone(), rel.clone());
    blocking(move || {
        if p2.exists() {
            std::fs::create_dir_all(&hist)?;
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let flat = r2.replace(['/', '\\'], "__");
            let _ = std::fs::copy(&p2, hist.join(format!("{flat}.{stamp}")));
            let mut old: Vec<PathBuf> = std::fs::read_dir(&hist)?.flatten().map(|e| e.path()).filter(|p| p.file_name().map(|n| n.to_string_lossy().starts_with(&format!("{flat}."))).unwrap_or(false)).collect();
            old.sort();
            while old.len() > 30 {
                let _ = std::fs::remove_file(old.remove(0));
            }
        }
        if let Some(p) = p2.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&p2, t2)?;
        Ok(())
    })
    .await?;
    let running = server::running_pid(&inst).is_some();
    if rel.eq_ignore_ascii_case("server.properties") {
        let doc = props::PropsDoc::parse(&text);
        let mut back = inst.clone();
        back.properties = doc.map();
        server::read_files_into(&mut back);
        for k in server::network_overlay(&back).keys() {
            back.properties.remove(k);
        }
        inst.properties = back.properties;
        inst.network = back.network;
        replace_instance(&state, &inst).await?;
        let _ = app.emit("instances-changed", ());
    }
    if running {
        state.with_runtime(&id, |r| r.pending_restart = true).await;
    }
    state.emit_status(&app, &id).await;
    Ok(WriteResult { restart_needed: running, instance: inst })
}

#[tauri::command]
pub async fn list_config_history(state: St<'_>, id: String, rel: String) -> AppResult<Vec<(String, i64)>> {
    let inst = state.instance(&id).await?;
    let dir = inst.meta_dir().join("history");
    let flat = rel.replace(['/', '\\'], "__");
    blocking(move || {
        let mut v: Vec<(String, i64)> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{flat}.")))
                    .map(|e| (e.path().to_string_lossy().into_owned(), e.metadata().ok().and_then(|m| m.modified().ok()).map(ms).unwrap_or(0)))
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(v)
    })
    .await
}

#[tauri::command]
pub async fn read_text_file(state: St<'_>, id: String, path: PathBuf) -> AppResult<String> {
    let inst = state.instance(&id).await?;
    if !proc::is_within(&path, &inst.install_dir) {
        return Err(AppError::msg("That file is outside the server folder."));
    }
    blocking(move || {
        let b = std::fs::read(&path)?;
        // Rotated logs are gzipped.
        if b.starts_with(&[0x1f, 0x8b]) {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&b[..]), &mut s)?;
            return Ok(s);
        }
        Ok(String::from_utf8_lossy(&b).into_owned())
    })
    .await
}

#[tauri::command]
pub async fn get_server_icon(state: St<'_>, id: String) -> AppResult<Option<String>> {
    let inst = state.instance(&id).await?;
    Ok(std::fs::read(inst.install_dir.join("server-icon.png")).ok().map(|b| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(b))))
}

/// Save a 64×64 PNG (the UI scales the picked image) as server-icon.png. Empty = remove.
#[tauri::command]
pub async fn set_server_icon(state: St<'_>, id: String, png_base64: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let p = inst.install_dir.join("server-icon.png");
    if png_base64.is_empty() {
        let _ = std::fs::remove_file(p);
        return Ok(());
    }
    let data = png_base64.split(',').next_back().unwrap_or_default();
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).map_err(|_| AppError::msg("That image couldn't be read."))?;
    if !bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Err(AppError::msg("The server icon must be a PNG."));
    }
    std::fs::create_dir_all(&inst.install_dir)?;
    std::fs::write(p, bytes)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------- logs

#[tauri::command]
pub async fn read_log_tail(state: St<'_>, id: String, max_lines: usize) -> AppResult<Vec<String>> {
    let inst = state.instance(&id).await?;
    blocking(move || {
        let path = if inst.console_log().exists() { inst.console_log() } else { inst.logs_dir().join("latest.log") };
        let Ok(meta) = std::fs::metadata(&path) else { return Ok(vec![]) };
        use std::io::{Read, Seek, SeekFrom};
        let len = meta.len();
        let start = len.saturating_sub(1_048_576);
        let mut f = std::fs::File::open(&path)?;
        f.seek(SeekFrom::Start(start))?;
        let mut buf = vec![];
        f.read_to_end(&mut buf)?;
        let text = String::from_utf8_lossy(&buf);
        let lines: Vec<String> = text.lines().map(|l| mcproto::strip_codes(l)).collect();
        let skip = lines.len().saturating_sub(max_lines.clamp(10, 20_000));
        Ok(lines[skip..].to_vec())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    path: PathBuf,
    name: String,
    /// log | crash | console
    kind: String,
    size: u64,
    modified: i64,
}

#[tauri::command]
pub async fn list_log_files(state: St<'_>, id: String) -> AppResult<Vec<LogFile>> {
    let inst = state.instance(&id).await?;
    blocking(move || {
        let mut v = vec![];
        for (dir, kind) in [(inst.logs_dir(), "log"), (inst.install_dir.join("crash-reports"), "crash"), (inst.meta_dir(), "console")] {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if !(n.ends_with(".log") || n.ends_with(".log.gz") || n.ends_with(".txt")) {
                    continue;
                }
                let Ok(m) = e.metadata() else { continue };
                v.push(LogFile { path: e.path(), name: n, kind: kind.into(), size: m.len(), modified: m.modified().map(ms).unwrap_or(0) });
            }
        }
        v.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(v)
    })
    .await
}

// -------------------------------------------------------------------------------------- worlds

#[tauri::command]
pub async fn list_worlds(state: St<'_>, id: String) -> AppResult<Vec<worlds::WorldInfo>> {
    let inst = state.instance(&id).await?;
    blocking(move || Ok(worlds::list(&inst))).await
}

#[tauri::command]
pub async fn activate_world(app: AppHandle, state: St<'_>, id: String, name: String) -> AppResult<ServerInstance> {
    let mut inst = state.instance(&id).await?;
    inst.properties.insert("level-name".into(), name);
    let running = server::running_pid(&inst).is_some();
    replace_instance(&state, &inst).await?;
    let i2 = inst.clone();
    blocking(move || server::sync_files(&i2)).await?;
    if running {
        state.with_runtime(&id, |r| r.pending_restart = true).await;
    }
    state.emit_status(&app, &id).await;
    let _ = app.emit("instances-changed", ());
    Ok(inst)
}

#[tauri::command]
pub async fn import_world(state: St<'_>, id: String, path: PathBuf, name: Option<String>) -> AppResult<String> {
    let inst = state.instance(&id).await?;
    blocking(move || worlds::import(&inst, &path, name.as_deref())).await
}

#[tauri::command]
pub async fn export_world(app: AppHandle, state: St<'_>, id: String, name: String, dest: PathBuf) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    if server::running_pid(&inst).is_some() && name == inst.level_name() {
        // Flush chunks so the export is consistent.
        let _ = server::send_command(&state, &app, &id, "save-all flush").await;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    blocking(move || worlds::export(&inst, &name, &dest).map(|_| ())).await
}

#[tauri::command]
pub async fn delete_world(state: St<'_>, id: String, name: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    if name == inst.level_name() {
        require_stopped(&inst, "the server is using this world")?;
    }
    blocking(move || worlds::delete(&inst, &name).map(|_| ())).await
}

/// Regenerate a world (all / nether / end), optionally with a new seed. A backup is made first.
#[tauri::command]
pub async fn reset_world(app: AppHandle, state: St<'_>, id: String, name: String, what: String, seed: Option<String>) -> AppResult<u32> {
    let mut inst = state.instance(&id).await?;
    require_stopped(&inst, "worlds can only be reset while the server is off")?;
    if let Ok(b) = backup::create(&state, &inst, &format!("Before {what} reset"), "auto").await {
        let _ = app.emit("backup-created", &b);
    }
    if let Some(s) = seed {
        if what == "all" {
            inst.properties.insert("level-seed".into(), s.trim().to_string());
            replace_instance(&state, &inst).await?;
            let _ = server::sync_files(&inst);
            let _ = app.emit("instances-changed", ());
        }
    }
    blocking(move || worlds::reset(&inst, &name, &what)).await
}

#[tauri::command]
pub async fn rename_world(app: AppHandle, state: St<'_>, id: String, name: String, new_name: String) -> AppResult<()> {
    let mut inst = state.instance(&id).await?;
    let active = name == inst.level_name();
    if active {
        require_stopped(&inst, "the server is using this world")?;
    }
    let (i2, n2, nn) = (inst.clone(), name.clone(), new_name.clone());
    blocking(move || worlds::rename(&i2, &n2, &nn)).await?;
    if active {
        inst.properties.insert("level-name".into(), new_name.trim().to_string());
        replace_instance(&state, &inst).await?;
        let _ = server::sync_files(&inst);
        let _ = app.emit("instances-changed", ());
    }
    Ok(())
}

#[tauri::command]
pub async fn list_datapacks(state: St<'_>, id: String, world: String) -> AppResult<Vec<worlds::Datapack>> {
    let inst = state.instance(&id).await?;
    blocking(move || Ok(worlds::datapacks(&inst, &world))).await
}

#[tauri::command]
pub async fn add_datapacks(state: St<'_>, id: String, world: String, paths: Vec<PathBuf>) -> AppResult<Vec<String>> {
    let inst = state.instance(&id).await?;
    blocking(move || worlds::add_datapacks(&inst, &world, &paths)).await
}

#[tauri::command]
pub async fn remove_datapack(state: St<'_>, id: String, world: String, name: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    blocking(move || worlds::remove_datapack(&inst, &world, &name)).await
}

// ------------------------------------------------------------------------------------- players

#[tauri::command]
pub async fn player_lists(state: St<'_>, id: String) -> AppResult<players::PlayerLists> {
    let inst = state.instance(&id).await?;
    blocking(move || Ok(players::read_lists(&inst))).await
}

/// Add/remove a player (or IP) on whitelist / ops / banned-players / banned-ips.
#[tauri::command]
pub async fn player_action(app: AppHandle, state: St<'_>, id: String, list: String, add: bool, target: String, reason: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let target = target.trim().to_string();
    if target.is_empty() {
        return Err(AppError::msg("Enter a player name."));
    }
    let live = state.with_runtime(&id, |r| r.state == Some(ServerState::Running)).await && server::running_pid(&inst).is_some();
    if live {
        let cmd = players::command_for(&list, add, &target, &reason)?;
        server::send_command(&state, &app, &id, &cmd).await?;
        return Ok(());
    }
    let online_mode = inst.properties.get("online-mode").map(|v| v != "false").unwrap_or(true);
    let (uuid, name) = if list == "banned-ips" {
        if target.parse::<std::net::IpAddr>().is_err() {
            return Err(AppError::msg(format!("“{target}” isn't an IP address.")));
        }
        (None, target.clone())
    } else if add {
        let (u, n) = players::lookup(&state.http, &target, online_mode).await?;
        (Some(u), n)
    } else {
        (None, target.clone())
    };
    let level = inst.properties.get("op-permission-level").and_then(|v| v.parse().ok()).unwrap_or(4);
    blocking(move || players::edit_file(&inst, &list, add, &name, uuid.as_deref(), &reason, level)).await
}

#[tauri::command]
pub async fn player_stats(state: St<'_>, id: String) -> AppResult<Vec<players::PlayerStats>> {
    let inst = state.instance(&id).await?;
    blocking(move || Ok(players::stats(&inst))).await
}

#[tauri::command]
pub async fn kick_player(app: AppHandle, state: St<'_>, id: String, name: String, reason: String) -> AppResult<()> {
    let r = if reason.trim().is_empty() { String::new() } else { format!(" {}", reason.trim()) };
    server::send_command(&state, &app, &id, &format!("kick {name}{r}")).await.map(|_| ())
}

#[tauri::command]
pub async fn lookup_player(state: St<'_>, name: String, online_mode: bool) -> AppResult<(String, String)> {
    players::lookup(&state.http, &name, online_mode).await
}

// ---------------------------------------------------------------------------- mods & plugins

#[tauri::command]
pub async fn list_content(state: St<'_>, id: String) -> AppResult<mods::ContentState> {
    let inst = state.instance(&id).await?;
    blocking(move || Ok(mods::list(&inst))).await
}

#[tauri::command]
pub async fn add_content_files(state: St<'_>, id: String, paths: Vec<PathBuf>) -> AppResult<Vec<String>> {
    let inst = state.instance(&id).await?;
    let added = blocking(move || mods::add_files(&inst, &paths)).await?;
    let inst = state.instance(&id).await?;
    let _ = sources::modrinth_identify(&state, &inst).await;
    Ok(added)
}

#[tauri::command]
pub async fn set_content_enabled(state: St<'_>, id: String, name: String, enabled: bool) -> AppResult<String> {
    let inst = state.instance(&id).await?;
    let out = blocking(move || mods::set_enabled(&inst, &name, enabled)).await?;
    state.with_runtime(&id, |r| r.pending_restart = r.state == Some(ServerState::Running)).await;
    Ok(out)
}

#[tauri::command]
pub async fn remove_content(state: St<'_>, id: String, name: String) -> AppResult<bool> {
    let inst = state.instance(&id).await?;
    let now = blocking(move || mods::remove_or_defer(&inst, &name)).await?;
    state.with_runtime(&id, |r| r.pending_restart = r.state == Some(ServerState::Running)).await;
    Ok(now)
}

#[tauri::command]
pub async fn identify_content(state: St<'_>, id: String) -> AppResult<u32> {
    let inst = state.instance(&id).await?;
    sources::modrinth_identify(&state, &inst).await
}

#[tauri::command]
pub async fn content_updates(state: St<'_>, id: String) -> AppResult<Vec<sources::UpdateCandidate>> {
    let inst = state.instance(&id).await?;
    sources::modrinth_updates(&state, &inst).await
}

#[tauri::command]
pub async fn search_content(state: St<'_>, id: Option<String>, source: String, query: String, project_type: String, sort: String, offset: u32, mc: Option<String>) -> AppResult<sources::SearchPage> {
    let inst = match &id {
        Some(i) => Some(state.instance(i).await?),
        None => None,
    };
    match (source.as_str(), &inst) {
        ("modrinth", Some(inst)) => {
            let loaders = if project_type == "datapack" { vec![] } else { content_loaders(inst.active_software()) };
            sources::modrinth_search(&state, &query, &project_type, &loaders, inst.active_mc_version(), &sort, offset).await
        }
        ("modrinth", None) => sources::modrinth_search(&state, &query, &project_type, &[], mc.as_deref().unwrap_or(""), &sort, offset).await,
        ("curseforge", Some(inst)) => sources::curseforge_search(&state, inst, &query, &project_type, &sort, offset).await,
        ("hangar", Some(inst)) => sources::hangar_search(&state, inst, &query, &sort, offset).await,
        _ => Err(AppError::msg("Pick a server to search for.")),
    }
}

#[tauri::command]
pub async fn project_versions(state: St<'_>, id: Option<String>, source: String, project: String, project_type: String) -> AppResult<Vec<sources::ProjectVersion>> {
    let inst = match &id {
        Some(i) => Some(state.instance(i).await?),
        None => None,
    };
    match (source.as_str(), &inst) {
        ("modrinth", Some(inst)) => {
            let loaders = if project_type == "datapack" { vec!["datapack"] } else { content_loaders(inst.active_software()) };
            sources::modrinth_versions(&state, &project, &loaders, inst.active_mc_version()).await
        }
        ("modrinth", None) => sources::modrinth_versions(&state, &project, &[], "").await,
        ("curseforge", Some(inst)) => sources::curseforge_versions(&state, inst, &project, &project_type).await,
        ("hangar", Some(inst)) => sources::hangar_versions(&state, inst, &project).await,
        _ => Err(AppError::msg("Pick a server first.")),
    }
}

#[tauri::command]
pub async fn install_content(app: AppHandle, state: St<'_>, id: String, source: String, project: String, version: Option<String>, project_type: String, title: String) -> AppResult<Vec<sources::Installed>> {
    let inst = state.instance(&id).await?;
    let r = TaskReporter::new(&app, "content", &format!("Installing {title}"), Some(&id));
    let res = match source.as_str() {
        "modrinth" => sources::modrinth_install(&state, &inst, &project, version.as_deref(), &project_type, &r).await,
        "curseforge" => sources::curseforge_install(&state, &inst, &project, version.as_deref(), &project_type, &r).await,
        "hangar" => sources::hangar_install(&state, &inst, &project, version.as_deref(), &r).await,
        _ => Err(AppError::msg("Unknown content source")),
    };
    match &res {
        Ok(list) => {
            let n = list.iter().filter(|i| i.skipped.is_none()).count();
            let deps = list.iter().filter(|i| i.dependency && i.skipped.is_none()).count();
            r.done(&format!("{title} installed{}", if deps > 0 { format!(" with {deps} dependenc{}", if deps == 1 { "y" } else { "ies" }) } else if n == 0 { " (nothing to download)".into() } else { String::new() }));
            if server::running_pid(&inst).is_some() {
                state.with_runtime(&id, |r| r.pending_restart = true).await;
                state.emit_status(&app, &id).await;
            }
        }
        Err(e) => r.fail(&e.to_string()),
    }
    res
}

#[tauri::command]
pub async fn update_content(app: AppHandle, state: St<'_>, id: String, items: Vec<sources::UpdateCandidate>) -> AppResult<u32> {
    let inst = state.instance(&id).await?;
    let r = TaskReporter::new(&app, "content", &format!("Updating {} mod{}", items.len(), if items.len() == 1 { "" } else { "s" }), Some(&id));
    let mut n = 0;
    for it in &items {
        r.progress(&format!("Updating {}…", it.title), Some(n as f64 / items.len().max(1) as f64));
        let kind = sources::default_type(&inst);
        match sources::modrinth_install(&state, &inst, &it.project_id, Some(&it.version_id), kind, &r).await {
            Ok(_) => n += 1,
            Err(e) => crate::state::notify(&app, &id, "warn", format!("{}: {e}", it.title)),
        }
    }
    r.done(&format!("Updated {n} of {}", items.len()));
    if server::running_pid(&inst).is_some() {
        state.with_runtime(&id, |r| r.pending_restart = true).await;
    }
    Ok(n)
}

#[tauri::command]
pub async fn export_client_pack(state: St<'_>, id: String, dest: PathBuf, include_unknown: bool) -> AppResult<(u32, u32)> {
    let inst = state.instance(&id).await?;
    sources::export_client_pack(&state, &inst, &dest, include_unknown).await
}

/// Install a modpack version onto an existing server (switches its software to the pack's).
#[tauri::command]
pub async fn install_modpack(app: AppHandle, state: St<'_>, id: String, modpack: ModpackRef, allow_downgrade: Option<bool>) -> AppResult<()> {
    let mut inst = state.instance(&id).await?;
    require_stopped(&inst, "a modpack replaces the server's mods")?;
    // The pack decides the Minecraft version – opening a world in an older one can corrupt it.
    if let Some(cur) = &inst.installed {
        let pack_mc = sources::modpack_game_version(&state, &modpack.version_id).await.unwrap_or_default();
        if !allow_downgrade.unwrap_or(false) && !pack_mc.is_empty() && inst.world_dir().join("level.dat").exists() && versions::is_newer(&state, &cur.mc_version, &pack_mc).await == Some(true) {
            return Err(AppError::msg(format!(
                "This modpack runs Minecraft {pack_mc}, which is older than {} – opening the world in an older version can corrupt it. Confirm the downgrade to continue (a backup is made first), or start the pack on a new server.",
                cur.mc_version
            )));
        }
    }
    inst.modpack = Some(modpack.clone());
    replace_instance(&state, &inst).await?;
    let st = state.inner().clone();
    let (app2, id2) = (app.clone(), id.clone());
    background(&app, &id, "Modpack install", async move { install_modpack_job(&st, &app2, &id2, &modpack.version_id, true).await });
    Ok(())
}

/// Bedrock crossplay: install Geyser + Floodgate and add the Bedrock port (UDP 19132) to the ports
/// the manager forwards and allows through the firewall.
#[tauri::command]
pub async fn setup_bedrock(app: AppHandle, state: St<'_>, id: String) -> AppResult<Vec<sources::Installed>> {
    let inst = state.instance(&id).await?;
    let r = TaskReporter::new(&app, "content", "Bedrock crossplay (Geyser + Floodgate)", Some(&id));
    let res = sources::install_bedrock(&state, &inst, &r).await;
    let list = match res {
        Ok(l) => l,
        Err(e) => {
            r.fail(&e.to_string());
            return Err(e);
        }
    };
    let mut inst = state.instance(&id).await?;
    let bedrock = |e: &ExtraPort| e.port == 19132 || e.label.to_lowercase().contains("bedrock") || e.label.to_lowercase().contains("geyser");
    if !inst.network.extra_ports.iter().any(bedrock) {
        inst.network.extra_ports.push(ExtraPort::default());
        replace_instance(&state, &inst).await?;
        let _ = app.emit("instances-changed", ());
    }
    r.done("Geyser + Floodgate installed – Bedrock players join on UDP port 19132");
    if server::running_pid(&inst).is_some() {
        state.with_runtime(&id, |r| r.pending_restart = true).await;
    }
    state.emit_status(&app, &id).await;
    Ok(list)
}

// ------------------------------------------------------------------------------------- backups

#[tauri::command]
pub async fn create_backup(app: AppHandle, state: St<'_>, id: String, label: String) -> AppResult<()> {
    let st = state.inner().clone();
    supervisor::run_backup(&st, &app, &id, if label.trim().is_empty() { "Manual" } else { label.trim() }, "manual").await
}

#[tauri::command]
pub async fn list_backups(state: St<'_>, id: String) -> AppResult<Vec<BackupInfo>> {
    let inst = state.instance(&id).await?;
    let dir = state.backup_dir(&inst).await;
    blocking(move || Ok(backup::list(&dir))).await
}

#[tauri::command]
pub async fn get_backup_dir(state: St<'_>, id: String) -> AppResult<PathBuf> {
    let inst = state.instance(&id).await?;
    let d = state.backup_dir(&inst).await;
    let _ = std::fs::create_dir_all(&d);
    Ok(d)
}

#[tauri::command]
pub async fn restore_backup(app: AppHandle, state: St<'_>, id: String, path: PathBuf, include_config: bool) -> AppResult<u32> {
    let inst = state.instance(&id).await?;
    require_stopped(&inst, "worlds can only be restored while the server is off")?;
    let dir = state.backup_dir(&inst).await;
    if !proc::is_within(&path, &dir) {
        return Err(AppError::msg("That backup isn't in this server's backup folder."));
    }
    // Safety net: the current state becomes a backup too.
    if let Ok(b) = backup::create(&state, &inst, "Before restore", "auto").await {
        let _ = app.emit("backup-created", &b);
    }
    let n = blocking(move || backup::restore(&inst, &path, include_config)).await?;
    if include_config {
        // server.properties may have changed – re-read it into the profile.
        let mut inst = state.instance(&id).await?;
        server::read_files_into(&mut inst);
        for k in server::network_overlay(&inst).keys() {
            inst.properties.remove(k);
        }
        replace_instance(&state, &inst).await?;
        let _ = app.emit("instances-changed", ());
    }
    Ok(n)
}

#[tauri::command]
pub async fn delete_backup(state: St<'_>, id: String, path: PathBuf) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let dir = state.backup_dir(&inst).await;
    if !proc::is_within(&path, &dir) {
        return Err(AppError::msg("That backup isn't in this server's backup folder."));
    }
    std::fs::remove_file(path)?;
    Ok(())
}

// --------------------------------------------------------------------------------------- packs

#[tauri::command]
pub async fn export_pack(state: St<'_>, id: String, dest: PathBuf, options: backup::PackOptions) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    blocking(move || backup::export_pack(&inst, &dest, &options)).await
}

#[tauri::command]
pub async fn read_pack(path: PathBuf) -> AppResult<PackManifest> {
    blocking(move || backup::read_pack(&path)).await
}

#[tauri::command]
pub async fn import_pack(app: AppHandle, state: St<'_>, path: PathBuf, name: String, install_dir: Option<PathBuf>, install_now: bool) -> AppResult<ServerInstance> {
    let p2 = path.clone();
    let m = blocking(move || backup::read_pack(&p2)).await?;
    let mut inst = m.instance;
    inst.id = uuid::Uuid::new_v4().to_string();
    inst.created_at = now_ms();
    inst.installed = None;
    inst.name = if name.trim().is_empty() { inst.name } else { name.trim().to_string() };
    inst.install_dir = match install_dir {
        Some(d) if !d.as_os_str().is_empty() => abs_dir(d)?,
        _ => unique_dir(&state, &inst.name).await,
    };
    let list = state.instances.read().await.clone();
    let (p, r) = network::suggest_ports(&list, None).await;
    if list.iter().any(|i| i.tcp_ports().contains(&inst.network.port)) {
        inst.network.port = p;
        inst.network.rcon_port = r;
    }
    if inst.network.rcon_password.is_empty() {
        inst.network.rcon_password = server::random_password(24);
    }
    let i2 = inst.clone();
    blocking(move || {
        std::fs::create_dir_all(&i2.install_dir)?;
        backup::unpack_into(&path, &i2)?;
        server::sync_files(&i2)
    })
    .await?;
    let inst = add_instance(&state, inst).await?;
    if install_now {
        let (st, app2, id) = (state.inner().clone(), app.clone(), inst.id.clone());
        background(&app, &inst.id, "Install", async move { server::install_job(&st, &app2, &id).await });
    }
    state.emit_status(&app, &inst.id).await;
    Ok(inst)
}

// ------------------------------------------------------------------------------------- network

#[tauri::command]
pub async fn firewall_state(state: St<'_>, id: String) -> AppResult<network::FirewallState> {
    let inst = state.instance(&id).await?;
    let java = diagnostics::resolve_java(&state, &inst).await;
    Ok(network::firewall_state(&inst, java.as_deref()).await)
}

#[tauri::command]
pub async fn add_firewall_rules(state: St<'_>, id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    let java = diagnostics::resolve_java(&state, &inst).await;
    let out = network::add_firewall_rules(&inst, java.as_deref(), &state.data_dir.join("tmp")).await?;
    if !out.contains("RULES_OK") {
        return Err(AppError::msg(format!("Windows didn't confirm the firewall rules: {}", out.lines().filter(|l| l.contains("ERROR") || l.contains("Exception")).collect::<Vec<_>>().join(" "))));
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_firewall_rules(state: St<'_>, id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    network::remove_firewall_rules(&inst, &state.data_dir.join("tmp")).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetInfo {
    public_ip: Option<String>,
    local: Vec<network::LocalIp>,
    gateway: Option<String>,
}

#[tauri::command]
pub async fn net_info(state: St<'_>) -> AppResult<NetInfo> {
    let (public_ip, gateway) = tokio::join!(network::public_ip(&state.http), network::default_gateway());
    let mut local = blocking(|| Ok(network::local_ips())).await?;
    // The adapter on the router's subnet first – that's the address to forward to.
    if let Some(gw) = &gateway {
        let prefix: String = gw.rsplitn(2, '.').nth(1).unwrap_or("").to_string() + ".";
        local.sort_by_key(|l| !l.ip.starts_with(&prefix));
    }
    Ok(NetInfo { public_ip, local, gateway })
}

#[tauri::command]
pub async fn upnp_probe(state: St<'_>) -> AppResult<crate::core::upnp::UpnpInfo> {
    Ok(crate::core::upnp::probe(&state.http).await)
}

#[tauri::command]
pub async fn upnp_map(app: AppHandle, state: St<'_>, id: String) -> AppResult<Vec<String>> {
    let inst = state.instance(&id).await?;
    let lines = crate::core::upnp::map_server(&inst).await?;
    manager_log(&app, &id, format!("UPnP: {}", lines.join(", ")));
    Ok(lines)
}

#[tauri::command]
pub async fn upnp_unmap(state: St<'_>, id: String) -> AppResult<()> {
    let inst = state.instance(&id).await?;
    crate::core::upnp::unmap_server(&inst).await
}

/// Server List Ping any address (LAN / public / another server).
#[tauri::command]
pub async fn ping_server(host: String, port: u16) -> AppResult<mcproto::PingResult> {
    Ok(mcproto::ping(host.trim(), port).await)
}

/// Is the server reachable from the internet? Asks mcsrvstat.us to ping it from outside.
#[tauri::command]
pub async fn external_check(state: St<'_>, host: String, port: u16) -> AppResult<serde_json::Value> {
    network::external_check(&state.http, host.trim(), port).await
}

// --------------------------------------------------------------------------------- diagnostics

#[tauri::command]
pub async fn run_diagnostics(state: St<'_>, id: String) -> AppResult<Vec<DiagCheck>> {
    let inst = state.instance(&id).await?;
    Ok(diagnostics::run(&state, &inst).await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    total: u64,
    worlds: u64,
    content: u64,
    libraries: u64,
    logs: u64,
    backups: u64,
}

#[tauri::command]
pub async fn disk_usage(state: St<'_>, id: String) -> AppResult<DiskUsage> {
    let inst = state.instance(&id).await?;
    let bk = state.backup_dir(&inst).await;
    blocking(move || {
        let backups = backup::folder_size(&bk);
        let total = backup::folder_size(&inst.install_dir);
        let inside = proc::is_within(&bk, &inst.install_dir);
        let worlds: u64 = backup::world_dirs(&inst).iter().map(|(_, p)| backup::folder_size(p)).sum::<u64>()
            + worlds::list(&inst).iter().filter(|w| !w.active).map(|w| w.size).sum::<u64>();
        Ok(DiskUsage {
            total: if inside { total.saturating_sub(backups) } else { total },
            worlds,
            content: inst.content_dir().map(|d| backup::folder_size(&d)).unwrap_or(0),
            libraries: backup::folder_size(&inst.install_dir.join("libraries")),
            logs: backup::folder_size(&inst.logs_dir()) + backup::folder_size(&inst.install_dir.join("crash-reports")),
            backups,
        })
    })
    .await
}

/// A picked image as a data URL (server icon editor scales it in the UI).
#[tauri::command]
pub async fn read_image(path: PathBuf) -> AppResult<String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Err(AppError::msg("Pick a PNG, JPG, GIF or WebP image.")),
    };
    let meta = std::fs::metadata(&path)?;
    if meta.len() > 15 * 1_048_576 {
        return Err(AppError::msg("That image is over 15 MB."));
    }
    let b = std::fs::read(&path)?;
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(b)))
}
