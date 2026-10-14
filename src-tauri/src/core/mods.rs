//! Installed mods / plugins: jar metadata (Fabric, Quilt, Forge, NeoForge, Bukkit/Paper),
//! enable/disable (`.disabled` suffix, the convention every launcher uses), removal to quarantine
//! and the source index (`.mcsm/content.json`) that remembers where each file came from.

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::{now_ms, read_json, write_json_atomic};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where a file came from (keyed by SHA-1, so renames and `.disabled` don't lose it).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SourceInfo {
    /// modrinth | curseforge | hangar
    pub source: String,
    pub project_id: String,
    pub version_id: String,
    pub slug: String,
    pub title: String,
    pub version_number: String,
    pub icon_url: String,
    pub client_side: String,
    pub server_side: String,
    pub installed_at: i64,
    /// Pulled in as a dependency of another project.
    pub dependency: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct JarMeta {
    pub mod_id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub icon: Option<String>,
    /// client | server | * (Fabric/Quilt declare it)
    pub environment: Option<String>,
    /// fabric | quilt | forge | neoforge | bukkit | paper
    pub loader: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentItem {
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    pub modified: i64,
    pub sha1: String,
    pub meta: JarMeta,
    pub source: Option<SourceInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentState {
    /// mods | plugins | none
    pub kind: String,
    pub dir: Option<PathBuf>,
    pub items: Vec<ContentItem>,
}

pub fn index_path(inst: &ServerInstance) -> PathBuf {
    inst.meta_dir().join("content.json")
}

pub fn load_index(inst: &ServerInstance) -> HashMap<String, SourceInfo> {
    read_json(&index_path(inst)).unwrap_or_default()
}

pub fn save_index(inst: &ServerInstance, idx: &HashMap<String, SourceInfo>) -> AppResult<()> {
    write_json_atomic(&index_path(inst), idx)
}

fn zip_text(z: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut e = z.by_name(name).ok()?;
    if e.size() > 2_000_000 {
        return None;
    }
    let mut s = String::new();
    e.read_to_string(&mut s).ok()?;
    Some(s)
}

fn zip_icon(z: &mut zip::ZipArchive<std::fs::File>, path: &str) -> Option<String> {
    let path = path.trim_start_matches('/');
    let mut e = z.by_name(path).ok()?;
    if e.size() > 192 * 1024 {
        return None;
    }
    let mut b = vec![];
    e.read_to_end(&mut b).ok()?;
    let mime = if path.ends_with(".jpg") || path.ends_with(".jpeg") { "image/jpeg" } else { "image/png" };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(b)))
}

fn toml_str(text: &str, key: &str) -> Option<String> {
    // Multi-line strings first, or `'''` would match as an empty '' literal.
    let re = regex::Regex::new(&format!(r#"(?m)^\s*{key}\s*=\s*(?:'''([\s\S]*?)'''|"""([\s\S]*?)"""|"([^"]*)"|'([^']*)')"#)).ok()?;
    let c = re.captures(text)?;
    (1..=4).find_map(|i| c.get(i)).map(|m| m.as_str().trim().to_string())
}

fn yml_str(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let l2 = l.trim_end();
        if l2.starts_with(' ') || l2.starts_with('\t') {
            return None;
        }
        l2.strip_prefix(&format!("{key}:")).map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
    }).filter(|v| !v.is_empty())
}

/// Read metadata from a mod/plugin jar.
pub fn read_jar_meta(path: &Path) -> JarMeta {
    let mut m = JarMeta::default();
    let Ok(f) = std::fs::File::open(path) else { return m };
    let Ok(mut z) = zip::ZipArchive::new(f) else { return m };
    if let Some(t) = zip_text(&mut z, "fabric.mod.json") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
            m.loader = Some("fabric".into());
            m.mod_id = v["id"].as_str().map(String::from);
            m.name = v["name"].as_str().map(String::from);
            m.version = v["version"].as_str().map(String::from);
            m.description = v["description"].as_str().map(String::from);
            m.environment = v["environment"].as_str().map(String::from);
            m.authors = v["authors"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from).or_else(|| x["name"].as_str().map(String::from))).collect()).unwrap_or_default();
            let icon = match &v["icon"] {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Object(o) => o.values().last().and_then(|x| x.as_str()).map(String::from),
                _ => None,
            };
            m.icon = icon.and_then(|i| zip_icon(&mut z, &i));
            return m;
        }
    }
    if let Some(t) = zip_text(&mut z, "quilt.mod.json") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
            let q = &v["quilt_loader"];
            m.loader = Some("quilt".into());
            m.mod_id = q["id"].as_str().map(String::from);
            m.version = q["version"].as_str().map(String::from);
            m.name = q["metadata"]["name"].as_str().map(String::from);
            m.description = q["metadata"]["description"].as_str().map(String::from);
            m.environment = v["minecraft"]["environment"].as_str().map(String::from);
            m.icon = q["metadata"]["icon"].as_str().and_then(|i| zip_icon(&mut z, i));
            return m;
        }
    }
    for (file, loader) in [("META-INF/neoforge.mods.toml", "neoforge"), ("META-INF/mods.toml", "forge")] {
        if let Some(t) = zip_text(&mut z, file) {
            m.loader = Some(loader.into());
            // The first [[mods]] block describes the jar's main mod.
            let block = t.split("[[mods]]").nth(1).unwrap_or(&t).split("[[").next().unwrap_or(&t).to_string();
            m.mod_id = toml_str(&block, "modId");
            m.name = toml_str(&block, "displayName");
            m.version = toml_str(&block, "version");
            m.description = toml_str(&block, "description");
            m.authors = toml_str(&block, "authors").or_else(|| toml_str(&t, "authors")).map(|a| vec![a]).unwrap_or_default();
            if m.version.as_deref().map(|v| v.contains("${")).unwrap_or(false) {
                m.version = zip_text(&mut z, "META-INF/MANIFEST.MF").and_then(|mf| mf.lines().find_map(|l| l.strip_prefix("Implementation-Version:").map(|v| v.trim().to_string())));
            }
            if let Some(logo) = toml_str(&block, "logoFile").or_else(|| toml_str(&t, "logoFile")) {
                m.icon = zip_icon(&mut z, &logo);
            }
            return m;
        }
    }
    if let Some(t) = zip_text(&mut z, "mcmod.info") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
            let first = v.as_array().and_then(|a| a.first().cloned()).or_else(|| v["modList"].as_array().and_then(|a| a.first().cloned())).unwrap_or_default();
            m.loader = Some("forge".into());
            m.mod_id = first["modid"].as_str().map(String::from);
            m.name = first["name"].as_str().map(String::from);
            m.version = first["version"].as_str().map(String::from);
            m.description = first["description"].as_str().map(String::from);
            return m;
        }
    }
    for (file, loader) in [("paper-plugin.yml", "paper"), ("plugin.yml", "bukkit")] {
        if let Some(t) = zip_text(&mut z, file) {
            m.loader = Some(loader.into());
            m.name = yml_str(&t, "name");
            m.mod_id = m.name.clone();
            m.version = yml_str(&t, "version");
            m.description = yml_str(&t, "description");
            m.authors = yml_str(&t, "author").map(|a| vec![a]).unwrap_or_default();
            return m;
        }
    }
    m
}

#[derive(Serialize, Deserialize, Default)]
struct MetaCache {
    entries: HashMap<String, (String, JarMeta)>,
}

/// Every jar in mods/ or plugins/ with metadata (cached by name+size+mtime) and source info.
pub fn list(inst: &ServerInstance) -> ContentState {
    let kind = content_kind(inst.active_software()).to_string();
    let Some(dir) = inst.content_dir() else { return ContentState { kind, dir: None, items: vec![] } };
    let cache_path = inst.meta_dir().join("jar-meta.json");
    let mut cache: MetaCache = read_json(&cache_path).unwrap_or_default();
    let idx = load_index(inst);
    let mut items = vec![];
    let mut dirty = false;
    let mut seen = std::collections::HashSet::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_lowercase();
            let enabled = lower.ends_with(".jar");
            if !p.is_file() || !(enabled || lower.ends_with(".jar.disabled")) {
                continue;
            }
            let md = e.metadata().ok();
            let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = md.and_then(|m| m.modified().ok()).map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()).unwrap_or(0);
            let key = format!("{name}|{size}|{modified}");
            seen.insert(key.clone());
            let (sha1, meta) = match cache.entries.get(&key) {
                Some(v) => v.clone(),
                None => {
                    let v = (super::download::sha1_file(&p).unwrap_or_default(), read_jar_meta(&p));
                    cache.entries.insert(key, v.clone());
                    dirty = true;
                    v
                }
            };
            let source = idx.get(&sha1).cloned();
            items.push(ContentItem { file_name: name, enabled, size, modified, sha1, meta, source });
        }
    }
    if dirty || cache.entries.len() != seen.len() {
        cache.entries.retain(|k, _| seen.contains(k));
        let _ = write_json_atomic(&cache_path, &cache);
    }
    items.sort_by_key(|i| display_name(i).to_lowercase());
    ContentState { kind, dir: Some(dir), items }
}

pub fn display_name(i: &ContentItem) -> String {
    i.source.as_ref().map(|s| s.title.clone()).filter(|t| !t.is_empty()).or_else(|| i.meta.name.clone()).unwrap_or_else(|| i.file_name.trim_end_matches(".disabled").trim_end_matches(".jar").to_string())
}

fn checked(inst: &ServerInstance, name: &str) -> AppResult<PathBuf> {
    let dir = inst.content_dir().ok_or_else(|| AppError::msg("This server software doesn't load mods or plugins."))?;
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err(AppError::msg("bad file name"));
    }
    Ok(dir.join(name))
}

pub fn set_enabled(inst: &ServerInstance, name: &str, enabled: bool) -> AppResult<String> {
    let p = checked(inst, name)?;
    if !p.exists() {
        return Err(AppError::msg(format!("{name} no longer exists.")));
    }
    let base = name.trim_end_matches(".disabled").to_string();
    let new_name = if enabled { base } else { format!("{base}.disabled") };
    if new_name != name {
        std::fs::rename(&p, checked(inst, &new_name)?).map_err(|e| {
            if e.raw_os_error() == Some(32) {
                AppError::msg("The running server has this file open – stop the server to enable or disable it.")
            } else {
                e.into()
            }
        })?;
    }
    Ok(new_name)
}

/// Move a mod/plugin to `.mcsm/removed/` (recoverable).
pub fn remove(inst: &ServerInstance, name: &str) -> AppResult<()> {
    let p = checked(inst, name)?;
    let dest = inst.meta_dir().join("removed").join(chrono::Local::now().format("%Y%m%d-%H%M%S").to_string());
    std::fs::create_dir_all(&dest)?;
    std::fs::rename(&p, dest.join(name))?;
    Ok(())
}

fn pending_path(inst: &ServerInstance) -> PathBuf {
    inst.meta_dir().join("pending-removals.json")
}

/// Remove now, or – when the running server holds the file open – right before the next start.
/// Returns false when the removal was deferred.
pub fn remove_or_defer(inst: &ServerInstance, name: &str) -> AppResult<bool> {
    match remove(inst, name) {
        Ok(()) => Ok(true),
        Err(_) if checked(inst, name)?.exists() => {
            let mut list: Vec<String> = read_json(&pending_path(inst)).unwrap_or_default();
            if !list.iter().any(|n| n == name) {
                list.push(name.to_string());
            }
            write_json_atomic(&pending_path(inst), &list)?;
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// Finish removals deferred while the server was running (called before every start).
pub fn apply_pending_removals(inst: &ServerInstance) -> u32 {
    let list: Vec<String> = read_json(&pending_path(inst)).unwrap_or_default();
    if list.is_empty() {
        return 0;
    }
    let mut n = 0;
    for name in &list {
        if checked(inst, name).map(|p| p.exists()).unwrap_or(false) && remove(inst, name).is_ok() {
            n += 1;
        }
    }
    let _ = std::fs::remove_file(pending_path(inst));
    n
}

/// Copy local jars into mods/ or plugins/.
pub fn add_files(inst: &ServerInstance, paths: &[PathBuf]) -> AppResult<Vec<String>> {
    let dir = inst.content_dir().ok_or_else(|| AppError::msg("This server software doesn't load mods or plugins – switch to Fabric, Forge, NeoForge, Quilt, Paper or Purpur first."))?;
    std::fs::create_dir_all(&dir)?;
    let mut out = vec![];
    for p in paths {
        let name = p.file_name().ok_or_else(|| AppError::msg("bad path"))?.to_string_lossy().into_owned();
        if !name.to_lowercase().ends_with(".jar") {
            return Err(AppError::msg(format!("{name} isn't a .jar file.")));
        }
        std::fs::copy(p, dir.join(&name))?;
        out.push(name);
    }
    Ok(out)
}

/// Record where a downloaded file came from.
pub fn remember(inst: &ServerInstance, sha1: &str, mut info: SourceInfo) -> AppResult<()> {
    let mut idx = load_index(inst);
    info.installed_at = now_ms();
    idx.insert(sha1.to_string(), info);
    save_index(inst, &idx)
}

/// Mods that only work on the client – they crash or do nothing on a dedicated server.
pub fn client_only(i: &ContentItem) -> bool {
    i.meta.environment.as_deref() == Some("client") || i.source.as_ref().map(|s| s.server_side == "unsupported").unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn jar(path: &Path, files: &[(&str, &str)]) {
        let f = std::fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        for (n, t) in files {
            z.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(t.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn reads_metadata_of_each_loader() {
        let root = std::env::temp_dir().join(format!("mcsm-mods-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let a = root.join("a.jar");
        jar(&a, &[("fabric.mod.json", r#"{"id":"lithium","name":"Lithium","version":"0.26.2","environment":"*","authors":["jellysquid3",{"name":"2No2Name"}]}"#)]);
        let m = read_jar_meta(&a);
        assert_eq!((m.loader.as_deref(), m.name.as_deref(), m.authors.len()), (Some("fabric"), Some("Lithium"), 2));
        let b = root.join("b.jar");
        jar(&b, &[("META-INF/neoforge.mods.toml", "modLoader=\"javafml\"\n[[mods]]\nmodId=\"jei\"\nversion=\"${file.jarVersion}\"\ndisplayName=\"Just Enough Items\"\ndescription='''\nShows items\n'''\n[[dependencies.jei]]\n"), ("META-INF/MANIFEST.MF", "Implementation-Version: 19.1.0\n")]);
        let m = read_jar_meta(&b);
        assert_eq!((m.loader.as_deref(), m.mod_id.as_deref(), m.version.as_deref(), m.description.as_deref()), (Some("neoforge"), Some("jei"), Some("19.1.0"), Some("Shows items")));
        let c = root.join("c.jar");
        jar(&c, &[("plugin.yml", "name: EssentialsX\nversion: 2.21.0\nmain: com.x.Main\ncommands:\n  name: nested\n")]);
        let m = read_jar_meta(&c);
        assert_eq!((m.loader.as_deref(), m.name.as_deref(), m.version.as_deref()), (Some("bukkit"), Some("EssentialsX"), Some("2.21.0")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn toggles_and_lists() {
        let root = std::env::temp_dir().join(format!("mcsm-mods2-{}", uuid::Uuid::new_v4()));
        let mut inst = ServerInstance { install_dir: root.clone(), software: "fabric".into(), ..Default::default() };
        inst.installed = Some(InstallState { software: "fabric".into(), ..Default::default() });
        std::fs::create_dir_all(root.join("mods")).unwrap();
        jar(&root.join("mods").join("x.jar"), &[("fabric.mod.json", r#"{"id":"x","name":"X","environment":"client"}"#)]);
        let l = list(&inst);
        assert_eq!(l.items.len(), 1);
        assert!(client_only(&l.items[0]));
        let n = set_enabled(&inst, "x.jar", false).unwrap();
        assert_eq!(n, "x.jar.disabled");
        assert!(!list(&inst).items[0].enabled);
        remove(&inst, "x.jar.disabled").unwrap();
        assert!(list(&inst).items.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}
