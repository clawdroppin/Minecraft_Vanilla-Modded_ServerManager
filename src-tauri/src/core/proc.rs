//! Process helpers: hidden spawning, the Java server launch (stdin pipe + console file),
//! marker-based server discovery, priority / affinity and port owners.

use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use crate::models::MARKER_PROP;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A tokio Command that never flashes a console window on Windows.
pub fn hidden_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut c = tokio::process::Command::new(program);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Run a program to completion and capture stdout+stderr as text.
pub async fn capture(program: &str, args: &[&str]) -> AppResult<(i32, String)> {
    let out = hidden_command(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| AppError::msg(format!("Failed to run {program}: {e}")))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.code().unwrap_or(-1), text))
}

pub async fn powershell(script: &str) -> AppResult<(i32, String)> {
    capture("powershell.exe", &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]).await
}

/// Run a PowerShell script elevated (one UAC prompt). Waits for completion and returns its output.
pub async fn powershell_elevated(script: &str, work_dir: &Path) -> AppResult<String> {
    tokio::fs::create_dir_all(work_dir).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let ps1 = work_dir.join(format!("elevated-{id}.ps1"));
    let log = work_dir.join(format!("elevated-{id}.log"));
    let wrapped = format!(
        "$ErrorActionPreference='Continue'\r\nStart-Transcript -Path '{}' | Out-Null\r\ntry {{\r\n{}\r\n}} catch {{ Write-Output (\"ERROR: \" + $_) }}\r\nStop-Transcript | Out-Null\r\n",
        log.display(),
        script
    );
    tokio::fs::write(&ps1, wrapped).await?;
    let launcher = format!(
        "try {{ $p = Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','\"{}\"'); exit $p.ExitCode }} catch {{ Write-Output 'UAC_DECLINED'; exit 1223 }}",
        ps1.display()
    );
    let (code, out) = powershell(&launcher).await?;
    let transcript = tokio::fs::read_to_string(&log).await.unwrap_or_default();
    let _ = tokio::fs::remove_file(&ps1).await;
    let _ = tokio::fs::remove_file(&log).await;
    if code == 1223 || out.contains("UAC_DECLINED") {
        return Err(AppError::msg("Administrator permission was declined"));
    }
    Ok(transcript)
}

pub fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// True when `path` is `dir` itself or lies inside it. Plain prefix matching would accept
/// `D:\srv\ab\…` as inside `D:\srv\a`.
pub fn is_within(path: &Path, dir: &Path) -> bool {
    // `D:\srv\mc\..\..\Windows` starts with `D:\srv\mc` but isn't inside it.
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }
    let (p, d) = (norm(path), norm(dir));
    !d.is_empty() && (p == d || p.starts_with(&format!("{d}\\")))
}

/// A running Java process the manager launched (tagged with `-Dmcsm.id=<profile id>`).
#[derive(Debug, Clone)]
pub struct JavaProc {
    pub pid: u32,
    pub id: String,
}

pub fn is_java(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(), "java.exe" | "javaw.exe" | "java" | "javaw")
}

/// Profile id from a server's command line.
pub fn marker_id<S: AsRef<OsStr>>(cmd: &[S]) -> Option<String> {
    cmd.iter().find_map(|a| a.as_ref().to_string_lossy().strip_prefix(MARKER_PROP).map(|s| s.trim_matches('"').to_string())).filter(|s| !s.is_empty())
}

/// Every manager-launched server on this PC – one scan serves all profiles. Only `java` processes
/// get their command line read, so the scan stays cheap.
pub fn scan_java(sys: &mut System) -> Vec<JavaProc> {
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let java: Vec<Pid> = sys.processes().iter().filter(|(_, p)| is_java(&p.name().to_string_lossy())).map(|(pid, _)| *pid).collect();
    if java.is_empty() {
        return vec![];
    }
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&java), false, ProcessRefreshKind::nothing().with_cmd(UpdateKind::OnlyIfNotSet));
    java.iter()
        .filter_map(|pid| {
            let p = sys.process(*pid)?;
            Some(JavaProc { pid: pid.as_u32(), id: marker_id(p.cmd())? })
        })
        .collect()
}

/// PID of a profile's running server (fresh scan).
pub fn find_server(id: &str) -> Option<u32> {
    let mut sys = System::new();
    scan_java(&mut sys).into_iter().find(|p| p.id == id).map(|p| p.pid)
}

pub fn pid_alive(sys: &mut System, pid: u32) -> bool {
    let p = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[p]), true, ProcessRefreshKind::nothing());
    sys.process(p).is_some()
}

pub async fn kill_tree(pid: u32) -> AppResult<()> {
    let pid_s = pid.to_string();
    let (code, out) = capture("taskkill.exe", &["/PID", &pid_s, "/T", "/F"]).await?;
    if code != 0 && !out.to_lowercase().contains("not found") {
        return Err(AppError::msg(format!("taskkill failed: {}", out.trim())));
    }
    Ok(())
}

/// Listening TCP ports and bound UDP ports -> owning PID, from one `netstat -ano` call.
pub async fn port_owners() -> (HashMap<u16, u32>, HashMap<u16, u32>) {
    let (mut tcp, mut udp) = (HashMap::new(), HashMap::new());
    let Ok((_, out)) = capture("netstat.exe", &["-ano"]).await else { return (tcp, udp) };
    parse_netstat(&out, &mut tcp, &mut udp);
    (tcp, udp)
}

fn parse_netstat(out: &str, tcp: &mut HashMap<u16, u32>, udp: &mut HashMap<u16, u32>) {
    for l in out.lines() {
        let cols: Vec<&str> = l.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        let Some(port) = cols[1].rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) else { continue };
        let Some(pid) = cols.last().and_then(|p| p.parse::<u32>().ok()) else { continue };
        if cols[0].eq_ignore_ascii_case("TCP") && cols.len() >= 5 && cols[3].eq_ignore_ascii_case("LISTENING") {
            tcp.entry(port).or_insert(pid);
        } else if cols[0].eq_ignore_ascii_case("UDP") {
            udp.entry(port).or_insert(pid);
        }
    }
}

pub fn process_name(sys: &mut System, pid: u32) -> Option<String> {
    let p = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[p]), false, ProcessRefreshKind::nothing());
    sys.process(p).map(|p| p.name().to_string_lossy().into_owned())
}

pub fn priority_class(name: &str) -> u32 {
    match name {
        "idle" => 0x40,
        "belowNormal" => 0x4000,
        "aboveNormal" => 0x8000,
        "high" => 0x80,
        _ => 0x20,
    }
}

/// Launch a Java server. stdout/stderr go to `console_log` (the supervisor tails it, also after a
/// manager restart) and stdin stays a pipe for console commands. The process is not tied to the
/// manager: closing the manager leaves the server running (commands then go over RCON).
pub fn spawn_java(java: &Path, args: &[String], cwd: &Path, console_log: &Path, priority: &str) -> AppResult<std::process::Child> {
    use std::process::{Command, Stdio};
    if let Some(p) = console_log.parent() {
        std::fs::create_dir_all(p)?;
    }
    // Keep the previous session's console next to the new one.
    if console_log.exists() {
        let _ = std::fs::rename(console_log, console_log.with_extension("prev.log"));
    }
    let out = std::fs::File::create(console_log)?;
    let err = out.try_clone()?;
    #[allow(unused_mut)]
    let mut attempts: Vec<u32> = vec![0];
    #[cfg(windows)]
    {
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let base = CREATE_NO_WINDOW | priority_class(priority);
        attempts = vec![base | CREATE_BREAKAWAY_FROM_JOB, base];
    }
    #[cfg(not(windows))]
    let _ = priority;
    let mut last_err = None;
    for flags in attempts {
        let mut c = Command::new(java);
        c.args(args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::from(out.try_clone()?)).stderr(Stdio::from(err.try_clone()?));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(flags);
        }
        #[cfg(not(windows))]
        let _ = flags;
        match c.spawn() {
            Ok(child) => return Ok(child),
            Err(e) => last_err = Some(e),
        }
    }
    Err(AppError::msg(format!("Failed to launch Java ({}): {}", java.display(), last_err.map(|e| e.to_string()).unwrap_or_default())))
}

/// Apply priority class and CPU affinity to a running process (works live).
#[cfg(windows)]
pub fn apply_priority_affinity(pid: u32, priority: &str, affinity: &[u32]) -> AppResult<()> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, SetPriorityClass, SetProcessAffinityMask, PROCESS_SET_INFORMATION, PROCESS_QUERY_INFORMATION};
    // SAFETY: handle is checked and closed; masks are plain integers.
    unsafe {
        let h = OpenProcess(PROCESS_SET_INFORMATION | PROCESS_QUERY_INFORMATION, 0, pid);
        if h.is_null() {
            return Err(AppError::msg(format!("Cannot open process {pid}: {}", std::io::Error::last_os_error())));
        }
        SetPriorityClass(h, priority_class(priority));
        if !affinity.is_empty() {
            let mask: usize = affinity.iter().filter(|c| **c < usize::BITS).fold(0usize, |m, c| m | (1usize << c));
            if mask != 0 {
                SetProcessAffinityMask(h, mask);
            }
        }
        CloseHandle(h);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn apply_priority_affinity(_pid: u32, _priority: &str, _affinity: &[u32]) -> AppResult<()> {
    Ok(())
}

/// Fast pre-check (no WMI): is any WebView2 process running whose parent has exited?
pub fn orphan_webview_suspected() -> bool {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes().values().any(|p| {
        if !p.name().to_string_lossy().eq_ignore_ascii_case("msedgewebview2.exe") {
            return false;
        }
        match p.parent().map(|pp| sys.process(pp)) {
            Some(None) => true,
            // PID reuse: a "parent" that started after its child is someone else.
            Some(Some(parent)) => parent.start_time() > p.start_time(),
            None => false,
        }
    })
}

/// WebView2 browser processes left behind by a previous manager run (hard kill / crash) keep the
/// browser profile locked and window creation fails with "The requested resource is in use".
/// The single-instance plugin guarantees no other manager is running, so any embedded WebView
/// tagged with our exe name whose parent process is gone is an orphan. Uses WMI because the
/// command line of WebView2 processes isn't readable otherwise. Returns how many were killed.
pub fn kill_orphaned_webviews() -> usize {
    let Ok(me) = std::env::current_exe() else { return 0 };
    let exe_name = me.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let script = format!(
        "$n=0; Get-CimInstance Win32_Process -Filter \"Name='msedgewebview2.exe'\" | Where-Object {{ $_.CommandLine -like '*--embedded-browser-webview*' -and $_.CommandLine -like '*--webview-exe-name={exe}*' -and -not (Get-Process -Id $_.ParentProcessId -ErrorAction SilentlyContinue) }} | ForEach-Object {{ taskkill.exe /PID $_.ProcessId /T /F | Out-Null; $n++ }}; $n",
        exe = exe_name.replace('\'', "''")
    );
    let mut c = std::process::Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_marker_in_command_line() {
        let cmd = ["C:\\java\\bin\\java.exe", "-Xmx4G", "-Dmcsm.id=abc-123", "-jar", "server.jar", "nogui"];
        assert_eq!(marker_id(&cmd).as_deref(), Some("abc-123"));
        assert_eq!(marker_id(&["java", "-jar", "x.jar"]), None);
        assert!(is_java("java.exe") && is_java("JAVAW.EXE") && !is_java("javac.exe"));
    }

    #[test]
    fn parses_netstat() {
        let out = "  TCP    0.0.0.0:25565          0.0.0.0:0              LISTENING       4242\n  TCP    192.168.1.5:50000      1.2.3.4:443            ESTABLISHED     77\n  TCP    [::]:25575             [::]:0                 LISTENING       4242\n  UDP    0.0.0.0:19132          *:*                                    900\n";
        let (mut t, mut u) = (HashMap::new(), HashMap::new());
        parse_netstat(out, &mut t, &mut u);
        assert_eq!(t.get(&25565), Some(&4242));
        assert_eq!(t.get(&25575), Some(&4242));
        assert!(!t.contains_key(&50000));
        assert_eq!(u.get(&19132), Some(&900));
    }

    #[test]
    fn within_is_component_safe() {
        assert!(is_within(Path::new(r"D:\srv\a\Backups\x.zip"), Path::new(r"d:\SRV\a")));
        assert!(!is_within(Path::new(r"D:\srv\ab\x.zip"), Path::new(r"D:\srv\a")));
        assert!(is_within(Path::new(r"D:\srv\a"), Path::new(r"D:\srv\a\")));
        assert!(!is_within(Path::new(r"D:\srv\a\..\..\Windows\win.ini"), Path::new(r"D:\srv\a")));
    }
}
