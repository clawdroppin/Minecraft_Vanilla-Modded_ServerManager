//! Health checks for one server, each with an optional one-click fix the UI can trigger.

use super::{java, mods, network, proc};
use crate::models::*;
use crate::state::AppState;
use std::path::PathBuf;

fn check(id: &str, title: &str, level: &str, detail: impl Into<String>, fix: &str) -> DiagCheck {
    DiagCheck { id: id.into(), title: title.into(), level: level.into(), detail: detail.into(), fix: fix.into() }
}

/// The java.exe a server would use right now (no download).
pub async fn resolve_java(state: &AppState, inst: &ServerInstance) -> Option<PathBuf> {
    if !inst.java.path.trim().is_empty() {
        return Some(PathBuf::from(inst.java.path.trim()));
    }
    let required = inst.installed.as_ref().map(|i| i.java_major).filter(|m| *m > 0).unwrap_or(21);
    let dir = state.java_dir().await;
    let found = tokio::task::spawn_blocking(move || java::detect(&dir)).await.ok()?;
    java::pick(&found, required).map(|r| r.path.clone())
}

pub async fn run(state: &AppState, inst: &ServerInstance) -> Vec<DiagCheck> {
    let mut out = vec![];
    let installed = inst.is_installed();
    let st = inst.installed.clone().unwrap_or_default();

    // Software
    if installed {
        out.push(check(
            "install",
            "Server software",
            "ok",
            format!("{} {}{} – {}", software_label(&st.software), st.mc_version, if st.loader_version.is_empty() { String::new() } else { format!(" ({})", st.loader_version) }, st.target.path),
            "",
        ));
    } else {
        out.push(check("install", "Server software", "error", "Not installed yet – the server can't start.", "install"));
    }
    if inst.eula_accepted {
        out.push(check("eula", "Minecraft EULA", "ok", "Accepted – written to eula.txt on every start.", ""));
    } else {
        out.push(check("eula", "Minecraft EULA", "error", "Not accepted. Minecraft servers refuse to start until you agree to the EULA (aka.ms/MinecraftEULA).", "eula"));
    }

    // Java
    let required = if st.java_major > 0 { st.java_major } else { 21 };
    let custom = inst.java.path.trim();
    if !custom.is_empty() {
        match java::probe_exe(std::path::Path::new(custom)).await {
            Ok(r) if r.major < required || (required <= 8 && r.major > 8 && matches!(st.software.as_str(), "forge")) => out.push(check(
                "java",
                "Java",
                "error",
                format!("The selected Java is {} but Minecraft {} needs Java {required}.", r.version, st.mc_version),
                "java",
            )),
            Ok(r) => out.push(check("java", "Java", "ok", format!("Java {} ({}) – custom path.", r.version, r.vendor), "")),
            Err(e) => out.push(check("java", "Java", "error", e.to_string(), "java")),
        }
    } else {
        match resolve_java(state, inst).await {
            Some(p) => out.push(check("java", "Java", "ok", format!("Java {required}+ found: {}", p.display()), "")),
            None => out.push(check("java", "Java", "info", format!("No Java {required} on this PC yet – it's downloaded automatically (Eclipse Temurin) on the first start."), "java")),
        }
    }

    // Memory
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_mb = sys.total_memory() / 1_048_576;
    let xmx = inst.java.max_mem_mb as u64;
    let content = mods::list(inst);
    let n_mods = content.items.iter().filter(|i| i.enabled).count();
    let recommended: u64 = match (content.kind.as_str(), n_mods) {
        ("mods", n) if n > 200 => 10240,
        ("mods", n) if n > 100 => 8192,
        ("mods", n) if n > 30 => 6144,
        ("mods", _) => 4096,
        (_, _) => 3072,
    };
    if inst.java.min_mem_mb > inst.java.max_mem_mb {
        out.push(check("memory", "Memory", "error", "Minimum memory (-Xms) is above the maximum (-Xmx) – Java refuses to start.", "memory"));
    } else if total_mb > 0 && xmx > total_mb * 85 / 100 {
        out.push(check("memory", "Memory", "error", format!("{xmx} MB allocated but the PC only has {total_mb} MB – Windows will swap and the server will lag or crash."), "memory"));
    } else if xmx < recommended {
        out.push(check(
            "memory",
            "Memory",
            "warn",
            format!("{xmx} MB allocated. With {n_mods} {} about {recommended} MB is recommended.", if content.kind == "plugins" { "plugins" } else { "mods" }),
            "memory",
        ));
    } else {
        out.push(check("memory", "Memory", "ok", format!("{xmx} MB of {total_mb} MB system memory allocated."), ""));
    }

    // Ports
    let instances = state.instances.read().await.clone();
    let mine = inst.tcp_ports();
    let clash: Vec<String> = instances.iter().filter(|o| o.id != inst.id).filter(|o| o.tcp_ports().iter().any(|p| mine.contains(p))).map(|o| o.name.clone()).collect();
    if !clash.is_empty() {
        out.push(check("ports", "Port conflict", "error", format!("Port shared with: {}. Only one can run at a time.", clash.join(", ")), "ports"));
    }
    let pid = super::server::running_pid(inst);
    let (tcp, _) = proc::port_owners().await;
    let busy: Vec<(u16, u32)> = mine.iter().filter_map(|p| tcp.get(p).filter(|o| Some(**o) != pid).map(|o| (*p, *o))).collect();
    if let Some((port, owner)) = busy.first() {
        let mut sys = sysinfo::System::new();
        let name = proc::process_name(&mut sys, *owner).unwrap_or_default();
        out.push(check("portbusy", "Port in use", "error", format!("TCP {port} is held by {name} (PID {owner}). Pick another port."), "ports"));
    } else if clash.is_empty() {
        out.push(check("ports", "Ports", "ok", format!("TCP {} is free{}.", inst.network.port, if pid.is_some() { " (used by this server)" } else { "" }), ""));
    }
    if inst.network.rcon_enabled && inst.network.rcon_password.len() < 8 {
        out.push(check("rcon", "RCON password", "warn", "RCON gives full console access – use a long random password (the manager generates one).", "settings"));
    }

    // Firewall
    let java_path = resolve_java(state, inst).await;
    let fw = network::firewall_state(inst, java_path.as_deref()).await;
    if !fw.blocked_by.is_empty() {
        out.push(check(
            "firewall",
            "Windows Firewall is blocking Java",
            "error",
            format!("Inbound Block rule(s) “{}” exist for the server's java.exe – usually from dismissing Windows’ “allow access?” prompt. Block beats Allow, so players can’t connect.", fw.blocked_by.join("”, “")),
            "firewall",
        ));
    } else if fw.ports_ok || fw.program_rule {
        out.push(check("firewall", "Windows Firewall", "ok", "An inbound rule allows the server.", ""));
    } else {
        out.push(check("firewall", "Windows Firewall", "warn", "No inbound rule for this server – players outside this PC may be blocked. Also forward TCP on your router.", "firewall"));
    }

    // Security / gameplay settings
    let p = |k: &str| inst.properties.get(k).cloned().unwrap_or_default();
    if p("online-mode") == "false" {
        let enforce = p("white-list") == "true";
        out.push(check(
            "online",
            "Offline mode",
            if enforce { "warn" } else { "error" },
            if enforce {
                "online-mode is off: accounts aren't verified, so anyone can join with any name – the whitelist helps but names can be spoofed. Only use this behind a proxy (Velocity/BungeeCord) or on a LAN.".to_string()
            } else {
                "online-mode is off and there's no whitelist: anyone can join as any player, including your operators.".to_string()
            },
            "settings",
        ));
    }
    if let Ok(vd) = p("view-distance").parse::<u32>() {
        if vd > 16 {
            out.push(check("view", "View distance", "warn", format!("view-distance {vd} is heavy – 10–12 is plenty for most servers (simulation-distance matters even more for TPS)."), "settings"));
        }
    }

    // Mods
    let client_only: Vec<String> = content.items.iter().filter(|i| i.enabled && mods::client_only(i)).map(mods::display_name).collect();
    if !client_only.is_empty() {
        out.push(check(
            "clientmods",
            "Client-only mods",
            "error",
            format!("{} only work{} in the game client and can crash a dedicated server: {}. Disable or remove them.", client_only.len(), if client_only.len() == 1 { "s" } else { "" }, client_only.join(", ")),
            "mods",
        ));
    }
    let sw = inst.active_software();
    let wrong: Vec<String> = content
        .items
        .iter()
        .filter(|i| i.enabled)
        .filter(|i| match (sw, i.meta.loader.as_deref()) {
            ("fabric", Some(l)) => !matches!(l, "fabric"),
            ("quilt", Some(l)) => !matches!(l, "fabric" | "quilt"),
            ("forge", Some(l)) => !matches!(l, "forge"),
            ("neoforge", Some(l)) => !matches!(l, "neoforge" | "forge"),
            ("paper" | "purpur" | "folia", Some(l)) => !matches!(l, "bukkit" | "paper"),
            _ => false,
        })
        .map(mods::display_name)
        .collect();
    if !wrong.is_empty() {
        out.push(check(
            "loader",
            "Mods for another loader",
            "error",
            format!("Not made for {}: {}.", software_label(sw), wrong.join(", ")),
            "mods",
        ));
    }

    // Disk
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let dir = proc::norm(&inst.install_dir);
    if let Some(d) = disks.iter().filter(|d| dir.starts_with(&proc::norm(d.mount_point()))).max_by_key(|d| d.mount_point().as_os_str().len()) {
        let free = d.available_space() as f64 / 1_073_741_824.0;
        let lvl = if free < 3.0 { "error" } else if free < 10.0 { "warn" } else { "ok" };
        out.push(check("disk", "Disk space", lvl, format!("{free:.1} GB free on {}.", d.mount_point().display()), ""));
    }

    // Updates
    if let Some(u) = state.with_runtime(&inst.id, |r| r.update.clone()).await {
        out.push(check("update", "Update available", "warn", u.label, "update"));
    }

    // Crash reports from the last 24 h
    let crash_dir = inst.install_dir.join("crash-reports");
    if let Ok(rd) = std::fs::read_dir(&crash_dir) {
        let newest = rd.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).max_by_key(|(t, _)| *t);
        if let Some((t, path)) = newest {
            if t.elapsed().map(|d| d.as_secs() < 86_400).unwrap_or(false) {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let desc = text.lines().find_map(|l| l.strip_prefix("Description: ")).unwrap_or("").to_string();
                let cause = text.lines().skip_while(|l| !l.starts_with("Description:")).nth(2).unwrap_or("").trim().to_string();
                out.push(check("crash", "Recent crash report", "error", format!("{} – {desc}\n{cause}", path.file_name().unwrap_or_default().to_string_lossy()), "log"));
            }
        }
    }
    // Errors in the last console log
    if let Ok(text) = std::fs::read_to_string(inst.console_log()) {
        let bad: Vec<&str> = text
            .lines()
            .filter(|l| {
                l.contains("FAILED TO BIND TO PORT")
                    || l.contains("You need to agree to the EULA")
                    || l.contains("UnsupportedClassVersionError")
                    || l.contains("Could not reserve enough space")
                    || l.contains("OutOfMemoryError")
                    || l.contains("Mod resolution failed")
                    || l.contains("Incompatible mods found")
                    || l.contains("is missing required dependency")
                    || l.contains("requires version")
            })
            .take(4)
            .collect();
        if !bad.is_empty() {
            out.push(check("log", "Errors in the last start", "error", bad.join("\n"), "log"));
        }
    }
    out
}
