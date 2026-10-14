//! Windows Firewall rules, IP addresses, port suggestions.

use super::proc;
use crate::error::AppResult;
use crate::models::*;
use serde::Serialize;
use std::path::Path;

pub fn rule_name(inst: &ServerInstance) -> String {
    format!("Minecraft Server ({})", &inst.id[..8.min(inst.id.len())])
}

fn join(ports: &[u16]) -> String {
    ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
}

fn split_ports(inst: &ServerInstance) -> (Vec<u16>, Vec<u16>) {
    let mut tcp = vec![];
    let mut udp = vec![];
    for (p, proto, _) in inst.public_ports() {
        let list = if proto == "tcp" { &mut tcp } else { &mut udp };
        if !list.contains(&p) {
            list.push(p);
        }
    }
    (tcp, udp)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirewallState {
    pub rule_exists: bool,
    pub ports_ok: bool,
    /// "TCP 25565, UDP 19132"
    pub rule_ports: String,
    /// An enabled inbound Allow rule for the server's java.exe exists.
    pub program_rule: bool,
    /// Enabled inbound Block rules for java.exe – Windows creates these when its "allow access?"
    /// prompt is dismissed, and Block always beats Allow.
    pub blocked_by: Vec<String>,
    pub java: Option<String>,
}

fn ps_quote(s: &str) -> String {
    s.replace('\'', "''")
}

/// Inspect inbound rules without elevation.
pub async fn firewall_state(inst: &ServerInstance, java: Option<&Path>) -> FirewallState {
    let name = ps_quote(&rule_name(inst));
    let prog = java.map(|j| ps_quote(&j.display().to_string()));
    let mut script = format!(
        "$r = Get-NetFirewallRule -DisplayName '{name}' -ErrorAction SilentlyContinue | Where-Object {{ $_.Enabled -eq 'True' -and $_.Direction -eq 'Inbound' -and $_.Action -eq 'Allow' }}; \
         if ($r) {{ $r | Get-NetFirewallPortFilter | ForEach-Object {{ 'PORT:' + $_.Protocol + ':' + ($_.LocalPort -join ',') }} }} else {{ 'PORT:NONE' }};"
    );
    if let Some(p) = &prog {
        script.push_str(&format!(
            " Get-NetFirewallApplicationFilter -Program '{p}' -ErrorAction SilentlyContinue | Get-NetFirewallRule | Where-Object {{ $_.Enabled -eq 'True' -and $_.Direction -eq 'Inbound' }} | ForEach-Object {{ $_.Action.ToString() + ':' + $_.DisplayName }}"
        ));
    }
    let out = proc::powershell(&script).await.map(|(_, o)| o).unwrap_or_default();
    let mut tcp_ok: Vec<String> = vec![];
    let mut udp_ok: Vec<String> = vec![];
    for l in out.lines().filter_map(|l| l.trim().strip_prefix("PORT:")) {
        if l == "NONE" {
            continue;
        }
        let mut parts = l.splitn(2, ':');
        let proto = parts.next().unwrap_or("").to_uppercase();
        let ports: Vec<String> = parts.next().unwrap_or("").split([',', ' ']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        if proto == "TCP" {
            tcp_ok.extend(ports);
        } else if proto == "UDP" {
            udp_ok.extend(ports);
        }
    }
    let (tcp, udp) = split_ports(inst);
    let covered = |list: &[String], p: u16| list.iter().any(|l| l == &p.to_string() || l == "Any");
    let exists = !tcp_ok.is_empty() || !udp_ok.is_empty();
    let ports_ok = exists && tcp.iter().all(|p| covered(&tcp_ok, *p)) && udp.iter().all(|p| covered(&udp_ok, *p));
    let mut parts = vec![];
    if !tcp_ok.is_empty() {
        parts.push(format!("TCP {}", tcp_ok.join(",")));
    }
    if !udp_ok.is_empty() {
        parts.push(format!("UDP {}", udp_ok.join(",")));
    }
    let program_rule = out.lines().any(|l| l.trim().starts_with("Allow:"));
    let mut blocked_by: Vec<String> = out.lines().filter_map(|l| l.trim().strip_prefix("Block:").map(|n| n.to_string())).collect();
    blocked_by.dedup();
    FirewallState { rule_exists: exists, ports_ok, rule_ports: parts.join(", "), program_rule, blocked_by, java: java.map(|j| j.display().to_string()) }
}

/// Create (or replace) inbound rules for this server's ports (+ its java.exe) and remove Block
/// rules Windows added for that java.exe – one UAC prompt.
pub async fn add_firewall_rules(inst: &ServerInstance, java: Option<&Path>, work_dir: &Path) -> AppResult<String> {
    let name = ps_quote(&rule_name(inst));
    let (tcp, udp) = split_ports(inst);
    let mut script = String::new();
    if let Some(j) = java {
        let p = ps_quote(&j.display().to_string());
        script.push_str(&format!("Get-NetFirewallApplicationFilter -Program '{p}' -ErrorAction SilentlyContinue | Get-NetFirewallRule | Where-Object {{ $_.Action -eq 'Block' }} | Remove-NetFirewallRule -ErrorAction SilentlyContinue\r\n"));
    }
    script.push_str(&format!("Remove-NetFirewallRule -DisplayName '{name}' -ErrorAction SilentlyContinue\r\n"));
    if !tcp.is_empty() {
        script.push_str(&format!("New-NetFirewallRule -DisplayName '{name}' -Direction Inbound -Protocol TCP -LocalPort {} -Action Allow -Profile Any | Out-Null\r\n", join(&tcp)));
    }
    if !udp.is_empty() {
        script.push_str(&format!("New-NetFirewallRule -DisplayName '{name}' -Direction Inbound -Protocol UDP -LocalPort {} -Action Allow -Profile Any | Out-Null\r\n", join(&udp)));
    }
    if let Some(j) = java {
        let p = ps_quote(&j.display().to_string());
        script.push_str(&format!("New-NetFirewallRule -DisplayName '{name}' -Direction Inbound -Program '{p}' -Action Allow -Profile Any | Out-Null\r\n"));
    }
    script.push_str("Write-Output 'RULES_OK'");
    proc::powershell_elevated(&script, work_dir).await
}

pub async fn remove_firewall_rules(inst: &ServerInstance, work_dir: &Path) -> AppResult<()> {
    let name = ps_quote(&rule_name(inst));
    proc::powershell_elevated(&format!("Remove-NetFirewallRule -DisplayName '{name}' -ErrorAction SilentlyContinue"), work_dir).await?;
    Ok(())
}

pub async fn public_ip(http: &reqwest::Client) -> Option<String> {
    for url in ["https://api.ipify.org", "https://ifconfig.me/ip", "https://icanhazip.com"] {
        if let Ok(r) = http.get(url).timeout(std::time::Duration::from_secs(5)).send().await {
            if let Ok(t) = r.text().await {
                let t = t.trim().to_string();
                if t.parse::<std::net::IpAddr>().is_ok() {
                    return Some(t);
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalIp {
    pub interface: String,
    pub ip: String,
}

pub fn local_ips() -> Vec<LocalIp> {
    let nets = sysinfo::Networks::new_with_refreshed_list();
    let mut out = vec![];
    for (name, data) in nets.iter() {
        for ip in data.ip_networks() {
            if let std::net::IpAddr::V4(v4) = ip.addr {
                if !v4.is_loopback() && !v4.is_link_local() {
                    out.push(LocalIp { interface: name.clone(), ip: v4.to_string() });
                }
            }
        }
    }
    // Real adapters first: virtual ones (WSL, Hyper-V, VPNs) are rarely what the router forwards to.
    let virt = |n: &str| {
        let l = n.to_lowercase();
        l.contains("vethernet") || l.contains("wsl") || l.contains("virtual") || l.contains("vmware") || l.contains("vbox") || l.contains("tailscale") || l.contains("zerotier") || l.contains("hamachi")
    };
    out.sort_by(|a, b| virt(&a.interface).cmp(&virt(&b.interface)).then(a.ip.cmp(&b.ip)));
    out
}

/// Next free game port (25565, 25566, …) with its RCON port (+10) – unused by other profiles and
/// by other programs.
pub async fn suggest_ports(instances: &[ServerInstance], exclude_id: Option<&str>) -> (u16, u16) {
    let (tcp, _) = proc::port_owners().await;
    let used: Vec<u16> = instances.iter().filter(|i| Some(i.id.as_str()) != exclude_id).flat_map(|i| i.tcp_ports()).collect();
    let free = |p: u16| !used.contains(&p) && !tcp.contains_key(&p);
    let port = (25565u16..25700).find(|p| free(*p) && free(p + 10)).unwrap_or(25565);
    (port, port + 10)
}

/// The router this PC sends internet traffic to (IPv4 default route), from `route print`.
pub async fn default_gateway() -> Option<String> {
    let (_, out) = proc::capture("route.exe", &["print", "-4", "0.0.0.0"]).await.ok()?;
    out.lines().find_map(|l| {
        let c: Vec<&str> = l.split_whitespace().collect();
        (c.len() >= 5 && c[0] == "0.0.0.0" && c[1] == "0.0.0.0" && c[2].parse::<std::net::Ipv4Addr>().is_ok()).then(|| c[2].to_string())
    })
}

/// Ask a public status service to ping the server from the internet (the only reliable test of
/// port forwarding – a ping from inside the LAN often fails on routers without NAT loopback).
pub async fn external_check(http: &reqwest::Client, host: &str, port: u16) -> AppResult<serde_json::Value> {
    let url = format!("https://api.mcsrvstat.us/3/{}:{port}", urlencoding::encode(host));
    let r = http.get(url).header("User-Agent", concat!("Minecraft-Server-Manager/", env!("CARGO_PKG_VERSION"))).timeout(std::time::Duration::from_secs(25)).send().await?;
    Ok(r.error_for_status()?.json().await?)
}
