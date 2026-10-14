//! Zip backups of a server's worlds (+ configs, optionally mods/plugins), restore, retention and
//! portable server packs (.mcserver).

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::{now_ms, AppState};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;

const MANIFEST: &str = "backup.json";

fn opts() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(6))
}

/// Region files, jars and images are already compressed: a light pass is several times faster
/// and the zip ends up nearly as small.
fn opts_for(name: &str) -> SimpleFileOptions {
    let n = name.to_ascii_lowercase();
    if [".mca", ".mcc", ".mcr", ".jar", ".zip", ".png", ".gz", ".dat"].iter().any(|e| n.ends_with(e)) {
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(1))
    } else {
        opts()
    }
}

/// Add every file under `dir` to the zip below `prefix/`. Files are read fully first so a save
/// the server is writing at that moment never ends up half-copied mid-stream.
fn add_dir<W: Write + std::io::Seek>(zip: &mut zip::ZipWriter<W>, dir: &Path, prefix: &str, skip: &[&str]) -> AppResult<u32> {
    let mut n = 0;
    if !dir.exists() {
        return Ok(0);
    }
    for e in WalkDir::new(dir).into_iter().flatten() {
        if !e.file_type().is_file() {
            continue;
        }
        let rel = e.path().strip_prefix(dir).unwrap_or(e.path());
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        if skip.iter().any(|s| rel_s.starts_with(s)) {
            continue;
        }
        let mut buf = vec![];
        if std::fs::File::open(e.path()).and_then(|mut f| f.read_to_end(&mut buf)).is_err() {
            continue; // locked file – skip rather than fail the whole backup
        }
        zip.start_file(format!("{prefix}/{rel_s}"), opts_for(&rel_s))?;
        zip.write_all(&buf)?;
        n += 1;
    }
    Ok(n)
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Manifest {
    label: String,
    kind: String,
    created_at: i64,
    server_name: String,
    files: u32,
    mc_version: String,
    worlds: Vec<String>,
}

/// World folders of a server: the level folder plus Bukkit-style `<level>_nether` / `_the_end`.
pub fn world_dirs(inst: &ServerInstance) -> Vec<(String, PathBuf)> {
    let level = inst.level_name();
    [level.clone(), format!("{level}_nether"), format!("{level}_the_end")]
        .into_iter()
        .map(|n| (n.clone(), inst.install_dir.join(&n)))
        .filter(|(_, p)| p.is_dir())
        .collect()
}

/// Root-level config files worth keeping with a backup.
const ROOT_CONFIGS: &[&str] = &[
    "server.properties",
    "whitelist.json",
    "ops.json",
    "banned-players.json",
    "banned-ips.json",
    "bukkit.yml",
    "spigot.yml",
    "commands.yml",
    "permissions.yml",
    "help.yml",
    "purpur.yml",
    "server-icon.png",
];

pub async fn create(state: &AppState, inst: &ServerInstance, label: &str, kind: &str) -> AppResult<BackupInfo> {
    let dir = state.backup_dir(inst).await;
    let inst = inst.clone();
    let label = label.to_string();
    let kind = kind.to_string();
    tokio::task::spawn_blocking(move || {
        let info = create_blocking(&inst, &dir, &label, &kind)?;
        prune(&dir, inst.automation.backup_keep);
        Ok(info)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

fn create_blocking(inst: &ServerInstance, dir: &Path, label: &str, kind: &str) -> AppResult<BackupInfo> {
    std::fs::create_dir_all(dir)?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let mut path = dir.join(format!("{stamp}_{kind}.zip"));
    let mut n = 2;
    while path.exists() {
        // Two backups in the same second (e.g. "Before restore" right after a manual one).
        path = dir.join(format!("{stamp}_{kind}-{n}.zip"));
        n += 1;
    }
    let tmp = path.with_extension("zip.part");
    let file = std::fs::File::create(&tmp)?;
    let mut zip = zip::ZipWriter::new(file);
    let mut files = 0;
    let mut worlds = vec![];
    for (name, p) in world_dirs(inst) {
        // session.lock is held open by a running server and is recreated on start anyway.
        files += add_dir(&mut zip, &p, &format!("worlds/{name}"), &["session.lock"])?;
        worlds.push(name);
    }
    files += add_root_configs(&mut zip, inst)?;
    files += add_dir(&mut zip, &inst.install_dir.join("config"), "config", &[])?;
    if inst.automation.backup_mods {
        if let Some(cd) = inst.content_dir() {
            let pre = cd.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "mods".into());
            files += add_dir(&mut zip, &cd, &pre, &[])?;
        }
    }
    let created_at = now_ms();
    let m = Manifest {
        label: label.to_string(),
        kind: kind.to_string(),
        created_at,
        server_name: inst.name.clone(),
        files,
        mc_version: inst.active_mc_version().to_string(),
        worlds: worlds.clone(),
    };
    zip.start_file(MANIFEST, opts())?;
    zip.write_all(&serde_json::to_vec_pretty(&m)?)?;
    zip.finish()?;
    std::fs::rename(&tmp, &path)?;
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    Ok(BackupInfo {
        name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        path,
        size,
        created_at,
        label: label.to_string(),
        kind: kind.to_string(),
        files,
        mc_version: m.mc_version,
        worlds,
    })
}

fn add_root_configs<W: Write + std::io::Seek>(zip: &mut zip::ZipWriter<W>, inst: &ServerInstance) -> AppResult<u32> {
    let mut n = 0;
    for f in ROOT_CONFIGS {
        let p = inst.install_dir.join(f);
        if let Ok(buf) = std::fs::read(&p) {
            zip.start_file(format!("root/{f}"), opts())?;
            zip.write_all(&buf)?;
            n += 1;
        }
    }
    Ok(n)
}

fn read_manifest(path: &Path) -> Option<Manifest> {
    let f = std::fs::File::open(path).ok()?;
    let mut z = zip::ZipArchive::new(f).ok()?;
    let mut e = z.by_name(MANIFEST).ok()?;
    let mut s = String::new();
    e.read_to_string(&mut s).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn list(dir: &Path) -> Vec<BackupInfo> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<BackupInfo> = rd
        .flatten()
        .filter(|e| e.path().extension().map(|x| x.eq_ignore_ascii_case("zip")).unwrap_or(false))
        .map(|e| {
            let path = e.path();
            let meta = e.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = meta
                .and_then(|m| m.modified().ok())
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis())
                .unwrap_or(0);
            let m = read_manifest(&path);
            BackupInfo {
                name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                size,
                created_at: m.as_ref().map(|m| m.created_at).unwrap_or(mtime),
                label: m.as_ref().map(|m| m.label.clone()).unwrap_or_else(|| "Imported".into()),
                kind: m.as_ref().map(|m| m.kind.clone()).unwrap_or_else(|| "manual".into()),
                files: m.as_ref().map(|m| m.files).unwrap_or(0),
                mc_version: m.as_ref().map(|m| m.mc_version.clone()).unwrap_or_default(),
                worlds: m.as_ref().map(|m| m.worlds.clone()).unwrap_or_default(),
                path,
            }
        })
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

/// Keep the newest `keep` automatic backups; manual ones are never pruned. Also drops `.zip.part`
/// leftovers of backups that were interrupted (manager closed mid-backup) once they're an hour old.
pub fn prune(dir: &Path, keep: u32) -> u32 {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let stale = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).map(|a| a.as_secs() > 3600).unwrap_or(false);
            if stale && e.file_name().to_string_lossy().ends_with(".zip.part") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    if keep == 0 {
        return 0;
    }
    let autos: Vec<BackupInfo> = list(dir).into_iter().filter(|b| b.kind != "manual").collect();
    let mut removed = 0;
    for b in autos.iter().skip(keep as usize) {
        if std::fs::remove_file(&b.path).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn extract_prefix(zip_path: &Path, prefix: &str, dest: &Path) -> AppResult<u32> {
    let f = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f)?;
    let mut n = 0;
    for i in 0..z.len() {
        let mut e = z.by_index(i)?;
        let Some(name) = e.enclosed_name() else { continue };
        let name_s = name.to_string_lossy().replace('\\', "/");
        let Some(rel) = name_s.strip_prefix(&format!("{prefix}/")) else { continue };
        if rel.is_empty() || e.is_dir() {
            continue;
        }
        let out = dest.join(rel);
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut w = std::fs::File::create(&out)?;
        std::io::copy(&mut e, &mut w)?;
        n += 1;
    }
    Ok(n)
}

fn zip_has_prefix(zip_path: &Path, prefix: &str) -> bool {
    let Ok(f) = std::fs::File::open(zip_path) else { return false };
    let Ok(z) = zip::ZipArchive::new(f) else { return false };
    let p = format!("{prefix}/");
    let found = z.file_names().any(|n| n.map(|n| n.starts_with(&p)).unwrap_or(false));
    found
}

/// Top-level world folder names stored in a backup.
fn zip_worlds(zip_path: &Path) -> Vec<String> {
    let Ok(f) = std::fs::File::open(zip_path) else { return vec![] };
    let Ok(z) = zip::ZipArchive::new(f) else { return vec![] };
    let mut out: Vec<String> = z
        .file_names()
        .filter_map(|n| n.ok().and_then(|n| n.strip_prefix("worlds/").and_then(|r| r.split('/').next()).map(String::from)))
        .filter(|n| !n.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Restore a backup into a stopped server. Current worlds are quarantined first, never deleted.
pub fn restore(inst: &ServerInstance, zip_path: &Path, include_config: bool) -> AppResult<u32> {
    let mut n = 0;
    for w in zip_worlds(zip_path) {
        let dest = inst.install_dir.join(&w);
        quarantine_to(&inst.install_dir, &dest, &format!("before-restore-{w}"))?;
        std::fs::create_dir_all(&dest)?;
        n += extract_prefix(zip_path, &format!("worlds/{w}"), &dest)?;
    }
    if include_config {
        n += extract_prefix(zip_path, "root", &inst.install_dir)?;
        if zip_has_prefix(zip_path, "config") {
            n += extract_prefix(zip_path, "config", &inst.install_dir.join("config"))?;
        }
    }
    Ok(n)
}

/// Move a folder inside a server into `<server>/.mcsm/quarantine/<stamp>-<label>` (never delete).
pub fn quarantine_to(server_dir: &Path, dir: &Path, label: &str) -> AppResult<Option<PathBuf>> {
    if !dir.exists() {
        return Ok(None);
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = server_dir.join(META_DIR).join("quarantine").join(format!("{stamp}-{label}"));
    std::fs::create_dir_all(dest.parent().unwrap())?;
    std::fs::rename(dir, &dest)?;
    Ok(Some(dest))
}

/// Move every file of a folder (not the folder itself) into quarantine, one by one. Renaming a
/// whole folder fails on Windows while any file in it is open – antivirus scanning freshly
/// downloaded jars is enough – so single files are retried and copied as a last resort.
pub fn quarantine_contents(server_dir: &Path, dir: &Path, label: &str) -> AppResult<(PathBuf, u32)> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = server_dir.join(META_DIR).join("quarantine").join(format!("{stamp}-{label}"));
    std::fs::create_dir_all(&dest)?;
    let mut moved = 0;
    let mut failed = vec![];
    for e in std::fs::read_dir(dir)?.flatten() {
        let (from, to) = (e.path(), dest.join(e.file_name()));
        let mut ok = false;
        for attempt in 0..5 {
            if std::fs::rename(&from, &to).is_ok() {
                ok = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150 * (attempt + 1)));
        }
        if !ok && from.is_file() && std::fs::copy(&from, &to).is_ok() {
            ok = std::fs::remove_file(&from).is_ok();
        }
        if ok {
            moved += 1;
        } else {
            failed.push(e.file_name().to_string_lossy().into_owned());
        }
    }
    if !failed.is_empty() {
        return Err(AppError::msg(format!("Couldn't move {} file(s) out of {}: {}", failed.len(), dir.display(), failed.join(", "))));
    }
    Ok((dest, moved))
}

// ---------------------------------------------------------------------------------------------
// Server packs (.mcserver): a zip with manifest.json + root/ + config/ + optional worlds/ and mods/.

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackOptions {
    pub include_worlds: bool,
    pub include_secrets: bool,
    #[serde(default)]
    pub include_mods: bool,
}

pub fn export_pack(inst: &ServerInstance, dest: &Path, o: &PackOptions) -> AppResult<()> {
    let mut profile = inst.clone();
    if !o.include_secrets {
        profile.network.rcon_password.clear();
        profile.properties.remove("rcon.password");
    }
    let tmp = dest.with_extension("part");
    let file = std::fs::File::create(&tmp)?;
    let mut zip = zip::ZipWriter::new(file);
    let mut has_configs = add_root_configs(&mut zip, inst)? > 0;
    has_configs |= add_dir(&mut zip, &inst.install_dir.join("config"), "config", &[])? > 0;
    let mut has_worlds = false;
    if o.include_worlds {
        for (name, p) in world_dirs(inst) {
            has_worlds |= add_dir(&mut zip, &p, &format!("worlds/{name}"), &["session.lock"])? > 0;
        }
    }
    let mut has_mods = false;
    if o.include_mods {
        for sub in ["mods", "plugins"] {
            has_mods |= add_dir(&mut zip, &inst.install_dir.join(sub), sub, &[])? > 0;
        }
    }
    let m = PackManifest {
        format: 1,
        app_version: env!("CARGO_PKG_VERSION").into(),
        exported_at: now_ms(),
        instance: profile,
        has_worlds,
        has_configs,
        has_mods,
        includes_secrets: o.include_secrets,
    };
    zip.start_file("manifest.json", opts())?;
    zip.write_all(&serde_json::to_vec_pretty(&m)?)?;
    zip.finish()?;
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

pub fn read_pack(path: &Path) -> AppResult<PackManifest> {
    let f = std::fs::File::open(path)?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| AppError::msg("That file isn't a Minecraft Server Manager pack."))?;
    let mut e = z.by_name("manifest.json").map_err(|_| AppError::msg("The pack has no manifest.json – not a server pack."))?;
    let mut s = String::new();
    e.read_to_string(&mut s)?;
    Ok(serde_json::from_str(&s)?)
}

/// Unpack configs, worlds and mods of a pack into a (new) server's folder.
pub fn unpack_into(path: &Path, inst: &ServerInstance) -> AppResult<()> {
    std::fs::create_dir_all(&inst.install_dir)?;
    extract_prefix(path, "root", &inst.install_dir)?;
    for sub in ["config", "mods", "plugins"] {
        if zip_has_prefix(path, sub) {
            extract_prefix(path, sub, &inst.install_dir.join(sub))?;
        }
    }
    for w in zip_worlds(path) {
        extract_prefix(path, &format!("worlds/{w}"), &inst.install_dir.join(&w))?;
    }
    Ok(())
}

pub fn folder_size(dir: &Path) -> u64 {
    WalkDir::new(dir).into_iter().flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarantines_folder_contents() {
        let root = std::env::temp_dir().join(format!("mcsm-q-{}", uuid::Uuid::new_v4()));
        let mods = root.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("a.jar"), b"a").unwrap();
        std::fs::write(mods.join("b.jar"), b"b").unwrap();
        // A file held open (like a jar being scanned) must not stop the move.
        let _held = std::fs::File::open(mods.join("a.jar")).unwrap();
        let (dest, n) = quarantine_contents(&root, &mods, "test").unwrap();
        assert_eq!(n, 2);
        assert!(mods.exists() && std::fs::read_dir(&mods).unwrap().next().is_none(), "folder stays, empty");
        assert_eq!(std::fs::read(dest.join("b.jar")).unwrap(), b"b");
        drop(_held);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn backup_restore_round_trip() {
        let root = std::env::temp_dir().join(format!("mcsm-bk-{}", uuid::Uuid::new_v4()));
        let inst = ServerInstance { install_dir: root.join("srv"), ..Default::default() };
        std::fs::create_dir_all(inst.world_dir().join("region")).unwrap();
        std::fs::create_dir_all(inst.install_dir.join("world_nether")).unwrap();
        std::fs::write(inst.world_dir().join("level.dat"), b"v1").unwrap();
        std::fs::write(inst.world_dir().join("session.lock"), b"lock").unwrap();
        std::fs::write(inst.install_dir.join("world_nether").join("level.dat"), b"n1").unwrap();
        std::fs::write(inst.properties_path(), b"motd=x\n").unwrap();
        let b = create_blocking(&inst, &root.join("bk"), "test", "manual").unwrap();
        assert_eq!(b.files, 3);
        assert_eq!(b.worlds, vec!["world", "world_nether"]);
        std::fs::write(inst.world_dir().join("level.dat"), b"v2").unwrap();
        restore(&inst, &b.path, false).unwrap();
        assert_eq!(std::fs::read(inst.world_dir().join("level.dat")).unwrap(), b"v1");
        assert!(!inst.world_dir().join("session.lock").exists());
        let listed = list(&root.join("bk"));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "test");
        assert!(inst.meta_dir().join("quarantine").exists(), "old world kept in quarantine");
        let _ = std::fs::remove_dir_all(root);
    }
}
