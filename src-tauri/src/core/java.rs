//! Java runtimes: detection of installed JDKs/JREs (including the ones the Minecraft launcher
//! ships), and managed Eclipse Temurin runtimes downloaded from Adoptium on demand.

use super::download;
use crate::error::{AppError, AppResult};
use crate::state::{AppState, TaskReporter};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntime {
    /// java.exe
    pub path: PathBuf,
    pub major: u32,
    pub version: String,
    pub vendor: String,
    /// Downloaded and owned by the manager.
    pub managed: bool,
    /// managed | system | minecraft-launcher | custom
    pub source: String,
}

/// "1.8.0_392" -> 8, "17.0.2" -> 17, "21" -> 21
pub fn parse_major(v: &str) -> Option<u32> {
    let v = v.trim().trim_matches('"');
    let mut parts = v.split(['.', '_', '+', '-']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Read a Java home's `release` file.
fn from_home(home: &Path, managed: bool, source: &str) -> Option<JavaRuntime> {
    let exe = home.join("bin").join(if cfg!(windows) { "java.exe" } else { "java" });
    if !exe.exists() {
        return None;
    }
    let release = std::fs::read_to_string(home.join("release")).unwrap_or_default();
    let get = |k: &str| release.lines().find_map(|l| l.strip_prefix(&format!("{k}=")).map(|v| v.trim().trim_matches('"').to_string()));
    let version = get("JAVA_VERSION")?;
    Some(JavaRuntime {
        path: exe,
        major: parse_major(&version)?,
        version,
        vendor: get("IMPLEMENTOR").unwrap_or_default(),
        managed,
        source: source.into(),
    })
}

fn homes_under(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

/// Every Java runtime found on this PC, newest major first.
pub fn detect(managed_dir: &Path) -> Vec<JavaRuntime> {
    let mut out: Vec<JavaRuntime> = vec![];
    let mut push = |r: Option<JavaRuntime>| {
        if let Some(r) = r {
            if !out.iter().any(|o| super::proc::norm(&o.path) == super::proc::norm(&r.path)) {
                out.push(r);
            }
        }
    };
    // Managed: <java dir>/temurin-21/<jdk-21.0.x-jre>/
    for d in homes_under(managed_dir) {
        push(from_home(&d, true, "managed"));
        for inner in homes_under(&d) {
            push(from_home(&inner, true, "managed"));
        }
    }
    if let Ok(h) = std::env::var("JAVA_HOME") {
        push(from_home(Path::new(&h), false, "system"));
    }
    if let Ok(path) = std::env::var("PATH") {
        for p in std::env::split_paths(&path) {
            if p.join("java.exe").exists() || p.join("java").exists() {
                if let Some(home) = p.parent() {
                    push(from_home(home, false, "system"));
                }
            }
        }
    }
    let mut roots: Vec<PathBuf> = vec![];
    for pf in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Ok(p) = std::env::var(pf) {
            for v in ["Java", "Eclipse Adoptium", "Eclipse Foundation", "Microsoft", "Zulu", "BellSoft", "Amazon Corretto", "Semeru", "OpenJDK", "AdoptOpenJDK", "GraalVM"] {
                roots.push(Path::new(&p).join(v));
            }
        }
    }
    if let Ok(up) = std::env::var("USERPROFILE") {
        roots.push(Path::new(&up).join(".jdks"));
    }
    for r in roots {
        for h in homes_under(&r) {
            push(from_home(&h, false, "system"));
        }
    }
    // Runtimes the official Minecraft launcher downloads: runtime/<component>/windows-x64/<component>/
    let mut launcher_roots = vec![];
    if let Ok(a) = std::env::var("APPDATA") {
        launcher_roots.push(Path::new(&a).join(".minecraft").join("runtime"));
    }
    if let Ok(l) = std::env::var("LOCALAPPDATA") {
        launcher_roots.push(Path::new(&l).join("Packages").join("Microsoft.4297127D64EC6_8wekyb3d8bbwe").join("LocalCache").join("Local").join("runtime"));
    }
    for r in launcher_roots {
        for comp in homes_under(&r) {
            for plat in homes_under(&comp) {
                for home in homes_under(&plat) {
                    push(from_home(&home, false, "minecraft-launcher"));
                }
            }
        }
    }
    out.sort_by(|a, b| b.major.cmp(&a.major).then(a.managed.cmp(&b.managed).reverse()));
    out
}

/// `java -version` of an explicit executable.
pub async fn probe_exe(path: &Path) -> AppResult<JavaRuntime> {
    if !path.exists() {
        return Err(AppError::msg(format!("{} doesn't exist.", path.display())));
    }
    if let Some(home) = path.parent().and_then(|b| b.parent()) {
        if let Some(mut r) = from_home(home, false, "custom") {
            r.path = path.to_path_buf();
            return Ok(r);
        }
    }
    let (_, out) = super::proc::capture(&path.to_string_lossy(), &["-version"]).await?;
    let re = regex::Regex::new(r#"version "([^"]+)""#).unwrap();
    let version = re.captures(&out).map(|c| c[1].to_string()).ok_or_else(|| AppError::msg(format!("{} didn't report a Java version.", path.display())))?;
    Ok(JavaRuntime {
        path: path.to_path_buf(),
        major: parse_major(&version).unwrap_or(0),
        vendor: out.lines().nth(1).unwrap_or_default().trim().to_string(),
        version,
        managed: false,
        source: "custom".into(),
    })
}

/// Pick a runtime for a server that needs Java `required`: exactly that major (mods such as
/// Cobblemon refuse anything else, and old Forge breaks on newer Java). Only requirements with no
/// LTS build of their own (Java 16 for 1.17) take the closest newer runtime.
pub fn pick(runtimes: &[JavaRuntime], required: u32) -> Option<&JavaRuntime> {
    let exact = runtimes.iter().filter(|r| r.major == required).max_by_key(|r| (r.managed, r.version.clone()));
    if exact.is_some() || feature_for(required) == required {
        return exact;
    }
    runtimes.iter().filter(|r| r.major > required).min_by_key(|r| r.major)
}

/// The Temurin feature release to download for a requirement (LTS at or above it).
pub fn feature_for(required: u32) -> u32 {
    [8, 11, 17, 21, 25].into_iter().find(|f| *f >= required).unwrap_or(required)
}

/// Make sure a suitable Java exists, downloading Temurin if needed. Returns java.exe.
pub async fn ensure(state: &AppState, required: u32, reporter: Option<&TaskReporter>) -> AppResult<PathBuf> {
    let dir = state.java_dir().await;
    let d2 = dir.clone();
    let found = tokio::task::spawn_blocking(move || detect(&d2)).await.map_err(|e| AppError::msg(e.to_string()))?;
    if let Some(r) = pick(&found, required) {
        return Ok(r.path.clone());
    }
    let _lock = state.java_lock.lock().await;
    // Another task may have installed it while we waited.
    let d2 = dir.clone();
    let found = tokio::task::spawn_blocking(move || detect(&d2)).await.map_err(|e| AppError::msg(e.to_string()))?;
    if let Some(r) = pick(&found, required) {
        return Ok(r.path.clone());
    }
    install_temurin(state, feature_for(required), reporter).await
}

/// Download and unpack an Eclipse Temurin JRE (JDK when no JRE build exists).
pub async fn install_temurin(state: &AppState, feature: u32, reporter: Option<&TaskReporter>) -> AppResult<PathBuf> {
    let dir = state.java_dir().await;
    if let Some(r) = reporter {
        r.progress(&format!("Finding Java {feature} (Eclipse Temurin)…"), None);
    }
    let mut pkg = None;
    for image in ["jre", "jdk"] {
        let url = format!("https://api.adoptium.net/v3/assets/latest/{feature}/hotspot?architecture=x64&image_type={image}&os=windows&vendor=eclipse");
        let resp = state.http.get(&url).timeout(std::time::Duration::from_secs(25)).send().await?;
        if !resp.status().is_success() {
            continue;
        }
        let v: serde_json::Value = resp.json().await?;
        if let Some(p) = v.as_array().and_then(|a| a.first()).map(|x| x["binary"]["package"].clone()) {
            if p["link"].is_string() {
                pkg = Some(p);
                break;
            }
        }
    }
    let pkg = pkg.ok_or_else(|| AppError::msg(format!("Adoptium has no Java {feature} build for Windows x64.")))?;
    let link = pkg["link"].as_str().unwrap_or_default().to_string();
    let sha256 = pkg["checksum"].as_str().map(String::from);
    let tmp = dir.join(format!("_download-{}", uuid::Uuid::new_v4()));
    let zip = tmp.join("java.zip");
    download::download(&state.http, &link, &zip, reporter, &format!("Java {feature}")).await?;
    if let Some(h) = sha256 {
        if !download::verify_sha256(&zip, &h).await? {
            let _ = tokio::fs::remove_dir_all(&tmp).await;
            return Err(AppError::msg("The Java download was corrupted (checksum mismatch). Try again."));
        }
    }
    if let Some(r) = reporter {
        r.progress(&format!("Unpacking Java {feature}…"), None);
    }
    let unpack = tmp.join("x");
    download::extract_zip(&zip, &unpack).await?;
    let home = homes_under(&unpack).into_iter().next().ok_or_else(|| AppError::msg("The Java archive was empty."))?;
    let dest_root = dir.join(format!("temurin-{feature}"));
    if dest_root.exists() {
        let _ = tokio::fs::remove_dir_all(&dest_root).await;
    }
    tokio::fs::create_dir_all(&dest_root).await?;
    let dest = dest_root.join(home.file_name().unwrap_or_default());
    tokio::fs::rename(&home, &dest).await?;
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    let rt = from_home(&dest, true, "managed").ok_or_else(|| AppError::msg("The downloaded Java has no bin\\java.exe."))?;
    Ok(rt.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rt(major: u32, managed: bool) -> JavaRuntime {
        JavaRuntime { path: PathBuf::from(format!("C:/j{major}{managed}/bin/java.exe")), major, version: format!("{major}.0.1"), vendor: String::new(), managed, source: String::new() }
    }
    #[test]
    fn picks_runtimes() {
        assert_eq!(parse_major("1.8.0_392"), Some(8));
        assert_eq!(parse_major("\"21.0.5\""), Some(21));
        assert_eq!(parse_major("25"), Some(25));
        let list = vec![rt(25, false), rt(21, true), rt(17, false), rt(8, false)];
        assert_eq!(pick(&list, 21).unwrap().major, 21);
        assert_eq!(pick(&list, 16).unwrap().major, 17);
        assert_eq!(pick(&list, 8).unwrap().major, 8);
        assert_eq!(pick(&[rt(17, false)], 8), None);
        assert_eq!(pick(&[rt(17, false)], 21), None);
        // Java 21 servers never silently run on 25 – the exact LTS is downloaded instead.
        assert_eq!(pick(&[rt(25, true)], 21), None);
        assert_eq!(feature_for(16), 17);
        assert_eq!(feature_for(25), 25);
        assert_eq!(feature_for(8), 8);
    }
}
