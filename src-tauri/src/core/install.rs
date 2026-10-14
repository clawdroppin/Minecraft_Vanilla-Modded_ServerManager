//! Installs (or switches) the server software in a profile's folder.
//!
//! * Vanilla / Paper / Purpur / Folia: one jar, checksum-verified where the source publishes one.
//! * Fabric: Fabric's server launcher jar + Mojang's server.jar next to it (no first-run download).
//! * Quilt / Forge / NeoForge: their official installer, run headless with the right Java.
//!   Modern Forge/NeoForge launch through `@libraries/…/win_args.txt` (what their run.bat does).

use super::{download, java, versions};
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::state::{now_ms, AppState, TaskReporter};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};

/// The Java a profile uses: its explicit path, or an automatic runtime for `required`.
pub async fn java_for(state: &AppState, inst: &ServerInstance, required: u32, reporter: Option<&TaskReporter>) -> AppResult<PathBuf> {
    let custom = inst.java.path.trim();
    if !custom.is_empty() {
        let p = PathBuf::from(custom);
        if !p.exists() {
            return Err(AppError::msg(format!("The Java set for this server ({custom}) doesn't exist. Pick another under Java & Memory, or switch to automatic.")));
        }
        return Ok(p);
    }
    java::ensure(state, required, reporter).await
}

/// Run an installer jar and stream its output into the server's console.
async fn run_installer(java: &Path, args: &[String], cwd: &Path, reporter: &TaskReporter, label: &str) -> AppResult<()> {
    reporter.progress(&format!("Running the {label} installer…"), None);
    let mut child = super::proc::hidden_command(java)
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AppError::msg(format!("Couldn't run Java for the {label} installer: {e}")))?;
    let out = child.stdout.take();
    let err = child.stderr.take();
    let r1 = reporter.clone();
    let r2 = reporter.clone();
    let lbl = label.to_string();
    let t1 = tokio::spawn(async move {
        let mut last = String::new();
        if let Some(o) = out {
            let mut lines = BufReader::new(o).lines();
            let mut n = 0u32;
            while let Ok(Some(l)) = lines.next_line().await {
                n += 1;
                r1.log(&l);
                if n % 20 == 0 {
                    r1.progress(&format!("{lbl} installer: {}", l.chars().take(90).collect::<String>()), None);
                }
                last = l;
            }
        }
        last
    });
    let t2 = tokio::spawn(async move {
        let mut tail = vec![];
        if let Some(e) = err {
            let mut lines = BufReader::new(e).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                r2.log(&l);
                tail.push(l);
                if tail.len() > 12 {
                    tail.remove(0);
                }
            }
        }
        tail
    });
    let status = child.wait().await?;
    let last = t1.await.unwrap_or_default();
    let tail = t2.await.unwrap_or_default();
    if !status.success() {
        let why = tail.iter().rev().find(|l| !l.trim().is_empty()).cloned().unwrap_or(last);
        return Err(AppError::msg(format!("The {label} installer failed (exit {}): {}", status.code().unwrap_or(-1), why.trim())));
    }
    Ok(())
}

/// Forge/NeoForge: the args file the installer created (modern), or the runnable jar (legacy).
fn find_forge_target(dir: &Path, software: &str, mc: &str, ver: &str) -> Option<LaunchTarget> {
    let args_dir = match software {
        "neoforge" => dir.join("libraries").join("net").join("neoforged").join("neoforge").join(ver),
        _ => dir.join("libraries").join("net").join("minecraftforge").join("forge").join(format!("{mc}-{ver}")),
    };
    let args = args_dir.join(if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" });
    if args.exists() {
        let rel = args.strip_prefix(dir).ok()?.to_string_lossy().replace('\\', "/");
        return Some(LaunchTarget { kind: "args".into(), path: rel });
    }
    let candidates = [
        format!("forge-{mc}-{ver}.jar"),
        format!("forge-{mc}-{ver}-universal.jar"),
        format!("forge-{mc}-{ver}-{mc}-universal.jar"),
        format!("forge-{mc}-{ver}-shim.jar"),
        format!("minecraftforge-universal-{mc}-{ver}.jar"),
    ];
    for c in candidates {
        if dir.join(&c).exists() {
            return Some(LaunchTarget { kind: "jar".into(), path: c });
        }
    }
    // Last resort: any forge*.jar the installer left in the root (not the installer itself).
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).find(|n| {
        let l = n.to_lowercase();
        l.starts_with("forge") && l.ends_with(".jar") && !l.contains("installer")
    }).map(|n| LaunchTarget { kind: "jar".into(), path: n })
}

/// Install `software` / `mc` / `loader` (empty = recommended) into the profile's folder.
pub async fn install(state: &AppState, inst: &ServerInstance, reporter: &TaskReporter) -> AppResult<InstallState> {
    let software = inst.software.trim().to_lowercase();
    if !SOFTWARE.contains(&software.as_str()) {
        return Err(AppError::msg(format!("Unknown server software “{}”.", inst.software)));
    }
    let label = software_label(&software);
    let mc = if inst.mc_version.trim().is_empty() {
        let list = versions::game_versions(state, &software).await?;
        list.versions.iter().find(|v| v.kind == "release").map(|v| v.id.clone()).ok_or_else(|| AppError::msg(format!("No {label} release found.")))?
    } else {
        inst.mc_version.trim().to_string()
    };
    reporter.progress(&format!("Resolving {label} {mc}…"), None);
    let loader = if software == "vanilla" {
        String::new()
    } else if inst.loader_version.trim().is_empty() {
        versions::default_loader(state, &software, &mc).await?
    } else {
        inst.loader_version.trim().to_string()
    };
    let java_major = versions::version_meta(state, &mc).await.map(|m| m.java_major).unwrap_or_else(|_| versions::java_guess(&mc));
    let dir = inst.install_dir.clone();
    tokio::fs::create_dir_all(&dir).await?;
    let meta = dir.join(META_DIR);
    tokio::fs::create_dir_all(&meta).await?;
    let http = &state.http;

    let mut managed: Vec<String> = vec![];
    let target = match software.as_str() {
        "vanilla" | "paper" | "purpur" | "folia" => {
            let a = versions::direct_artifact(state, &software, &mc, &loader).await?;
            download::download_verified(http, &a.url, &dir.join(&a.file_name), a.sha1.as_deref(), a.sha256.as_deref(), Some(reporter), &format!("{label} {mc}")).await?;
            managed.push(a.file_name.clone());
            LaunchTarget { kind: "jar".into(), path: a.file_name }
        }
        "fabric" => {
            // Mojang's jar first, so the launcher doesn't download it on the first start.
            if let Ok(v) = versions::vanilla_artifact(state, &mc).await {
                download::download_verified(http, &v.url, &dir.join("server.jar"), v.sha1.as_deref(), None, Some(reporter), &format!("Minecraft {mc} server")).await?;
                managed.push("server.jar".into());
            }
            let a = versions::direct_artifact(state, "fabric", &mc, &loader).await?;
            download::download(http, &a.url, &dir.join(&a.file_name), Some(reporter), &format!("Fabric loader {loader}")).await?;
            std::fs::write(dir.join("fabric-server-launcher.properties"), "serverJar=server.jar\n")?;
            managed.push(a.file_name.clone());
            LaunchTarget { kind: "jar".into(), path: a.file_name }
        }
        "quilt" => {
            let java = java_for(state, inst, java_major, Some(reporter)).await?;
            let url = versions::quilt_installer_url(state).await?;
            let installer = meta.join("installers").join("quilt-installer.jar");
            download::download(http, &url, &installer, Some(reporter), "Quilt installer").await?;
            let args = vec![
                "-jar".to_string(),
                installer.to_string_lossy().into_owned(),
                "install".into(),
                "server".into(),
                mc.clone(),
                loader.clone(),
                "--download-server".into(),
                format!("--install-dir={}", dir.display()),
            ];
            run_installer(&java, &args, &dir, reporter, "Quilt").await?;
            let _ = tokio::fs::remove_file(&installer).await;
            if !dir.join("quilt-server-launch.jar").exists() {
                return Err(AppError::msg("The Quilt installer finished but quilt-server-launch.jar is missing."));
            }
            managed.push("quilt-server-launch.jar".into());
            LaunchTarget { kind: "jar".into(), path: "quilt-server-launch.jar".into() }
        }
        "forge" | "neoforge" => {
            let java = java_for(state, inst, java_major, Some(reporter)).await?;
            // Old Forge installers fetch the vanilla jar from long-gone URLs; hand it over locally.
            if software == "forge" && java_major <= 8 {
                if let Ok(v) = versions::vanilla_artifact(state, &mc).await {
                    let name = format!("minecraft_server.{mc}.jar");
                    if !dir.join(&name).exists() {
                        let _ = download::download_verified(http, &v.url, &dir.join(&name), v.sha1.as_deref(), None, Some(reporter), &format!("Minecraft {mc} server")).await;
                    }
                }
            }
            let url = if software == "forge" { versions::forge_installer_url(&mc, &loader) } else { versions::neoforge_installer_url(&loader) };
            let installer = meta.join("installers").join(format!("{software}-installer.jar"));
            download::download(http, &url, &installer, Some(reporter), &format!("{label} {loader} installer")).await?;
            let args = vec!["-jar".to_string(), installer.to_string_lossy().into_owned(), "--installServer".into()];
            run_installer(&java, &args, &dir, reporter, label).await?;
            let _ = tokio::fs::remove_file(&installer).await;
            for junk in ["installer.log"] {
                let p = dir.join(junk);
                if p.exists() {
                    let _ = std::fs::rename(&p, meta.join(format!("{software}-{junk}")));
                }
            }
            for e in std::fs::read_dir(&dir).map(|r| r.flatten().collect::<Vec<_>>()).unwrap_or_default() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.ends_with("-installer.jar.log") {
                    let _ = std::fs::rename(e.path(), meta.join(n));
                }
            }
            let t = find_forge_target(&dir, &software, &mc, &loader).ok_or_else(|| AppError::msg(format!("The {label} installer finished but no launch files were found.")))?;
            if t.kind == "jar" {
                managed.push(t.path.clone());
            }
            t
        }
        _ => unreachable!(),
    };

    // Switching versions: drop the previous software's own jars (worlds, configs and libraries stay).
    if let Some(old) = &inst.installed {
        for f in &old.managed_files {
            if !managed.contains(f) && f != "server.jar" {
                let p = dir.join(f);
                if p.is_file() && super::proc::is_within(&p, &dir) {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
    }
    let st = InstallState {
        software: software.clone(),
        mc_version: mc,
        loader_version: loader,
        target,
        java_major,
        installed_at: now_ms(),
        managed_files: managed,
    };
    let _ = crate::state::write_json_atomic(&meta.join("install.json"), &st);
    Ok(st)
}

/// Recover an install record from a folder (import of an existing server).
pub fn detect_existing(dir: &Path) -> Option<InstallState> {
    if let Some(st) = crate::state::read_json::<InstallState>(&dir.join(META_DIR).join("install.json")) {
        if dir.join(&st.target.path).exists() {
            return Some(st);
        }
    }
    let names: Vec<String> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let jar = |pred: &dyn Fn(&str) -> bool| names.iter().find(|n| n.to_lowercase().ends_with(".jar") && pred(&n.to_lowercase())).cloned();
    let mk = |software: &str, path: String, kind: &str| InstallState {
        software: software.into(),
        target: LaunchTarget { kind: kind.into(), path },
        ..Default::default()
    };
    // Modern Forge / NeoForge: the args files.
    for (sw, base) in [("neoforge", "libraries/net/neoforged/neoforge"), ("forge", "libraries/net/minecraftforge/forge")] {
        let root = dir.join(base);
        if let Ok(rd) = std::fs::read_dir(&root) {
            if let Some(v) = rd.flatten().filter(|e| e.path().join("win_args.txt").exists()).map(|e| e.file_name().to_string_lossy().into_owned()).max() {
                let mut st = mk(sw, format!("{base}/{v}/win_args.txt"), "args");
                if sw == "forge" {
                    if let Some((mc, l)) = v.split_once('-') {
                        st.mc_version = mc.into();
                        st.loader_version = l.into();
                    }
                } else {
                    st.mc_version = versions::neoforge_mc(&v).unwrap_or_default();
                    st.loader_version = v;
                }
                return Some(st);
            }
        }
    }
    if let Some(j) = jar(&|n| n == "fabric-server-launch.jar") {
        return Some(mk("fabric", j, "jar"));
    }
    if let Some(j) = jar(&|n| n == "quilt-server-launch.jar") {
        return Some(mk("quilt", j, "jar"));
    }
    let ver_re = regex::Regex::new(r"^(paper|purpur|folia)-(.+?)-(\d+)\.jar$").ok()?;
    for n in &names {
        if let Some(c) = ver_re.captures(&n.to_lowercase()) {
            let mut st = mk(&c[1], n.clone(), "jar");
            st.mc_version = c[2].to_string();
            st.loader_version = c[3].to_string();
            return Some(st);
        }
    }
    if let Some(j) = jar(&|n| n.starts_with("paper") || n.starts_with("purpur") || n.starts_with("folia")) {
        let sw = ["paper", "purpur", "folia"].into_iter().find(|s| j.to_lowercase().starts_with(s)).unwrap_or("paper");
        return Some(mk(sw, j, "jar"));
    }
    if let Some(j) = jar(&|n| n.starts_with("forge") && !n.contains("installer")) {
        return Some(mk("forge", j, "jar"));
    }
    if let Some(j) = jar(&|n| n == "server.jar" || n.starts_with("minecraft_server")) {
        return Some(mk("vanilla", j, "jar"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_existing_layouts() {
        let root = std::env::temp_dir().join(format!("mcsm-detect-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("paper-1.21.8-60.jar"), b"x").unwrap();
        let st = detect_existing(&root).unwrap();
        assert_eq!((st.software.as_str(), st.mc_version.as_str(), st.loader_version.as_str()), ("paper", "1.21.8", "60"));
        let nf = root.join("libraries/net/neoforged/neoforge/21.1.200");
        std::fs::create_dir_all(&nf).unwrap();
        std::fs::write(nf.join("win_args.txt"), b"x").unwrap();
        let st = detect_existing(&root).unwrap();
        assert_eq!((st.software.as_str(), st.mc_version.as_str(), st.target.kind.as_str()), ("neoforge", "1.21.1", "args"));
        let t = find_forge_target(&root, "neoforge", "1.21.1", "21.1.200").unwrap();
        assert_eq!(t.path, "libraries/net/neoforged/neoforge/21.1.200/win_args.txt");
        let _ = std::fs::remove_dir_all(root);
    }
}
