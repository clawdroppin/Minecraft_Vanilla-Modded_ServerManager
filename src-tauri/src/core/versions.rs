//! Version catalogs for every supported server software, straight from the official sources:
//! Mojang's piston-meta manifest, PaperMC Fill (Paper / Folia), Purpur, Fabric meta, Quilt meta,
//! Forge's maven metadata and NeoForge's maven. Everything goes through `AppState::cached_json`
//! so lists load instantly the second time and still work offline from the disk cache.

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FILL: &str = "https://fill.papermc.io/v3/projects";
const PURPUR: &str = "https://api.purpurmc.org/v2/purpur";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const QUILT_META: &str = "https://meta.quiltmc.org/v3";
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const FORGE_FILES: &str = "https://files.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";
const NEOFORGE_API: &str = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";

const LIST_TTL: i64 = 10 * 60_000;
const META_TTL: i64 = 7 * 24 * 3600_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McVersion {
    pub id: String,
    /// release | snapshot | old_beta | old_alpha
    pub kind: String,
    pub release_time: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionList {
    pub latest_release: String,
    pub latest_snapshot: String,
    pub versions: Vec<McVersion>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub id: String,
    pub stable: bool,
    /// recommended | latest | beta | alpha | ""
    pub tag: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionMeta {
    pub id: String,
    pub server_url: Option<String>,
    pub sha1: Option<String>,
    pub size: u64,
    pub java_major: u32,
}

/// A concrete file to download for an install.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub url: String,
    pub file_name: String,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

pub async fn manifest(state: &AppState) -> AppResult<Value> {
    state.cached_json("mojang-manifest", MANIFEST_URL, LIST_TTL).await
}

/// Mojang's manifest as (index by id, list). Index 0 = newest.
async fn manifest_index(state: &AppState) -> AppResult<(HashMap<String, (usize, String, String)>, Value)> {
    let m = manifest(state).await?;
    let mut idx = HashMap::new();
    for (i, v) in m["versions"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        idx.insert(s(&v["id"]), (i, s(&v["type"]), s(&v["releaseTime"])));
    }
    Ok((idx, m))
}

/// Guess release vs snapshot for ids Mojang's manifest doesn't know (loader-specific lists).
fn guess_kind(id: &str) -> &'static str {
    let l = id.to_lowercase();
    if l.contains("pre") || l.contains("rc") || l.contains("snapshot") || l.contains('w') && l.chars().any(|c| c.is_ascii_digit()) && !l.contains('.') {
        "snapshot"
    } else {
        "release"
    }
}

fn to_list(ids: Vec<String>, idx: &HashMap<String, (usize, String, String)>, m: &Value) -> VersionList {
    let mut versions: Vec<McVersion> = ids
        .into_iter()
        .map(|id| match idx.get(&id) {
            Some((_, kind, time)) => McVersion { kind: kind.clone(), release_time: time.clone(), id },
            None => McVersion { kind: guess_kind(&id).into(), release_time: String::new(), id },
        })
        .collect();
    versions.sort_by_key(|v| idx.get(&v.id).map(|x| x.0).unwrap_or(usize::MAX));
    versions.dedup_by(|a, b| a.id == b.id);
    VersionList { latest_release: s(&m["latest"]["release"]), latest_snapshot: s(&m["latest"]["snapshot"]), versions }
}

/// Minecraft versions a software can run, newest first.
pub async fn game_versions(state: &AppState, software: &str) -> AppResult<VersionList> {
    let (idx, m) = manifest_index(state).await?;
    let ids: Vec<String> = match software {
        "vanilla" => m["versions"].as_array().cloned().unwrap_or_default().iter().map(|v| s(&v["id"])).collect(),
        "paper" | "folia" => {
            let p = state.cached_json(&format!("fill-{software}"), &format!("{FILL}/{software}"), LIST_TTL).await?;
            p["versions"].as_object().map(|o| o.values().flat_map(|a| a.as_array().cloned().unwrap_or_default()).map(|v| s(&v)).collect()).unwrap_or_default()
        }
        "purpur" => {
            let p = state.cached_json("purpur", PURPUR, LIST_TTL).await?;
            p["versions"].as_array().cloned().unwrap_or_default().iter().map(s).collect()
        }
        "fabric" => {
            let p = state.cached_json("fabric-game", &format!("{FABRIC_META}/versions/game"), LIST_TTL).await?;
            p.as_array().cloned().unwrap_or_default().iter().map(|v| s(&v["version"])).collect()
        }
        "quilt" => {
            let p = state.cached_json("quilt-game", &format!("{QUILT_META}/versions/game"), LIST_TTL).await?;
            p.as_array().cloned().unwrap_or_default().iter().map(|v| s(&v["version"])).collect()
        }
        "forge" => {
            let p = state.cached_json("forge-meta", &format!("{FORGE_FILES}/maven-metadata.json"), LIST_TTL).await?;
            p.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
        }
        "neoforge" => {
            let p = state.cached_json("neoforge", NEOFORGE_API, LIST_TTL).await?;
            let mut seen = std::collections::HashSet::new();
            p["versions"].as_array().cloned().unwrap_or_default().iter().filter_map(|v| neoforge_mc(&s(v))).filter(|v| seen.insert(v.clone())).collect()
        }
        _ => return Err(AppError::msg(format!("Unknown server software “{software}”"))),
    };
    // Only versions Mojang actually published (drops Fabric's experimental / april-fools ids).
    let ids = if software == "vanilla" { ids } else { ids.into_iter().filter(|i| idx.contains_key(i)).collect() };
    Ok(to_list(ids, &idx, &m))
}

/// Minecraft version a NeoForge version targets: 21.1.200 -> 1.21.1, 21.0.x -> 1.21,
/// 26.1.2.5 -> 26.1.2, 26.3.0.76 -> 26.3 (year-based versions since 2026).
pub fn neoforge_mc(v: &str) -> Option<String> {
    let base = v.split('-').next()?;
    let parts: Vec<&str> = base.split('.').collect();
    let major: u32 = parts.first()?.parse().ok()?;
    if major == 0 || parts.len() < 3 {
        return None;
    }
    if major >= 25 {
        if parts.len() >= 4 && parts[2] != "0" {
            Some(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
        } else {
            Some(format!("{}.{}", parts[0], parts[1]))
        }
    } else if parts[1] == "0" {
        Some(format!("1.{major}"))
    } else {
        Some(format!("1.{major}.{}", parts[1]))
    }
}

/// Builds / loader versions for a Minecraft version, newest first.
pub async fn loader_versions(state: &AppState, software: &str, mc: &str) -> AppResult<Vec<LoaderVersion>> {
    let lv = |id: String, stable: bool, tag: &str| LoaderVersion { id, stable, tag: tag.to_string() };
    Ok(match software {
        "vanilla" => vec![],
        "paper" | "folia" => {
            let b = state.cached_json(&format!("fill-{software}-{mc}"), &format!("{FILL}/{software}/versions/{mc}/builds"), LIST_TTL).await?;
            let mut out: Vec<LoaderVersion> = b
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|x| {
                    let ch = s(&x["channel"]).to_lowercase();
                    let id = x["id"].as_i64().map(|n| n.to_string()).unwrap_or_else(|| s(&x["id"]));
                    LoaderVersion { id, stable: ch == "stable" || ch == "recommended", tag: if ch == "stable" { String::new() } else { ch } }
                })
                .collect();
            out.sort_by_key(|x| std::cmp::Reverse(x.id.parse::<i64>().unwrap_or(0)));
            out
        }
        "purpur" => {
            let b = state.cached_json(&format!("purpur-{mc}"), &format!("{PURPUR}/{mc}"), LIST_TTL).await?;
            let mut all: Vec<String> = b["builds"]["all"].as_array().cloned().unwrap_or_default().iter().map(s).collect();
            all.reverse();
            all.into_iter().map(|id| lv(id, true, "")).collect()
        }
        "fabric" => {
            let b = state.cached_json("fabric-loader", &format!("{FABRIC_META}/versions/loader"), LIST_TTL).await?;
            b.as_array().cloned().unwrap_or_default().iter().map(|x| lv(s(&x["version"]), x["stable"].as_bool().unwrap_or(false), if x["stable"].as_bool().unwrap_or(false) { "" } else { "beta" })).collect()
        }
        "quilt" => {
            let b = state.cached_json("quilt-loader", &format!("{QUILT_META}/versions/loader"), LIST_TTL).await?;
            b.as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|x| {
                    let id = s(&x["version"]);
                    let pre = id.contains("beta") || id.contains("pre") || id.contains("alpha");
                    lv(id, !pre, if pre { "beta" } else { "" })
                })
                .collect()
        }
        "forge" => {
            let meta = state.cached_json("forge-meta", &format!("{FORGE_FILES}/maven-metadata.json"), LIST_TTL).await?;
            let promos = state.cached_json("forge-promos", &format!("{FORGE_FILES}/promotions_slim.json"), LIST_TTL).await.unwrap_or(Value::Null);
            let rec = s(&promos["promos"][format!("{mc}-recommended")]);
            let latest = s(&promos["promos"][format!("{mc}-latest")]);
            let mut out: Vec<LoaderVersion> = meta[mc]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|v| {
                    let full = s(v);
                    let id = full.strip_prefix(&format!("{mc}-")).unwrap_or(&full).to_string();
                    let base = id.split('-').next().unwrap_or(&id).to_string();
                    let tag = if base == rec { "recommended" } else if base == latest { "latest" } else { "" };
                    lv(id, true, tag)
                })
                .collect();
            out.reverse();
            out
        }
        "neoforge" => {
            let p = state.cached_json("neoforge", NEOFORGE_API, LIST_TTL).await?;
            let mut out: Vec<LoaderVersion> = p["versions"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(s)
                .filter(|v| neoforge_mc(v).as_deref() == Some(mc))
                .map(|id| {
                    let beta = id.contains("beta") || id.contains("alpha");
                    lv(id, !beta, if beta { "beta" } else { "" })
                })
                .collect();
            out.reverse();
            out
        }
        _ => return Err(AppError::msg(format!("Unknown server software “{software}”"))),
    })
}

/// The build/loader to install when the profile leaves it empty: the recommended or newest stable.
pub async fn default_loader(state: &AppState, software: &str, mc: &str) -> AppResult<String> {
    let list = loader_versions(state, software, mc).await?;
    if software == "vanilla" {
        return Ok(String::new());
    }
    list.iter()
        .find(|l| l.tag == "recommended")
        .or_else(|| list.iter().find(|l| l.stable))
        .or_else(|| list.first())
        .map(|l| l.id.clone())
        .ok_or_else(|| AppError::msg(format!("No {} build is available for Minecraft {mc} yet.", crate::models::software_label(software))))
}

/// Per-version metadata from Mojang (server jar + required Java). Immutable, so cached for a week.
pub async fn version_meta(state: &AppState, mc: &str) -> AppResult<VersionMeta> {
    let (idx, m) = manifest_index(state).await?;
    if !idx.contains_key(mc) {
        return Err(AppError::msg(format!("Minecraft {mc} isn't in Mojang's version list.")));
    }
    let url = m["versions"].as_array().and_then(|a| a.iter().find(|v| v["id"] == mc)).map(|v| s(&v["url"])).unwrap_or_default();
    let j = state.cached_json(&format!("mojang-v-{mc}"), &url, META_TTL).await?;
    let server = &j["downloads"]["server"];
    Ok(VersionMeta {
        id: mc.to_string(),
        server_url: server["url"].as_str().map(String::from),
        sha1: server["sha1"].as_str().map(String::from),
        size: server["size"].as_u64().unwrap_or(0),
        java_major: j["javaVersion"]["majorVersion"].as_u64().map(|v| v as u32).unwrap_or_else(|| java_guess(mc)),
    })
}

/// Fallback when Mojang's metadata is unreachable.
pub fn java_guess(mc: &str) -> u32 {
    let parts: Vec<u32> = mc.split(['.', '-']).filter_map(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [y, ..] if *y >= 26 => 25,
        [1, m, p, ..] if *m == 20 && *p >= 5 => 21,
        [1, m, ..] if *m >= 21 => 21,
        [1, m, ..] if *m >= 18 => 17,
        [1, 17, ..] => 16,
        _ => 8,
    }
}

/// Mojang's official server jar for a version.
pub async fn vanilla_artifact(state: &AppState, mc: &str) -> AppResult<Artifact> {
    let meta = version_meta(state, mc).await?;
    let url = meta.server_url.ok_or_else(|| AppError::msg(format!("Mojang never published a server for Minecraft {mc}.")))?;
    Ok(Artifact { url, file_name: "server.jar".into(), sha1: meta.sha1, sha256: None })
}

/// The download for Paper / Folia / Purpur / Fabric builds (installer-based software is handled
/// in `install.rs`).
pub async fn direct_artifact(state: &AppState, software: &str, mc: &str, build: &str) -> AppResult<Artifact> {
    match software {
        "vanilla" => vanilla_artifact(state, mc).await,
        "paper" | "folia" => {
            let b = state.cached_json(&format!("fill-{software}-{mc}"), &format!("{FILL}/{software}/versions/{mc}/builds"), LIST_TTL).await?;
            let item = b
                .as_array()
                .and_then(|a| a.iter().find(|x| x["id"].as_i64().map(|n| n.to_string()).unwrap_or_else(|| s(&x["id"])) == build))
                .cloned()
                .ok_or_else(|| AppError::msg(format!("Build {build} of {} {mc} wasn't found.", crate::models::software_label(software))))?;
            let d = &item["downloads"]["server:default"];
            Ok(Artifact {
                url: s(&d["url"]),
                file_name: if d["name"].is_string() { s(&d["name"]) } else { format!("{software}-{mc}-{build}.jar") },
                sha1: None,
                sha256: d["checksums"]["sha256"].as_str().map(String::from),
            })
        }
        "purpur" => Ok(Artifact { url: format!("{PURPUR}/{mc}/{build}/download"), file_name: format!("purpur-{mc}-{build}.jar"), sha1: None, sha256: None }),
        "fabric" => {
            let inst = state.cached_json("fabric-installer", &format!("{FABRIC_META}/versions/installer"), LIST_TTL).await?;
            let installer = inst
                .as_array()
                .and_then(|a| a.iter().find(|x| x["stable"].as_bool() == Some(true)).or_else(|| a.first()))
                .map(|x| s(&x["version"]))
                .ok_or_else(|| AppError::msg("Fabric's installer list is empty."))?;
            Ok(Artifact {
                url: format!("{FABRIC_META}/versions/loader/{mc}/{build}/{installer}/server/jar"),
                file_name: "fabric-server-launch.jar".into(),
                sha1: None,
                sha256: None,
            })
        }
        _ => Err(AppError::msg(format!("{software} is installed with its installer"))),
    }
}

pub fn forge_installer_url(mc: &str, ver: &str) -> String {
    let full = format!("{mc}-{ver}");
    format!("{FORGE_MAVEN}/{full}/forge-{full}-installer.jar")
}

pub fn neoforge_installer_url(ver: &str) -> String {
    format!("{NEOFORGE_MAVEN}/{ver}/neoforge-{ver}-installer.jar")
}

/// Newest Quilt installer from Quilt's maven.
pub async fn quilt_installer_url(state: &AppState) -> AppResult<String> {
    let base = "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer";
    let xml = state.http.get(format!("{base}/maven-metadata.xml")).timeout(std::time::Duration::from_secs(20)).send().await?.error_for_status()?.text().await?;
    let v = regex::Regex::new(r"<release>([^<]+)</release>")
        .ok()
        .and_then(|r| r.captures(&xml).map(|c| c[1].to_string()))
        .ok_or_else(|| AppError::msg("Couldn't read Quilt's installer version."))?;
    Ok(format!("{base}/{v}/quilt-installer-{v}.jar"))
}

/// Compare two Minecraft versions by Mojang's release order: Some(true) when `a` is newer.
pub async fn is_newer(state: &AppState, a: &str, b: &str) -> Option<bool> {
    let (idx, _) = manifest_index(state).await.ok()?;
    Some(idx.get(a)?.0 < idx.get(b)?.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_neoforge_versions() {
        assert_eq!(neoforge_mc("21.1.200").as_deref(), Some("1.21.1"));
        assert_eq!(neoforge_mc("21.0.167").as_deref(), Some("1.21"));
        assert_eq!(neoforge_mc("20.4.237").as_deref(), Some("1.20.4"));
        assert_eq!(neoforge_mc("20.2.86-beta").as_deref(), Some("1.20.2"));
        assert_eq!(neoforge_mc("26.3.0.76").as_deref(), Some("26.3"));
        assert_eq!(neoforge_mc("26.1.2.5-beta").as_deref(), Some("26.1.2"));
        assert_eq!(neoforge_mc("0.25w14craftmine.3-beta"), None);
    }
    #[test]
    fn guesses_java() {
        assert_eq!(java_guess("26.3"), 25);
        assert_eq!(java_guess("1.21.8"), 21);
        assert_eq!(java_guess("1.20.5"), 21);
        assert_eq!(java_guess("1.20.4"), 17);
        assert_eq!(java_guess("1.18.2"), 17);
        assert_eq!(java_guess("1.12.2"), 8);
        assert_eq!(guess_kind("1.21.9-pre4"), "snapshot");
        assert_eq!(guess_kind("1.21.9"), "release");
    }
}
