//! Worlds: discovery (level.dat), active world, import/export, reset (whole world or one
//! dimension), rename and datapacks. Nothing is ever deleted – old data goes to quarantine.

use super::backup::quarantine_to;
use super::nbt;
use crate::error::{AppError, AppResult};
use crate::models::*;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorldInfo {
    pub name: String,
    pub path: PathBuf,
    /// The server loads this one (level-name).
    pub active: bool,
    pub size: u64,
    pub modified: i64,
    pub level_name: Option<String>,
    pub version: Option<String>,
    pub data_version: Option<i64>,
    pub seed: Option<String>,
    pub game_type: Option<i64>,
    pub difficulty: Option<i64>,
    pub hardcore: bool,
    pub last_played: Option<i64>,
    pub day_time: Option<i64>,
    /// Extra dimension folders that belong to this world (Paper/Spigot `_nether`, `_the_end`).
    pub dimensions: Vec<String>,
    pub has_nether: bool,
    pub has_end: bool,
    pub datapacks: u32,
}

fn dir_size(p: &Path) -> u64 {
    WalkDir::new(p).into_iter().flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}

fn mtime(p: &Path) -> i64 {
    std::fs::metadata(p).and_then(|m| m.modified()).map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()).unwrap_or(0)
}

pub fn read_level(path: &Path, info: &mut WorldInfo) {
    let Ok(bytes) = std::fs::read(path.join("level.dat")) else { return };
    let Ok(root) = nbt::parse(&bytes) else { return };
    let Some(d) = root.get("Data") else { return };
    info.level_name = d.get("LevelName").and_then(|t| t.as_str()).map(String::from);
    info.version = d.path(&["Version", "Name"]).and_then(|t| t.as_str()).map(String::from);
    info.data_version = d.get("DataVersion").and_then(|t| t.as_i64());
    info.seed = d
        .path(&["WorldGenSettings", "seed"])
        .or_else(|| d.get("RandomSeed"))
        .or_else(|| d.path(&["world_gen_settings", "seed"]))
        .and_then(|t| t.as_i64())
        .map(|s| s.to_string());
    info.game_type = d.get("GameType").and_then(|t| t.as_i64());
    // 26.x stores difficulty as a name inside difficulty_settings.
    info.difficulty = d.get("Difficulty").and_then(|t| t.as_i64()).or_else(|| {
        let v = d.path(&["difficulty_settings", "difficulty"])?;
        v.as_i64().or_else(|| ["peaceful", "easy", "normal", "hard"].iter().position(|x| Some(*x) == v.as_str()).map(|i| i as i64))
    });
    info.hardcore = d.get("hardcore").and_then(|t| t.as_i64()).unwrap_or(0) != 0 || d.path(&["difficulty_settings", "hardcore"]).and_then(|t| t.as_i64()).unwrap_or(0) != 0;
    info.last_played = d.get("LastPlayed").and_then(|t| t.as_i64());
    info.day_time = d.get("DayTime").and_then(|t| t.as_i64()).or_else(|| d.get("Time").and_then(|t| t.as_i64()));
    // 26.x moved world generation settings out of level.dat.
    if info.seed.is_none() {
        let wgs = path.join("dimensions").join("minecraft").join("overworld").join("data").join("minecraft").join("world_gen_settings.dat");
        if let Some(seed) = std::fs::read(wgs).ok().and_then(|b| nbt::parse(&b).ok()).and_then(|t| t.path(&["data", "seed"]).and_then(|x| x.as_i64())) {
            info.seed = Some(seed.to_string());
        }
    }
}

/// Every world folder in the server root (a folder with level.dat).
pub fn list(inst: &ServerInstance) -> Vec<WorldInfo> {
    let root = &inst.install_dir;
    let active = inst.level_name();
    let Ok(rd) = std::fs::read_dir(root) else { return vec![] };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_dir() && e.path().join("level.dat").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let is_dim_of = |n: &str| -> Option<String> {
        for suf in ["_nether", "_the_end"] {
            if let Some(base) = n.strip_suffix(suf) {
                if names.iter().any(|x| x == base) {
                    return Some(base.to_string());
                }
            }
        }
        None
    };
    let mut out = vec![];
    for n in names.iter().filter(|n| is_dim_of(n).is_none()) {
        let path = root.join(n);
        let mut w = WorldInfo { name: n.clone(), active: *n == active, size: dir_size(&path), modified: mtime(&path.join("level.dat")), path: path.clone(), ..Default::default() };
        read_level(&path, &mut w);
        for suf in ["_nether", "_the_end"] {
            let d = root.join(format!("{n}{suf}"));
            if d.is_dir() {
                w.size += dir_size(&d);
                w.dimensions.push(format!("{n}{suf}"));
            }
        }
        let dims = path.join("dimensions").join("minecraft");
        w.has_nether = path.join("DIM-1").exists() || root.join(format!("{n}_nether")).exists() || dims.join("the_nether").join("region").exists();
        w.has_end = path.join("DIM1").exists() || root.join(format!("{n}_the_end")).exists() || dims.join("the_end").join("region").exists();
        w.datapacks = std::fs::read_dir(path.join("datapacks")).map(|r| r.count() as u32).unwrap_or(0);
        out.push(w);
    }
    // The configured level that hasn't been generated yet.
    if !out.iter().any(|w| w.active) {
        out.insert(0, WorldInfo { name: active.clone(), path: root.join(&active), active: true, ..Default::default() });
    }
    out
}

fn valid_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.len() > 64 || n.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || n.starts_with('.') || n == ".." {
        return Err(AppError::msg("Use a simple folder name for the world (letters, numbers, spaces, - and _)."));
    }
    let reserved = ["config", "mods", "plugins", "libraries", "logs", "crash-reports", "versions", "cache", ".mcsm", "backups"];
    if reserved.contains(&n.to_lowercase().as_str()) {
        return Err(AppError::msg(format!("“{n}” is used by the server itself – pick another world name.")));
    }
    Ok(n.to_string())
}

/// Folders that make up a world (itself + Bukkit dimension folders).
fn world_folders(inst: &ServerInstance, name: &str) -> Vec<PathBuf> {
    let mut v = vec![inst.install_dir.join(name)];
    for suf in ["_nether", "_the_end"] {
        let d = inst.install_dir.join(format!("{name}{suf}"));
        if d.is_dir() {
            v.push(d);
        }
    }
    v
}

/// Move a world (with its dimension folders) to quarantine. Returns where it went.
pub fn delete(inst: &ServerInstance, name: &str) -> AppResult<Option<PathBuf>> {
    let mut first = None;
    for f in world_folders(inst, name) {
        let label = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let r = quarantine_to(&inst.install_dir, &f, &format!("deleted-{label}"))?;
        first = first.or(r);
    }
    Ok(first)
}

/// Reset a world: `all` regenerates everything; `nether` / `end` only that dimension.
pub fn reset(inst: &ServerInstance, name: &str, what: &str) -> AppResult<u32> {
    let base = inst.install_dir.join(name);
    let targets: Vec<PathBuf> = match what {
        "nether" => vec![base.join("DIM-1"), inst.install_dir.join(format!("{name}_nether")), base.join("dimensions").join("minecraft").join("the_nether")],
        "end" => vec![base.join("DIM1"), inst.install_dir.join(format!("{name}_the_end")), base.join("dimensions").join("minecraft").join("the_end")],
        _ => world_folders(inst, name),
    };
    let mut n = 0;
    for t in targets.into_iter().filter(|t| t.exists()) {
        let label = t.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        quarantine_to(&inst.install_dir, &t, &format!("reset-{name}-{label}"))?;
        n += 1;
    }
    Ok(n)
}

pub fn rename(inst: &ServerInstance, name: &str, new_name: &str) -> AppResult<()> {
    let new_name = valid_name(new_name)?;
    if inst.install_dir.join(&new_name).exists() {
        return Err(AppError::msg(format!("A folder called “{new_name}” already exists.")));
    }
    for f in world_folders(inst, name) {
        let fname = f.file_name().unwrap().to_string_lossy().into_owned();
        let suffix = fname.strip_prefix(name).unwrap_or("");
        std::fs::rename(&f, inst.install_dir.join(format!("{new_name}{suffix}")))?;
    }
    Ok(())
}

/// Find the folder holding level.dat inside an unpacked archive / folder.
fn find_level_root(dir: &Path) -> Option<PathBuf> {
    WalkDir::new(dir).max_depth(4).into_iter().flatten().filter(|e| e.file_name() == "level.dat").map(|e| e.path().parent().unwrap().to_path_buf()).min_by_key(|p| p.components().count())
}

fn copy_dir(src: &Path, dst: &Path) -> AppResult<u32> {
    let mut n = 0;
    for e in WalkDir::new(src).into_iter().flatten() {
        let rel = e.path().strip_prefix(src).unwrap();
        let out = dst.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&out)?;
        } else if e.file_type().is_file() {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(e.path(), &out)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Import a world from a .zip or a folder (singleplayer saves work too). Returns its name.
pub fn import(inst: &ServerInstance, src: &Path, name: Option<&str>) -> AppResult<String> {
    let default_name = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "imported".into());
    let mut name = valid_name(name.filter(|n| !n.trim().is_empty()).unwrap_or(&default_name))?;
    let mut i = 2;
    while inst.install_dir.join(&name).exists() {
        name = format!("{}-{i}", name.trim_end_matches(char::is_numeric).trim_end_matches('-'));
        i += 1;
    }
    let dest = inst.install_dir.join(&name);
    if src.is_dir() {
        let root = find_level_root(src).ok_or_else(|| AppError::msg("That folder has no level.dat – it isn't a Minecraft world."))?;
        copy_dir(&root, &dest)?;
    } else {
        let tmp = inst.meta_dir().join(format!("import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp)?;
        let res = (|| -> AppResult<()> {
            let f = std::fs::File::open(src)?;
            let mut z = zip::ZipArchive::new(f).map_err(|_| AppError::msg("That file isn't a zip archive."))?;
            z.extract(&tmp)?;
            let root = find_level_root(&tmp).ok_or_else(|| AppError::msg("The archive has no level.dat – it isn't a Minecraft world."))?;
            std::fs::rename(&root, &dest).or_else(|_| copy_dir(&root, &dest).map(|_| ()))?;
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&tmp);
        res?;
    }
    // A world copied from a running game keeps its lock – the server makes a new one.
    let _ = std::fs::remove_file(dest.join("session.lock"));
    Ok(name)
}

/// Zip a world (and its dimension folders) so it can be opened in singleplayer or shared.
pub fn export(inst: &ServerInstance, name: &str, dest: &Path) -> AppResult<u32> {
    let tmp = dest.with_extension("part");
    let f = std::fs::File::create(&tmp)?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut n = 0;
    for folder in world_folders(inst, name) {
        let fname = folder.file_name().unwrap().to_string_lossy().into_owned();
        for e in WalkDir::new(&folder).into_iter().flatten().filter(|e| e.file_type().is_file()) {
            if e.file_name() == "session.lock" {
                continue;
            }
            let rel = e.path().strip_prefix(&folder).unwrap().to_string_lossy().replace('\\', "/");
            let Ok(buf) = std::fs::read(e.path()) else { continue };
            z.start_file(format!("{fname}/{rel}"), opts)?;
            z.write_all(&buf)?;
            n += 1;
        }
    }
    z.finish()?;
    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(n)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Datapack {
    pub name: String,
    pub size: u64,
    pub is_zip: bool,
    pub description: Option<String>,
}

pub fn datapacks(inst: &ServerInstance, world: &str) -> Vec<Datapack> {
    let dir = inst.install_dir.join(world).join("datapacks");
    let Ok(rd) = std::fs::read_dir(&dir) else { return vec![] };
    let mut out: Vec<Datapack> = rd
        .flatten()
        .map(|e| {
            let p = e.path();
            let is_zip = p.is_file();
            let description = if is_zip {
                std::fs::File::open(&p).ok().and_then(|f| zip::ZipArchive::new(f).ok()).and_then(|mut z| {
                    let mut s = String::new();
                    std::io::Read::read_to_string(&mut z.by_name("pack.mcmeta").ok()?, &mut s).ok()?;
                    pack_description(&s)
                })
            } else {
                std::fs::read_to_string(p.join("pack.mcmeta")).ok().and_then(|s| pack_description(&s))
            };
            Datapack { name: e.file_name().to_string_lossy().into_owned(), size: if is_zip { e.metadata().map(|m| m.len()).unwrap_or(0) } else { dir_size(&p) }, is_zip, description }
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn pack_description(mcmeta: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(mcmeta).ok()?;
    let d = &v["pack"]["description"];
    Some(match d {
        serde_json::Value::String(s) => super::mcproto::strip_codes(s),
        other => super::mcproto::strip_codes(&other.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_string()),
    })
    .filter(|s| !s.is_empty())
}

pub fn add_datapacks(inst: &ServerInstance, world: &str, paths: &[PathBuf]) -> AppResult<Vec<String>> {
    let dir = inst.install_dir.join(world).join("datapacks");
    std::fs::create_dir_all(&dir)?;
    let mut out = vec![];
    for p in paths {
        let name = p.file_name().ok_or_else(|| AppError::msg("bad path"))?.to_string_lossy().into_owned();
        if p.is_dir() {
            copy_dir(p, &dir.join(&name))?;
        } else {
            std::fs::copy(p, dir.join(&name))?;
        }
        out.push(name);
    }
    Ok(out)
}

pub fn remove_datapack(inst: &ServerInstance, world: &str, name: &str) -> AppResult<()> {
    let p = inst.install_dir.join(world).join("datapacks").join(name);
    if !super::proc::is_within(&p, &inst.install_dir.join(world).join("datapacks")) || name.contains("..") {
        return Err(AppError::msg("bad datapack name"));
    }
    quarantine_to(&inst.install_dir, &p, &format!("datapack-{name}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lists_and_resets_worlds() {
        let root = std::env::temp_dir().join(format!("mcsm-w-{}", uuid::Uuid::new_v4()));
        let inst = ServerInstance { install_dir: root.clone(), ..Default::default() };
        for d in ["world", "world_nether", "world_the_end", "creative"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
            std::fs::write(root.join(d).join("level.dat"), b"junk").unwrap();
        }
        let l = list(&inst);
        assert_eq!(l.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), vec!["creative", "world"]);
        let w = l.iter().find(|w| w.name == "world").unwrap();
        assert!(w.active && w.has_nether && w.has_end);
        assert_eq!(w.dimensions.len(), 2);
        assert_eq!(reset(&inst, "world", "nether").unwrap(), 1);
        assert!(!root.join("world_nether").exists() && root.join("world").exists());
        rename(&inst, "creative", "build").unwrap();
        assert!(root.join("build").exists());
        assert!(valid_name("../x").is_err() && valid_name("mods").is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
