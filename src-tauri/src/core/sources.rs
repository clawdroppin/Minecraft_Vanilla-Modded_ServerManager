//! Online content: Modrinth (mods, plugins, datapacks, modpacks – no key needed), CurseForge
//! (optional API key) and Hangar (PaperMC's plugin repository). Installs resolve required
//! dependencies, identify jars that were dropped in by hand, check for updates, install Modrinth
//! modpacks (.mrpack) as servers and export a client pack players can import into their launcher.

use super::download;
use super::mods::{self, SourceInfo};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::{AppState, TaskReporter};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MODRINTH: &str = "https://api.modrinth.com/v2";
const CURSEFORGE: &str = "https://api.curseforge.com/v1";
const HANGAR: &str = "https://hangar.papermc.io/api/v1";
const CF_MINECRAFT: u32 = 432;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub source: String,
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub follows: u64,
    pub icon_url: String,
    pub categories: Vec<String>,
    pub client_side: String,
    pub server_side: String,
    pub updated: String,
    pub project_type: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub total: u64,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    /// release | beta | alpha
    pub version_type: String,
    pub published: String,
    pub downloads: u64,
    pub file_name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub title: String,
    pub file_name: String,
    pub dependency: bool,
    pub skipped: Option<String>,
}

fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn arr(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().map(s).collect()).unwrap_or_default()
}

async fn get_json(http: &reqwest::Client, url: &str) -> AppResult<Value> {
    let r = http.get(url).timeout(std::time::Duration::from_secs(25)).send().await?;
    if r.status().as_u16() == 404 {
        return Err(AppError::msg("Not found on the content site."));
    }
    Ok(r.error_for_status()?.json().await?)
}

/// What a server can install, and where it goes.
pub fn target_dir(inst: &ServerInstance, project_type: &str) -> AppResult<PathBuf> {
    match project_type {
        "datapack" => Ok(inst.world_dir().join("datapacks")),
        _ => inst.content_dir().ok_or_else(|| AppError::msg("Vanilla servers can't load mods or plugins – only datapacks. Switch to Fabric/Forge/NeoForge/Quilt for mods or Paper/Purpur for plugins.")),
    }
}

/// Default project type for a server's software.
pub fn default_type(inst: &ServerInstance) -> &'static str {
    match content_kind(inst.active_software()) {
        "mods" => "mod",
        "plugins" => "plugin",
        _ => "datapack",
    }
}

// ------------------------------------------------------------------------------- Modrinth

fn mr_facets(project_type: &str, loaders: &[&str], mc: &str) -> String {
    let mut f: Vec<Vec<String>> = vec![vec![format!("project_type:{project_type}")]];
    if project_type != "datapack" && project_type != "modpack" && !loaders.is_empty() {
        f.push(loaders.iter().map(|l| format!("categories:{l}")).collect());
    }
    if project_type == "datapack" {
        f.push(vec!["categories:datapack".into()]);
    }
    if !mc.is_empty() {
        f.push(vec![format!("versions:{mc}")]);
    }
    if project_type == "mod" || project_type == "modpack" {
        f.push(vec!["server_side:required".into(), "server_side:optional".into()]);
    }
    serde_json::to_string(&f).unwrap_or_default()
}

pub async fn modrinth_search(state: &AppState, query: &str, project_type: &str, loaders: &[&str], mc: &str, sort: &str, offset: u32) -> AppResult<SearchPage> {
    let index = match sort {
        "downloads" | "follows" | "newest" | "updated" => sort,
        _ => "relevance",
    };
    let url = format!(
        "{MODRINTH}/search?query={}&facets={}&index={index}&offset={offset}&limit=24",
        urlencoding::encode(query.trim()),
        urlencoding::encode(&mr_facets(project_type, loaders, mc))
    );
    let v = get_json(&state.http, &url).await?;
    let hits = v["hits"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|h| SearchHit {
            source: "modrinth".into(),
            id: s(&h["project_id"]),
            slug: s(&h["slug"]),
            title: s(&h["title"]),
            description: s(&h["description"]),
            author: s(&h["author"]),
            downloads: h["downloads"].as_u64().unwrap_or(0),
            follows: h["follows"].as_u64().unwrap_or(0),
            icon_url: s(&h["icon_url"]),
            categories: arr(&h["display_categories"]),
            client_side: s(&h["client_side"]),
            server_side: s(&h["server_side"]),
            updated: s(&h["date_modified"]),
            project_type: s(&h["project_type"]),
            url: format!("https://modrinth.com/{}/{}", s(&h["project_type"]), s(&h["slug"])),
        })
        .collect();
    Ok(SearchPage { hits, total: v["total_hits"].as_u64().unwrap_or(0), offset })
}

fn mr_version(v: &Value) -> ProjectVersion {
    let file = primary_file(v);
    ProjectVersion {
        id: s(&v["id"]),
        name: s(&v["name"]),
        version_number: s(&v["version_number"]),
        game_versions: arr(&v["game_versions"]),
        loaders: arr(&v["loaders"]),
        version_type: s(&v["version_type"]),
        published: s(&v["date_published"]),
        downloads: v["downloads"].as_u64().unwrap_or(0),
        file_name: file.map(|f| s(&f["filename"])).unwrap_or_default(),
        size: file.and_then(|f| f["size"].as_u64()).unwrap_or(0),
    }
}

fn primary_file(v: &Value) -> Option<&Value> {
    let files = v["files"].as_array()?;
    files.iter().find(|f| f["primary"].as_bool() == Some(true)).or_else(|| files.first())
}

async fn mr_versions_raw(state: &AppState, project: &str, loaders: &[&str], mc: &str) -> AppResult<Vec<Value>> {
    let mut url = format!("{MODRINTH}/project/{}/version?", urlencoding::encode(project));
    if !loaders.is_empty() {
        url.push_str(&format!("loaders={}&", urlencoding::encode(&serde_json::to_string(loaders).unwrap())));
    }
    if !mc.is_empty() {
        url.push_str(&format!("game_versions={}", urlencoding::encode(&serde_json::to_string(&[mc]).unwrap())));
    }
    Ok(get_json(&state.http, &url).await?.as_array().cloned().unwrap_or_default())
}

pub async fn modrinth_versions(state: &AppState, project: &str, loaders: &[&str], mc: &str) -> AppResult<Vec<ProjectVersion>> {
    Ok(mr_versions_raw(state, project, loaders, mc).await?.iter().map(mr_version).collect())
}

/// Loaders to ask Modrinth for, by project type.
fn mr_loaders(inst: &ServerInstance, project_type: &str) -> Vec<&'static str> {
    match project_type {
        "datapack" => vec!["datapack"],
        _ => content_loaders(inst.active_software()),
    }
}

fn pick_version(list: &[Value]) -> Option<&Value> {
    list.iter().find(|v| s(&v["version_type"]) == "release").or_else(|| list.first())
}

/// Install a Modrinth project (specific version or the newest compatible) plus its required
/// dependencies.
pub async fn modrinth_install(state: &AppState, inst: &ServerInstance, project: &str, version_id: Option<&str>, project_type: &str, reporter: &TaskReporter) -> AppResult<Vec<Installed>> {
    let mc = inst.active_mc_version().to_string();
    let mut out = vec![];
    let mut seen: HashSet<String> = mods::load_index(inst).values().filter(|i| i.source == "modrinth").map(|i| i.project_id.clone()).collect();
    let mut queue: Vec<(String, Option<String>, bool)> = vec![(project.to_string(), version_id.map(String::from), false)];
    while let Some((pid, vid, dep)) = queue.pop() {
        let v = match &vid {
            Some(id) => get_json(&state.http, &format!("{MODRINTH}/version/{id}")).await?,
            None => {
                let list = mr_versions_raw(state, &pid, &mr_loaders(inst, project_type), &mc).await?;
                match pick_version(&list) {
                    Some(v) => v.clone(),
                    None if dep => {
                        out.push(Installed { title: pid.clone(), file_name: String::new(), dependency: true, skipped: Some(format!("no version for {mc}")) });
                        continue;
                    }
                    None => return Err(AppError::msg(format!("This project has no version for {} {mc}.", software_label(inst.active_software())))),
                }
            }
        };
        let project_id = s(&v["project_id"]);
        if dep && seen.contains(&project_id) {
            continue;
        }
        seen.insert(project_id.clone());
        let proj = get_json(&state.http, &format!("{MODRINTH}/project/{project_id}")).await.unwrap_or(Value::Null);
        let ptype = if proj["project_type"].is_string() { s(&proj["project_type"]) } else { project_type.to_string() };
        let title = s(&proj["title"]);
        if ptype == "mod" && s(&proj["server_side"]) == "unsupported" {
            out.push(Installed { title: title.clone(), file_name: String::new(), dependency: dep, skipped: Some("client-only mod – not needed on a server".into()) });
            continue;
        }
        let file = primary_file(&v).ok_or_else(|| AppError::msg(format!("{title} has no downloadable file.")))?;
        let fname = s(&file["filename"]);
        let dir = target_dir(inst, if arr(&v["loaders"]).iter().all(|l| l == "datapack") { "datapack" } else { &ptype })?;
        tokio::fs::create_dir_all(&dir).await?;
        let dest = dir.join(&fname);
        reporter.progress(&format!("Downloading {title}…"), None);
        download::download_verified(&state.http, &s(&file["url"]), &dest, file["hashes"]["sha1"].as_str(), None, Some(reporter), &title).await?;
        let sha1 = s(&file["hashes"]["sha1"]);
        // Replacing an older version of the same project: retire the old file (recoverable).
        if inst.content_dir().is_some() {
            let old: Vec<String> = mods::list(inst)
                .items
                .into_iter()
                .filter(|i| i.sha1 != sha1 && i.file_name != fname && i.source.as_ref().map(|s| s.project_id == project_id).unwrap_or(false))
                .map(|i| i.file_name)
                .collect();
            for f in old {
                if !mods::remove_or_defer(inst, &f)? {
                    reporter.log(&format!("{f} is in use – it will be removed before the next start."));
                }
            }
        }
        mods::remember(
            inst,
            &sha1,
            SourceInfo {
                source: "modrinth".into(),
                project_id: project_id.clone(),
                version_id: s(&v["id"]),
                slug: s(&proj["slug"]),
                title: title.clone(),
                version_number: s(&v["version_number"]),
                icon_url: s(&proj["icon_url"]),
                client_side: s(&proj["client_side"]),
                server_side: s(&proj["server_side"]),
                installed_at: 0,
                dependency: dep,
            },
        )?;
        out.push(Installed { title, file_name: fname, dependency: dep, skipped: None });
        for d in v["dependencies"].as_array().cloned().unwrap_or_default() {
            if s(&d["dependency_type"]) != "required" {
                continue;
            }
            let dpid = s(&d["project_id"]);
            let dvid = d["version_id"].as_str().map(String::from);
            if !dpid.is_empty() && !seen.contains(&dpid) {
                queue.push((dpid, dvid, true));
            } else if dpid.is_empty() {
                if let Some(dv) = dvid {
                    queue.push((String::new(), Some(dv), true));
                }
            }
        }
    }
    Ok(out)
}

/// Look up jars nobody told us about (dropped in by hand, from a modpack…) on Modrinth by hash.
pub async fn modrinth_identify(state: &AppState, inst: &ServerInstance) -> AppResult<u32> {
    let listing = mods::list(inst);
    let unknown: Vec<String> = listing.items.iter().filter(|i| i.source.is_none() && !i.sha1.is_empty()).map(|i| i.sha1.clone()).collect();
    if unknown.is_empty() {
        return Ok(0);
    }
    let r: Value = state.http.post(format!("{MODRINTH}/version_files")).json(&json!({ "hashes": unknown, "algorithm": "sha1" })).send().await?.error_for_status()?.json().await?;
    let Some(map) = r.as_object() else { return Ok(0) };
    let ids: Vec<String> = map.values().map(|v| s(&v["project_id"])).collect::<HashSet<_>>().into_iter().collect();
    let projects = if ids.is_empty() {
        Value::Array(vec![])
    } else {
        get_json(&state.http, &format!("{MODRINTH}/projects?ids={}", urlencoding::encode(&serde_json::to_string(&ids).unwrap()))).await?
    };
    let by_id: HashMap<String, Value> = projects.as_array().cloned().unwrap_or_default().into_iter().map(|p| (s(&p["id"]), p)).collect();
    let mut idx = mods::load_index(inst);
    let mut n = 0;
    for (sha1, v) in map {
        let p = by_id.get(&s(&v["project_id"])).cloned().unwrap_or(Value::Null);
        idx.insert(
            sha1.clone(),
            SourceInfo {
                source: "modrinth".into(),
                project_id: s(&v["project_id"]),
                version_id: s(&v["id"]),
                slug: s(&p["slug"]),
                title: s(&p["title"]),
                version_number: s(&v["version_number"]),
                icon_url: s(&p["icon_url"]),
                client_side: s(&p["client_side"]),
                server_side: s(&p["server_side"]),
                installed_at: crate::state::now_ms(),
                dependency: false,
            },
        );
        n += 1;
    }
    mods::save_index(inst, &idx)?;
    Ok(n)
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCandidate {
    pub file_name: String,
    pub title: String,
    pub project_id: String,
    pub current: String,
    pub latest: String,
    pub version_id: String,
    pub source: String,
}

/// Newer compatible versions of installed Modrinth content.
pub async fn modrinth_updates(state: &AppState, inst: &ServerInstance) -> AppResult<Vec<UpdateCandidate>> {
    let listing = mods::list(inst);
    let tracked: Vec<&mods::ContentItem> = listing.items.iter().filter(|i| i.source.as_ref().map(|s| s.source == "modrinth").unwrap_or(false)).collect();
    if tracked.is_empty() {
        return Ok(vec![]);
    }
    let hashes: Vec<&str> = tracked.iter().map(|i| i.sha1.as_str()).collect();
    let body = json!({ "hashes": hashes, "algorithm": "sha1", "loaders": content_loaders(inst.active_software()), "game_versions": [inst.active_mc_version()] });
    let r: Value = state.http.post(format!("{MODRINTH}/version_files/update")).json(&body).send().await?.error_for_status()?.json().await?;
    let mut out = vec![];
    for it in tracked {
        let Some(v) = r.get(&it.sha1) else { continue };
        let src = it.source.as_ref().unwrap();
        if s(&v["id"]) != src.version_id && primary_file(v).map(|f| s(&f["hashes"]["sha1"])) != Some(it.sha1.clone()) {
            out.push(UpdateCandidate {
                file_name: it.file_name.clone(),
                title: src.title.clone(),
                project_id: src.project_id.clone(),
                current: src.version_number.clone(),
                latest: s(&v["version_number"]),
                version_id: s(&v["id"]),
                source: "modrinth".into(),
            });
        }
    }
    Ok(out)
}

// ----------------------------------------------------------------------------- CurseForge

fn cf_loader(software: &str) -> Option<u32> {
    match software {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

fn cf_class(project_type: &str) -> u32 {
    match project_type {
        "plugin" => 5,
        "modpack" => 4471,
        "datapack" => 6945,
        _ => 6,
    }
}

async fn cf_get(state: &AppState, path: &str) -> AppResult<Value> {
    let key = state.settings.read().await.curseforge_key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::msg("Add a CurseForge API key under App settings → Content sources to search CurseForge."));
    }
    let r = state.http.get(format!("{CURSEFORGE}{path}")).header("x-api-key", key).header("Accept", "application/json").timeout(std::time::Duration::from_secs(25)).send().await?;
    if r.status().as_u16() == 403 || r.status().as_u16() == 401 {
        return Err(AppError::msg("CurseForge rejected the API key – check it under App settings."));
    }
    Ok(r.error_for_status()?.json().await?)
}

pub async fn curseforge_search(state: &AppState, inst: &ServerInstance, query: &str, project_type: &str, sort: &str, offset: u32) -> AppResult<SearchPage> {
    let mut path = format!("/mods/search?gameId={CF_MINECRAFT}&classId={}&searchFilter={}&pageSize=24&index={offset}&sortOrder=desc", cf_class(project_type), urlencoding::encode(query.trim()));
    path.push_str(match sort {
        "downloads" => "&sortField=6",
        "updated" | "newest" => "&sortField=3",
        "follows" => "&sortField=2",
        _ => if query.trim().is_empty() { "&sortField=2" } else { "&sortField=1" },
    });
    let mc = inst.active_mc_version();
    if !mc.is_empty() {
        path.push_str(&format!("&gameVersion={}", urlencoding::encode(mc)));
    }
    if project_type == "mod" {
        if let Some(l) = cf_loader(inst.active_software()) {
            path.push_str(&format!("&modLoaderType={l}"));
        }
    }
    let v = cf_get(state, &path).await?;
    let hits = v["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|h| SearchHit {
            source: "curseforge".into(),
            id: s(&h["id"]),
            slug: s(&h["slug"]),
            title: s(&h["name"]),
            description: s(&h["summary"]),
            author: h["authors"].as_array().and_then(|a| a.first()).map(|a| s(&a["name"])).unwrap_or_default(),
            downloads: h["downloadCount"].as_f64().unwrap_or(0.0) as u64,
            follows: h["thumbsUpCount"].as_u64().unwrap_or(0),
            icon_url: s(&h["logo"]["thumbnailUrl"]),
            categories: h["categories"].as_array().map(|a| a.iter().map(|c| s(&c["name"])).collect()).unwrap_or_default(),
            client_side: String::new(),
            server_side: String::new(),
            updated: s(&h["dateModified"]),
            project_type: project_type.into(),
            url: s(&h["links"]["websiteUrl"]),
        })
        .collect();
    Ok(SearchPage { hits, total: v["pagination"]["totalCount"].as_u64().unwrap_or(0), offset })
}

async fn cf_files(state: &AppState, inst: &ServerInstance, mod_id: &str, project_type: &str) -> AppResult<Vec<Value>> {
    let mut path = format!("/mods/{mod_id}/files?pageSize=50");
    let mc = inst.active_mc_version();
    if !mc.is_empty() {
        path.push_str(&format!("&gameVersion={}", urlencoding::encode(mc)));
    }
    if project_type == "mod" {
        if let Some(l) = cf_loader(inst.active_software()) {
            path.push_str(&format!("&modLoaderType={l}"));
        }
    }
    let mut files = cf_get(state, &path).await?["data"].as_array().cloned().unwrap_or_default();
    files.sort_by(|a, b| s(&b["fileDate"]).cmp(&s(&a["fileDate"])));
    Ok(files)
}

pub async fn curseforge_versions(state: &AppState, inst: &ServerInstance, mod_id: &str, project_type: &str) -> AppResult<Vec<ProjectVersion>> {
    Ok(cf_files(state, inst, mod_id, project_type)
        .await?
        .iter()
        .map(|f| ProjectVersion {
            id: s(&f["id"]),
            name: s(&f["displayName"]),
            version_number: s(&f["displayName"]),
            game_versions: arr(&f["gameVersions"]),
            loaders: vec![],
            version_type: match f["releaseType"].as_u64() {
                Some(2) => "beta".into(),
                Some(3) => "alpha".into(),
                _ => "release".into(),
            },
            published: s(&f["fileDate"]),
            downloads: f["downloadCount"].as_u64().unwrap_or(0),
            file_name: s(&f["fileName"]),
            size: f["fileLength"].as_u64().unwrap_or(0),
        })
        .collect())
}

pub async fn curseforge_install(state: &AppState, inst: &ServerInstance, mod_id: &str, file_id: Option<&str>, project_type: &str, reporter: &TaskReporter) -> AppResult<Vec<Installed>> {
    let mut out = vec![];
    let mut seen: HashSet<String> = mods::load_index(inst).values().filter(|i| i.source == "curseforge").map(|i| i.project_id.clone()).collect();
    let mut queue = vec![(mod_id.to_string(), file_id.map(String::from), false)];
    while let Some((mid, fid, dep)) = queue.pop() {
        if dep && seen.contains(&mid) {
            continue;
        }
        seen.insert(mid.clone());
        let proj = cf_get(state, &format!("/mods/{mid}")).await?["data"].clone();
        let title = s(&proj["name"]);
        let file = match fid {
            Some(f) => cf_get(state, &format!("/mods/{mid}/files/{f}")).await?["data"].clone(),
            None => {
                let files = cf_files(state, inst, &mid, project_type).await?;
                match files.iter().find(|f| f["releaseType"].as_u64() == Some(1)).or(files.first()).cloned() {
                    Some(f) => f,
                    None if dep => {
                        out.push(Installed { title, file_name: String::new(), dependency: true, skipped: Some("no compatible file".into()) });
                        continue;
                    }
                    None => return Err(AppError::msg(format!("{title} has no file for this Minecraft version / loader."))),
                }
            }
        };
        let Some(url) = file["downloadUrl"].as_str().filter(|u| !u.is_empty()) else {
            let msg = format!("The author of {title} doesn't allow downloads through other apps – download it from CurseForge and add the file manually.");
            if dep {
                out.push(Installed { title, file_name: String::new(), dependency: true, skipped: Some(msg) });
                continue;
            }
            return Err(AppError::msg(msg));
        };
        let fname = s(&file["fileName"]);
        let dir = target_dir(inst, project_type)?;
        tokio::fs::create_dir_all(&dir).await?;
        let sha1 = file["hashes"].as_array().and_then(|h| h.iter().find(|x| x["algo"].as_u64() == Some(1))).map(|x| s(&x["value"]));
        reporter.progress(&format!("Downloading {title}…"), None);
        download::download_verified(&state.http, url, &dir.join(&fname), sha1.as_deref(), None, Some(reporter), &title).await?;
        let sha1 = match sha1 {
            Some(h) => h,
            None => download::sha1_file(&dir.join(&fname)).unwrap_or_default(),
        };
        mods::remember(
            inst,
            &sha1,
            SourceInfo {
                source: "curseforge".into(),
                project_id: mid.clone(),
                version_id: s(&file["id"]),
                slug: s(&proj["slug"]),
                title: title.clone(),
                version_number: s(&file["displayName"]),
                icon_url: s(&proj["logo"]["thumbnailUrl"]),
                dependency: dep,
                ..Default::default()
            },
        )?;
        out.push(Installed { title, file_name: fname, dependency: dep, skipped: None });
        for d in file["dependencies"].as_array().cloned().unwrap_or_default() {
            if d["relationType"].as_u64() == Some(3) {
                let id = s(&d["modId"]);
                if !seen.contains(&id) {
                    queue.push((id, None, true));
                }
            }
        }
    }
    Ok(out)
}

// --------------------------------------------------------------------------------- Hangar

pub async fn hangar_search(state: &AppState, inst: &ServerInstance, query: &str, sort: &str, offset: u32) -> AppResult<SearchPage> {
    let order = match sort {
        "downloads" => "-downloads",
        "updated" | "newest" => "-updated",
        "follows" => "-stars",
        _ => if query.trim().is_empty() { "-downloads" } else { "" },
    };
    let mut url = format!("{HANGAR}/projects?limit=24&offset={offset}&platform=PAPER&q={}", urlencoding::encode(query.trim()));
    if !order.is_empty() {
        url.push_str(&format!("&sort={order}"));
    }
    let mc = inst.active_mc_version();
    if !mc.is_empty() {
        url.push_str(&format!("&version={}", urlencoding::encode(mc)));
    }
    let v = get_json(&state.http, &url).await?;
    let hits = v["result"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|h| {
            let owner = s(&h["namespace"]["owner"]);
            let slug = s(&h["namespace"]["slug"]);
            SearchHit {
                source: "hangar".into(),
                id: slug.clone(),
                slug: slug.clone(),
                title: s(&h["name"]),
                description: s(&h["description"]),
                author: owner.clone(),
                downloads: h["stats"]["downloads"].as_u64().unwrap_or(0),
                follows: h["stats"]["stars"].as_u64().unwrap_or(0),
                icon_url: s(&h["avatarUrl"]),
                categories: vec![s(&h["category"]).replace('_', " ")],
                updated: s(&h["lastUpdated"]),
                project_type: "plugin".into(),
                url: format!("https://hangar.papermc.io/{owner}/{slug}"),
                ..Default::default()
            }
        })
        .collect();
    Ok(SearchPage { hits, total: v["pagination"]["count"].as_u64().unwrap_or(0), offset })
}

async fn hangar_versions_raw(state: &AppState, inst: &ServerInstance, slug: &str) -> AppResult<Vec<Value>> {
    let mc = inst.active_mc_version();
    let mut url = format!("{HANGAR}/projects/{}/versions?limit=25&platform=PAPER", urlencoding::encode(slug));
    if !mc.is_empty() {
        url.push_str(&format!("&platformVersion={}", urlencoding::encode(mc)));
    }
    Ok(get_json(&state.http, &url).await?["result"].as_array().cloned().unwrap_or_default())
}

pub async fn hangar_versions(state: &AppState, inst: &ServerInstance, slug: &str) -> AppResult<Vec<ProjectVersion>> {
    Ok(hangar_versions_raw(state, inst, slug)
        .await?
        .iter()
        .map(|v| ProjectVersion {
            id: s(&v["name"]),
            name: s(&v["name"]),
            version_number: s(&v["name"]),
            game_versions: arr(&v["platformDependencies"]["PAPER"]),
            loaders: vec!["paper".into()],
            version_type: s(&v["channel"]["name"]).to_lowercase(),
            published: s(&v["createdAt"]),
            downloads: v["stats"]["totalDownloads"].as_u64().unwrap_or(0),
            file_name: s(&v["downloads"]["PAPER"]["fileInfo"]["name"]),
            size: v["downloads"]["PAPER"]["fileInfo"]["sizeBytes"].as_u64().unwrap_or(0),
        })
        .collect())
}

pub async fn hangar_install(state: &AppState, inst: &ServerInstance, slug: &str, version: Option<&str>, reporter: &TaskReporter) -> AppResult<Vec<Installed>> {
    let list = hangar_versions_raw(state, inst, slug).await?;
    let v = match version {
        Some(name) => list.iter().find(|v| s(&v["name"]) == name).cloned(),
        None => list.iter().find(|v| s(&v["channel"]["name"]).eq_ignore_ascii_case("release")).or_else(|| list.first()).cloned(),
    }
    .ok_or_else(|| AppError::msg(format!("{slug} has no version for Paper {}.", inst.active_mc_version())))?;
    let d = &v["downloads"]["PAPER"];
    let Some(url) = d["downloadUrl"].as_str().filter(|u| !u.is_empty()) else {
        return Err(AppError::msg(format!("{slug} is hosted elsewhere ({}) – download it there and add the jar manually.", s(&d["externalUrl"]))));
    };
    let fname = s(&d["fileInfo"]["name"]);
    let dir = target_dir(inst, "plugin")?;
    tokio::fs::create_dir_all(&dir).await?;
    reporter.progress(&format!("Downloading {slug}…"), None);
    download::download_verified(&state.http, url, &dir.join(&fname), None, d["fileInfo"]["sha256Hash"].as_str(), Some(reporter), slug).await?;
    let sha1 = download::sha1_file(&dir.join(&fname)).unwrap_or_default();
    mods::remember(inst, &sha1, SourceInfo { source: "hangar".into(), project_id: slug.into(), version_id: s(&v["name"]), slug: slug.into(), title: slug.into(), version_number: s(&v["name"]), ..Default::default() })?;
    Ok(vec![Installed { title: slug.into(), file_name: fname, dependency: false, skipped: None }])
}

// ------------------------------------------------------------------------ Bedrock crossplay

const GEYSER_DL: &str = "https://download.geysermc.org/v2/projects";

/// Let Bedrock players (phones, consoles, Windows edition) join: Geyser translates the Bedrock
/// protocol, Floodgate lets them in without a Java account. Geyser comes from Modrinth, where every
/// build lists the Minecraft versions it supports – the newest build fails to enable on a release
/// it doesn't know yet (tested: Geyser 2.11.3 on Paper 26.3), so an unsupported version is refused
/// up front. Floodgate for Paper/Purpur isn't on Modrinth: it comes from GeyserMC's own download
/// API (sha256-verified, works across versions); the Fabric/NeoForge Floodgate is on Modrinth.
pub async fn install_bedrock(state: &AppState, inst: &ServerInstance, reporter: &TaskReporter) -> AppResult<Vec<Installed>> {
    let sw = inst.active_software();
    let plugin = match sw {
        "paper" | "purpur" => true,
        "fabric" | "quilt" | "neoforge" => false,
        "folia" => return Err(AppError::msg("Geyser doesn't run on Folia. Use Paper or Purpur for Bedrock crossplay.")),
        _ => return Err(AppError::msg("Bedrock crossplay needs Paper, Purpur, Fabric, Quilt or NeoForge – Vanilla and Forge can't load Geyser. Switch under Version & Software.")),
    };
    let mc = inst.active_mc_version();
    let loaders = content_loaders(sw);
    if mr_versions_raw(state, "geyser", &loaders, mc).await?.is_empty() {
        let newest = mr_versions_raw(state, "geyser", &loaders, "").await.ok().and_then(|l| l.first().and_then(|v| v["game_versions"].as_array().and_then(|g| g.last()).map(s))).unwrap_or_default();
        return Err(AppError::msg(format!(
            "Geyser doesn't support {} {mc} yet{}. GeyserMC usually catches up within days of a Minecraft release – try again later.",
            software_label(sw),
            if newest.is_empty() { String::new() } else { format!(" (newest supported: {newest})") }
        )));
    }
    let mut out = modrinth_install(state, inst, "geyser", None, if plugin { "plugin" } else { "mod" }, reporter).await?;
    if !plugin {
        out.extend(modrinth_install(state, inst, "floodgate", None, "mod", reporter).await?);
        return Ok(out);
    }
    let dir = target_dir(inst, "plugin")?;
    let b = get_json(&state.http, &format!("{GEYSER_DL}/floodgate/versions/latest/builds/latest")).await?;
    let (version, build) = (s(&b["version"]), s(&b["build"]));
    let d = &b["downloads"]["spigot"];
    let fname = s(&d["name"]);
    if fname.is_empty() || version.is_empty() {
        return Err(AppError::msg("GeyserMC has no Floodgate download for Paper right now – try again later."));
    }
    let dest = dir.join(&fname);
    let current = mods::load_index(inst).values().any(|i| i.source == "geysermc" && i.project_id == "floodgate" && i.version_id == build);
    if dest.exists() && current {
        out.push(Installed { title: "Floodgate".into(), file_name: fname, dependency: true, skipped: Some("already up to date".into()) });
        return Ok(out);
    }
    reporter.progress(&format!("Downloading Floodgate {version}…"), None);
    let existed = dest.exists();
    let url = format!("{GEYSER_DL}/floodgate/versions/{version}/builds/{build}/downloads/spigot");
    download::download_verified(&state.http, &url, &dest, None, d["sha256"].as_str(), Some(reporter), "Floodgate").await.map_err(|e| {
        if existed {
            AppError::msg(format!("Floodgate couldn't be replaced – stop the server to update it ({e})."))
        } else {
            e
        }
    })?;
    let sha1 = download::sha1_file(&dest).unwrap_or_default();
    mods::remember(
        inst,
        &sha1,
        SourceInfo {
            source: "geysermc".into(),
            project_id: "floodgate".into(),
            version_id: build.clone(),
            slug: "floodgate".into(),
            title: "Floodgate".into(),
            version_number: format!("{version} (build {build})"),
            server_side: "required".into(),
            dependency: true,
            ..Default::default()
        },
    )?;
    out.push(Installed { title: "Floodgate".into(), file_name: fname, dependency: true, skipped: None });
    Ok(out)
}

// ---------------------------------------------------------------------------- Modpacks

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub software: String,
    pub mc_version: String,
    pub loader_version: String,
    pub name: String,
    pub version_name: String,
    pub files: u32,
    pub server_files: u32,
}

/// Read a downloaded .mrpack's index: which software/version the server needs.
pub fn read_mrpack(path: &Path) -> AppResult<(Value, PackInfo)> {
    let f = std::fs::File::open(path)?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| AppError::msg("That isn't a Modrinth modpack (.mrpack)."))?;
    let mut s_ = String::new();
    z.by_name("modrinth.index.json").map_err(|_| AppError::msg("The modpack has no modrinth.index.json."))?.read_to_string(&mut s_)?;
    let idx: Value = serde_json::from_str(&s_)?;
    let deps = &idx["dependencies"];
    let (software, loader) = if let Some(v) = deps["neoforge"].as_str() {
        ("neoforge", v)
    } else if let Some(v) = deps["forge"].as_str() {
        ("forge", v)
    } else if let Some(v) = deps["quilt-loader"].as_str() {
        ("quilt", v)
    } else if let Some(v) = deps["fabric-loader"].as_str() {
        ("fabric", v)
    } else {
        ("vanilla", "")
    };
    let files = idx["files"].as_array().cloned().unwrap_or_default();
    let server_files = files.iter().filter(|f| s(&f["env"]["server"]) != "unsupported").count() as u32;
    let info = PackInfo {
        software: software.into(),
        mc_version: s(&deps["minecraft"]),
        // Forge in .mrpack is "47.2.0"; our Forge ids are the same.
        loader_version: loader.to_string(),
        name: s(&idx["name"]),
        version_name: s(&idx["versionId"]),
        files: files.len() as u32,
        server_files,
    };
    Ok((idx, info))
}

/// The Minecraft version a modpack version is made for.
pub async fn modpack_game_version(state: &AppState, version_id: &str) -> AppResult<String> {
    let v = get_json(&state.http, &format!("{MODRINTH}/version/{}", urlencoding::encode(version_id))).await?;
    Ok(v["game_versions"].as_array().and_then(|a| a.first()).map(s).unwrap_or_default())
}

/// Download a modpack version's .mrpack into the server's meta folder.
pub async fn fetch_mrpack(state: &AppState, version_id: &str, dest_dir: &Path, reporter: &TaskReporter) -> AppResult<(PathBuf, Value)> {
    let v = get_json(&state.http, &format!("{MODRINTH}/version/{version_id}")).await?;
    let file = v["files"]
        .as_array()
        .and_then(|a| a.iter().find(|f| s(&f["filename"]).ends_with(".mrpack")).or_else(|| primary_file(&v)))
        .cloned()
        .ok_or_else(|| AppError::msg("That modpack version has no .mrpack file."))?;
    let dest = dest_dir.join("modpack.mrpack");
    download::download_verified(&state.http, &s(&file["url"]), &dest, file["hashes"]["sha1"].as_str(), None, Some(reporter), "modpack").await?;
    Ok((dest, v))
}

/// Install a .mrpack's server-side files and overrides into the server folder (the software
/// itself is installed by the caller from `read_mrpack`'s info).
pub async fn apply_mrpack(state: &AppState, inst: &ServerInstance, pack: &Path, reporter: &TaskReporter) -> AppResult<u32> {
    let (idx, _) = read_mrpack(pack)?;
    let root = inst.install_dir.clone();
    let files = idx["files"].as_array().cloned().unwrap_or_default();
    // (relative path, destination, url, sha1) of every server-side file.
    let jobs: Vec<(String, PathBuf, String, Option<String>)> = files
        .iter()
        .filter(|f| s(&f["env"]["server"]) != "unsupported")
        .filter_map(|f| {
            let rel = s(&f["path"]).replace('\\', "/");
            if rel.is_empty() || rel.contains("..") || rel.starts_with('/') || rel.contains(':') {
                return None;
            }
            let dest = root.join(&rel);
            let url = f["downloads"].as_array().and_then(|a| a.first()).map(s).unwrap_or_default();
            (super::proc::is_within(&dest, &root) && !url.is_empty()).then(|| (rel, dest, url, f["hashes"]["sha1"].as_str().map(String::from)))
        })
        .collect();
    let total = jobs.len().max(1);
    // Packs have 100+ small files: a few downloads at once is much faster than one by one.
    use futures_util::StreamExt;
    let http = &state.http;
    let mut stream = futures_util::stream::iter(jobs.into_iter().map(|(rel, dest, url, sha1)| async move {
        download::download_retry(http, &url, &dest, sha1.as_deref(), None, &rel).await.map(|_| rel)
    }))
    .buffer_unordered(6);
    let mut n = 0u32;
    while let Some(res) = stream.next().await {
        let rel = res?;
        n += 1;
        reporter.progress(&format!("Modpack files {n}/{total}: {}", rel.rsplit('/').next().unwrap_or(&rel)), Some(n as f64 / total as f64));
    }
    drop(stream);
    // overrides/ then server-overrides/ (the latter wins).
    let p = pack.to_path_buf();
    let r2 = root.clone();
    n += tokio::task::spawn_blocking(move || -> AppResult<u32> {
        let f = std::fs::File::open(&p)?;
        let mut z = zip::ZipArchive::new(f)?;
        let mut n = 0;
        for prefix in ["overrides/", "server-overrides/"] {
            for i in 0..z.len() {
                let mut e = z.by_index(i)?;
                let Some(name) = e.enclosed_name() else { continue };
                let name = name.to_string_lossy().replace('\\', "/");
                let Some(rel) = name.strip_prefix(prefix) else { continue };
                if rel.is_empty() || e.is_dir() {
                    continue;
                }
                let out = r2.join(rel);
                if let Some(pp) = out.parent() {
                    std::fs::create_dir_all(pp)?;
                }
                let mut w = std::fs::File::create(&out)?;
                std::io::copy(&mut e, &mut w)?;
                n += 1;
            }
        }
        Ok(n)
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))??;
    // Learn where the jars came from so updates and the client pack work.
    let _ = modrinth_identify(state, inst).await;
    Ok(n)
}

/// Build a .mrpack with the server's client-relevant mods so players can import it into the
/// Modrinth App, Prism Launcher, ATLauncher… and join with matching mods.
pub async fn export_client_pack(state: &AppState, inst: &ServerInstance, dest: &Path, include_unknown: bool) -> AppResult<(u32, u32)> {
    let listing = mods::list(inst);
    let enabled: Vec<&mods::ContentItem> = listing.items.iter().filter(|i| i.enabled).collect();
    let hashes: Vec<&str> = enabled.iter().map(|i| i.sha1.as_str()).collect();
    let found: Value = if hashes.is_empty() {
        json!({})
    } else {
        state.http.post(format!("{MODRINTH}/version_files")).json(&json!({ "hashes": hashes, "algorithm": "sha1" })).send().await?.error_for_status()?.json().await?
    };
    let idx = mods::load_index(inst);
    let mut files = vec![];
    let mut unknown = vec![];
    for it in &enabled {
        let client_side = idx.get(&it.sha1).map(|s| s.client_side.clone()).unwrap_or_default();
        if client_side == "unsupported" || it.meta.environment.as_deref() == Some("server") {
            continue; // server-only
        }
        if let Some(v) = found.get(&it.sha1) {
            let f = v["files"].as_array().and_then(|a| a.iter().find(|f| s(&f["hashes"]["sha1"]) == it.sha1)).cloned().unwrap_or(Value::Null);
            files.push(json!({
                "path": format!("mods/{}", it.file_name),
                "hashes": { "sha1": it.sha1, "sha512": s(&f["hashes"]["sha512"]) },
                "env": { "client": if client_side == "optional" { "optional" } else { "required" }, "server": "required" },
                "downloads": [s(&f["url"])],
                "fileSize": it.size,
            }));
        } else {
            unknown.push(it.file_name.clone());
        }
    }
    let st = inst.installed.clone().unwrap_or_default();
    let mut deps = serde_json::Map::new();
    deps.insert("minecraft".into(), json!(st.mc_version));
    match st.software.as_str() {
        "fabric" => {
            deps.insert("fabric-loader".into(), json!(st.loader_version));
        }
        "quilt" => {
            deps.insert("quilt-loader".into(), json!(st.loader_version));
        }
        "forge" => {
            deps.insert("forge".into(), json!(st.loader_version.split('-').next().unwrap_or(&st.loader_version)));
        }
        "neoforge" => {
            deps.insert("neoforge".into(), json!(st.loader_version));
        }
        _ => {}
    }
    let index = json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": chrono::Local::now().format("%Y.%m.%d").to_string(),
        "name": format!("{} (client)", inst.name),
        "summary": format!("Mods to join {} – exported by Minecraft Server Manager", inst.name),
        "files": files,
        "dependencies": deps,
    });
    let dir = listing.dir.clone();
    let dest = dest.to_path_buf();
    let count = files.len() as u32;
    let unknown_n = unknown.len() as u32;
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let tmp = dest.with_extension("part");
        let f = std::fs::File::create(&tmp)?;
        let mut z = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.start_file("modrinth.index.json", o)?;
        z.write_all(&serde_json::to_vec_pretty(&index)?)?;
        if include_unknown {
            if let Some(d) = dir {
                for name in &unknown {
                    if let Ok(b) = std::fs::read(d.join(name)) {
                        z.start_file(format!("overrides/mods/{name}"), o)?;
                        z.write_all(&b)?;
                    }
                }
            }
        }
        z.finish()?;
        if dest.exists() {
            std::fs::remove_file(&dest)?;
        }
        std::fs::rename(&tmp, &dest)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))??;
    Ok((count, unknown_n))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_modrinth_facets() {
        let f = mr_facets("mod", &["quilt", "fabric"], "26.3");
        assert_eq!(f, r#"[["project_type:mod"],["categories:quilt","categories:fabric"],["versions:26.3"],["server_side:required","server_side:optional"]]"#);
        let f = mr_facets("datapack", &["datapack"], "1.21.8");
        assert!(f.contains("categories:datapack") && !f.contains("server_side"));
    }
    #[test]
    fn reads_mrpack_index() {
        let root = std::env::temp_dir().join(format!("mcsm-mrpack-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let p = root.join("p.mrpack");
        let f = std::fs::File::create(&p).unwrap();
        let mut z = zip::ZipWriter::new(f);
        z.start_file("modrinth.index.json", zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(br#"{"formatVersion":1,"game":"minecraft","versionId":"1.0","name":"Pack","files":[{"path":"mods/a.jar","env":{"client":"required","server":"required"}},{"path":"mods/b.jar","env":{"client":"required","server":"unsupported"}}],"dependencies":{"minecraft":"1.21.1","neoforge":"21.1.200"}}"#).unwrap();
        z.finish().unwrap();
        let (_, info) = read_mrpack(&p).unwrap();
        assert_eq!((info.software.as_str(), info.mc_version.as_str(), info.loader_version.as_str()), ("neoforge", "1.21.1", "21.1.200"));
        assert_eq!((info.files, info.server_files), (2, 1));
        let _ = std::fs::remove_dir_all(root);
    }
}
