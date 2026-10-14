//! Persisted and event types. Everything is camelCase on the wire and mirrored in `src/lib/types.ts`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// JVM system property that tags every server the manager launches (`-Dmcsm.id=<profile id>`).
/// The supervisor finds and adopts servers by it, even after the manager restarts.
pub const MARKER_PROP: &str = "-Dmcsm.id=";
/// Manager-owned files inside a server folder (install record, console log, content index).
pub const META_DIR: &str = ".mcsm";

/// Server software the manager can install. Stored as the lowercase id.
pub const SOFTWARE: &[&str] = &["vanilla", "paper", "purpur", "folia", "fabric", "quilt", "forge", "neoforge"];

pub fn software_label(s: &str) -> &'static str {
    match s {
        "vanilla" => "Vanilla",
        "paper" => "Paper",
        "purpur" => "Purpur",
        "folia" => "Folia",
        "fabric" => "Fabric",
        "quilt" => "Quilt",
        "forge" => "Forge",
        "neoforge" => "NeoForge",
        _ => "Unknown",
    }
}

/// What kind of add-ons a software loads: `mods` (mods/), `plugins` (plugins/) or none.
pub fn content_kind(software: &str) -> &'static str {
    match software {
        "fabric" | "quilt" | "forge" | "neoforge" => "mods",
        "paper" | "purpur" | "folia" => "plugins",
        _ => "none",
    }
}

/// Modrinth / CurseForge loader names a server can use, best match first.
pub fn content_loaders(software: &str) -> Vec<&'static str> {
    match software {
        "fabric" => vec!["fabric"],
        // Quilt loads (most) Fabric mods too.
        "quilt" => vec!["quilt", "fabric"],
        "forge" => vec!["forge"],
        "neoforge" => vec!["neoforge"],
        "paper" => vec!["paper", "spigot", "bukkit"],
        "purpur" => vec!["purpur", "paper", "spigot", "bukkit"],
        "folia" => vec!["folia"],
        _ => vec![],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub default_install_root: Option<PathBuf>,
    pub backup_dir: Option<PathBuf>,
    pub java_dir: Option<PathBuf>,
    pub last_data_dir: Option<PathBuf>,
    /// Supervisor sample interval for live telemetry.
    pub telemetry_interval_ms: u64,
    pub confirm_destructive: bool,
    pub reduce_motion: bool,
    /// Watch Mojang's version manifest for new releases/snapshots (minutes, 0 = off).
    pub version_check_mins: u32,
    pub notify_snapshots: bool,
    /// Last versions the user was told about (so a new release is announced once).
    pub last_seen_release: String,
    pub last_seen_snapshot: String,
    pub accent: String,
    pub close_to_tray: bool,
    /// Discord/Slack-compatible webhook for server events. Empty = off.
    pub webhook_url: String,
    pub webhook_levels: Vec<String>,
    /// Optional CurseForge API key (console.curseforge.com). Empty = CurseForge search off.
    pub curseforge_key: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_install_root: None,
            backup_dir: None,
            java_dir: None,
            last_data_dir: None,
            telemetry_interval_ms: 1000,
            confirm_destructive: true,
            reduce_motion: false,
            version_check_mins: 30,
            notify_snapshots: false,
            last_seen_release: String::new(),
            last_seen_snapshot: String::new(),
            accent: "grass".into(),
            close_to_tray: true,
            webhook_url: String::new(),
            webhook_levels: vec!["success".into(), "error".into()],
            curseforge_key: String::new(),
        }
    }
}

/// How the installed server is launched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LaunchTarget {
    /// jar = `java -jar <path>`; args = `java @<path>` (modern Forge / NeoForge win_args.txt)
    pub kind: String,
    /// Relative to the server folder.
    pub path: String,
}

/// What is actually installed in the folder (written by the installer).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallState {
    pub software: String,
    pub mc_version: String,
    pub loader_version: String,
    pub target: LaunchTarget,
    /// Java feature version this Minecraft version needs (from Mojang's metadata).
    pub java_major: u32,
    pub installed_at: i64,
    /// Files the installer created that may be removed when switching versions.
    pub managed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct JavaSettings {
    /// Explicit java.exe. Empty = automatic (a managed Temurin runtime matching the version).
    pub path: String,
    pub min_mem_mb: u32,
    pub max_mem_mb: u32,
    /// aikar | g1 | zgc | none
    pub gc_preset: String,
    /// Extra JVM arguments (before -jar).
    pub jvm_args: String,
    /// Arguments after the jar (`nogui` keeps the Swing window away).
    pub server_args: String,
    /// normal | belowNormal | aboveNormal | high | idle
    pub priority: String,
    /// CPU cores the server may use. Empty = all cores.
    pub affinity: Vec<u32>,
}

impl Default for JavaSettings {
    fn default() -> Self {
        Self {
            path: String::new(),
            min_mem_mb: 2048,
            max_mem_mb: 4096,
            gc_preset: "aikar".into(),
            jvm_args: String::new(),
            server_args: "nogui".into(),
            priority: "normal".into(),
            affinity: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtraPort {
    pub port: u16,
    /// tcp | udp | both
    pub protocol: String,
    pub label: String,
}

impl Default for ExtraPort {
    fn default() -> Self {
        Self { port: 19132, protocol: "udp".into(), label: "Geyser (Bedrock)".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NetworkSettings {
    /// server-port (TCP)
    pub port: u16,
    /// server-ip – bind to one local address. Empty = all interfaces.
    pub bind_ip: String,
    /// The manager talks to the server over RCON (localhost) for commands, player lists and TPS.
    pub rcon_enabled: bool,
    pub rcon_port: u16,
    pub rcon_password: String,
    /// GameSpy4 query (UDP) – used by some server lists.
    pub query_enabled: bool,
    pub query_port: u16,
    /// Ask the router to forward the ports over UPnP on every start.
    pub upnp: bool,
    /// Plugin / mod ports (Geyser, BlueMap, Dynmap, voice chat…) – forwarded and allowed too.
    pub extra_ports: Vec<ExtraPort>,
    /// Address players use (domain or IP) – shown on the Overview; empty = detected public IP.
    pub public_address: String,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            port: 25565,
            bind_ip: String::new(),
            rcon_enabled: true,
            rcon_port: 25575,
            rcon_password: String::new(),
            query_enabled: false,
            query_port: 25565,
            upnp: false,
            extra_ports: vec![],
            public_address: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleEntry {
    pub enabled: bool,
    /// Local time, "HH:MM".
    pub time: String,
    /// 0 = Sunday … 6 = Saturday. Empty = every day.
    pub days: Vec<u8>,
    /// restart | backup | stop | start | command | say
    pub action: String,
    /// Console command / broadcast text for `command` and `say`.
    pub command: String,
}

impl Default for ScheduleEntry {
    fn default() -> Self {
        Self { enabled: true, time: "04:00".into(), days: vec![], action: "restart".into(), command: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AutomationSettings {
    pub auto_start: bool,
    pub auto_restart: bool,
    pub restart_delay_secs: u32,
    /// Crash-loop guard: stop auto-restarting after this many crashes within 15 minutes.
    pub crash_limit: u32,
    pub stop_timeout_secs: u32,
    pub schedules: Vec<ScheduleEntry>,
    /// In-game countdown before scheduled/automatic restarts and stops (seconds before, e.g. 300, 60, 10).
    pub restart_warnings: Vec<u32>,
    /// `{time}` is replaced with "5 minutes" etc.
    pub warning_message: String,
    pub backup_interval_mins: u32,
    pub backup_keep: u32,
    /// Skip interval backups when nobody played since the last one.
    pub backup_only_if_players: bool,
    pub backup_on_stop: bool,
    pub backup_before_update: bool,
    pub backup_mods: bool,
    /// Check for newer builds / loader versions of the installed Minecraft version (minutes, 0 = off).
    pub update_check_mins: u32,
    /// Install newer builds of the same Minecraft version automatically (Paper builds, loader patches).
    pub auto_update_builds: bool,
    /// Restart when the server's memory exceeds this many MB (0 = off).
    pub memory_limit_mb: u32,
    /// Restart when the server has been up this many hours (0 = off).
    pub max_uptime_hours: u32,
    /// Restart when no player has been online for this many minutes after someone played (0 = off).
    pub empty_restart_mins: u32,
}

impl Default for AutomationSettings {
    fn default() -> Self {
        Self {
            auto_start: false,
            auto_restart: true,
            restart_delay_secs: 10,
            crash_limit: 3,
            stop_timeout_secs: 120,
            schedules: vec![],
            restart_warnings: vec![300, 60, 10],
            warning_message: "Server restarting in {time}!".into(),
            backup_interval_mins: 60,
            backup_keep: 24,
            backup_only_if_players: true,
            backup_on_stop: true,
            backup_before_update: true,
            backup_mods: false,
            update_check_mins: 120,
            auto_update_builds: false,
            memory_limit_mb: 0,
            max_uptime_hours: 0,
            empty_restart_mins: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerInstance {
    pub id: String,
    pub name: String,
    pub color: String,
    pub install_dir: PathBuf,
    pub created_at: i64,
    pub notes: String,
    /// Wanted software / versions (what Install or Change version installs).
    pub software: String,
    pub mc_version: String,
    /// Paper/Purpur/Folia build, Fabric/Quilt loader, Forge/NeoForge version. Empty = latest.
    pub loader_version: String,
    /// What is installed right now (None = not installed).
    pub installed: Option<InstallState>,
    /// The user accepted the Minecraft EULA for this server (written to eula.txt on start).
    pub eula_accepted: bool,
    pub java: JavaSettings,
    pub network: NetworkSettings,
    /// server.properties values the profile owns (the whole file; network keys are overlaid).
    pub properties: BTreeMap<String, String>,
    pub automation: AutomationSettings,
    /// Modrinth modpack this server was created from (project id / version id), if any.
    pub modpack: Option<ModpackRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ModpackRef {
    pub source: String,
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub version_name: String,
    pub icon_url: String,
}

impl Default for ServerInstance {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Minecraft Server".into(),
            color: "grass".into(),
            install_dir: PathBuf::new(),
            created_at: 0,
            notes: String::new(),
            software: "paper".into(),
            mc_version: String::new(),
            loader_version: String::new(),
            installed: None,
            eula_accepted: false,
            java: JavaSettings::default(),
            network: NetworkSettings::default(),
            properties: BTreeMap::new(),
            automation: AutomationSettings::default(),
            modpack: None,
        }
    }
}

impl ServerInstance {
    pub fn meta_dir(&self) -> PathBuf {
        self.install_dir.join(META_DIR)
    }
    pub fn properties_path(&self) -> PathBuf {
        self.install_dir.join("server.properties")
    }
    pub fn console_log(&self) -> PathBuf {
        self.meta_dir().join("console.log")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.install_dir.join("logs")
    }
    /// Level name from the profile's properties (default "world").
    pub fn level_name(&self) -> String {
        self.properties.get("level-name").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "world".into())
    }
    pub fn world_dir(&self) -> PathBuf {
        self.install_dir.join(self.level_name())
    }
    /// mods/ or plugins/ (None for vanilla).
    pub fn content_dir(&self) -> Option<PathBuf> {
        let sw = self.installed.as_ref().map(|i| i.software.as_str()).unwrap_or(&self.software);
        match content_kind(sw) {
            "mods" => Some(self.install_dir.join("mods")),
            "plugins" => Some(self.install_dir.join("plugins")),
            _ => None,
        }
    }
    /// Software of the installed server (falls back to the wanted one).
    pub fn active_software(&self) -> &str {
        self.installed.as_ref().map(|i| i.software.as_str()).unwrap_or(&self.software)
    }
    pub fn active_mc_version(&self) -> &str {
        self.installed.as_ref().map(|i| i.mc_version.as_str()).unwrap_or(&self.mc_version)
    }
    pub fn launch_file(&self) -> Option<PathBuf> {
        self.installed.as_ref().filter(|i| !i.target.path.is_empty()).map(|i| self.install_dir.join(&i.target.path))
    }
    pub fn is_installed(&self) -> bool {
        self.launch_file().map(|p| p.exists()).unwrap_or(false)
    }

    /// Every port the server listens on that players need: (port, protocol, label).
    pub fn public_ports(&self) -> Vec<(u16, &'static str, String)> {
        let n = &self.network;
        let mut v = vec![(n.port, "tcp", "Minecraft".to_string())];
        if n.query_enabled {
            v.push((if n.query_port == 0 { n.port } else { n.query_port }, "udp", "Query".to_string()));
        }
        for e in &n.extra_ports {
            if e.port == 0 {
                continue;
            }
            match e.protocol.as_str() {
                "tcp" => v.push((e.port, "tcp", e.label.clone())),
                "both" => {
                    v.push((e.port, "tcp", e.label.clone()));
                    v.push((e.port, "udp", e.label.clone()));
                }
                _ => v.push((e.port, "udp", e.label.clone())),
            }
        }
        v
    }

    pub fn tcp_ports(&self) -> Vec<u16> {
        let mut v: Vec<u16> = self.public_ports().into_iter().filter(|p| p.1 == "tcp").map(|p| p.0).collect();
        if self.network.rcon_enabled {
            v.push(self.network.rcon_port);
        }
        v
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ServerState {
    NotInstalled,
    Stopped,
    Installing,
    Updating,
    Starting,
    Running,
    Stopping,
    Restarting,
    Crashed,
}

impl ServerState {
    pub fn is_live(self) -> bool {
        matches!(self, ServerState::Starting | ServerState::Running | ServerState::Stopping)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OnlinePlayer {
    pub name: String,
    pub uuid: Option<String>,
    pub ip: Option<String>,
    pub joined_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub id: String,
    pub state: ServerState,
    pub pid: Option<u32>,
    pub started_at: Option<i64>,
    pub ready_at: Option<i64>,
    /// Seconds the server reported for "Done (x.xxxs)!"
    pub boot_secs: Option<f64>,
    /// Version line from the log ("Starting minecraft server version …").
    pub running_version: Option<String>,
    pub crash_count: u32,
    pub crash_loop: bool,
    pub pending_restart: bool,
    pub players: Vec<OnlinePlayer>,
    pub max_players: Option<u32>,
    pub tps: Option<f64>,
    pub mspt: Option<f64>,
    pub next_schedule: Option<NextSchedule>,
    pub last_backup: Option<i64>,
    pub last_exit_code: Option<i64>,
    pub busy: Option<String>,
    /// Newer build / loader for the installed Minecraft version, or a newer release.
    pub update: Option<UpdateInfo>,
    /// The manager can send commands (stdin pipe or RCON).
    pub can_command: bool,
    /// Countdown to a pending restart/stop (ms timestamp) and what it is.
    pub countdown: Option<NextSchedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// build | release
    pub kind: String,
    pub current: String,
    pub latest: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NextSchedule {
    pub at: i64,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProcSample {
    pub id: String,
    pub cpu: f32,
    pub mem: u64,
    pub virt: u64,
    pub read_bps: u64,
    pub write_bps: u64,
    pub uptime: u64,
    pub players: u32,
    pub tps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemSample {
    pub cpu: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub rx_bps: u64,
    pub tx_bps: u64,
    pub cores: usize,
    pub per_core: Vec<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEvent {
    pub ts: i64,
    pub system: SystemSample,
    pub servers: Vec<ProcSample>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub task_id: String,
    pub instance_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub message: String,
    pub progress: Option<f64>,
    pub done: bool,
    pub error: Option<String>,
    /// Finished without anything worth a toast.
    pub silent: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogBatch {
    pub id: String,
    /// server | manager | install | rcon
    pub source: String,
    pub lines: Vec<String>,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEvent {
    pub id: String,
    /// info | success | warn | error
    pub level: String,
    pub message: String,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerEvent {
    pub id: String,
    pub name: String,
    /// join | leave | chat | death | advancement
    pub kind: String,
    pub text: String,
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub created_at: i64,
    pub label: String,
    pub kind: String,
    pub files: u32,
    pub mc_version: String,
    pub worlds: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub path: PathBuf,
    pub exists: bool,
    pub text: String,
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackManifest {
    pub format: u32,
    pub app_version: String,
    pub exported_at: i64,
    pub instance: ServerInstance,
    pub has_worlds: bool,
    pub has_configs: bool,
    pub has_mods: bool,
    pub includes_secrets: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagCheck {
    pub id: String,
    pub title: String,
    /// ok | warn | error | info
    pub level: String,
    pub detail: String,
    /// Fix the UI can trigger: firewall | install | java | eula | ports | memory | settings | mods | log | ""
    pub fix: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: PathBuf,
    pub portable: bool,
    pub cores: usize,
    pub total_memory: u64,
    pub os: String,
    pub hostname: String,
}
