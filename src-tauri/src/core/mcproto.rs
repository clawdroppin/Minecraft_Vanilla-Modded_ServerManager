//! Minimal Minecraft network protocols: RCON (commands, player list, TPS) and the Server List
//! Ping status query (reachability tests, MOTD / player count of any server).

use crate::error::{AppError, AppResult};
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// ------------------------------------------------------------------------------------- RCON

pub struct Rcon {
    stream: TcpStream,
    next_id: i32,
}

async fn write_packet(s: &mut TcpStream, id: i32, kind: i32, body: &str) -> std::io::Result<()> {
    let b = body.as_bytes();
    let len = 4 + 4 + b.len() as i32 + 2;
    let mut buf = Vec::with_capacity(len as usize + 4);
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&id.to_le_bytes());
    buf.extend_from_slice(&kind.to_le_bytes());
    buf.extend_from_slice(b);
    buf.extend_from_slice(&[0, 0]);
    s.write_all(&buf).await
}

async fn read_packet(s: &mut TcpStream) -> std::io::Result<(i32, i32, String)> {
    let len = s.read_i32_le().await?;
    if !(10..=1_048_576).contains(&len) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad RCON packet length"));
    }
    let id = s.read_i32_le().await?;
    let kind = s.read_i32_le().await?;
    let mut body = vec![0u8; (len - 8) as usize];
    s.read_exact(&mut body).await?;
    while body.last() == Some(&0) {
        body.pop();
    }
    Ok((id, kind, String::from_utf8_lossy(&body).into_owned()))
}

impl Rcon {
    pub async fn connect(host: &str, port: u16, password: &str) -> AppResult<Self> {
        let addr = format!("{host}:{port}");
        let stream = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(&addr))
            .await
            .map_err(|_| AppError::msg("RCON connection timed out"))?
            .map_err(|e| AppError::msg(format!("RCON connection failed: {e}")))?;
        let _ = stream.set_nodelay(true);
        let mut r = Rcon { stream, next_id: 1 };
        write_packet(&mut r.stream, 1, 3, password).await?;
        let (id, _, _) = tokio::time::timeout(Duration::from_secs(4), read_packet(&mut r.stream))
            .await
            .map_err(|_| AppError::msg("RCON login timed out"))??;
        if id == -1 {
            return Err(AppError::msg("RCON password was rejected"));
        }
        Ok(r)
    }

    /// Run a command and return its output (multi-packet replies are joined).
    pub async fn cmd(&mut self, command: &str) -> AppResult<String> {
        self.next_id = self.next_id.wrapping_add(1).max(2);
        let id = self.next_id;
        write_packet(&mut self.stream, id, 2, command).await?;
        let mut out = String::new();
        let first = tokio::time::timeout(Duration::from_secs(8), read_packet(&mut self.stream)).await.map_err(|_| AppError::msg("RCON reply timed out"))??;
        out.push_str(&first.2);
        // Long replies arrive in 4 KB chunks: keep reading while more is immediately available.
        if first.2.len() >= 4000 {
            while let Ok(Ok(p)) = tokio::time::timeout(Duration::from_millis(150), read_packet(&mut self.stream)).await {
                out.push_str(&p.2);
                if p.2.len() < 4000 {
                    break;
                }
            }
        }
        Ok(out)
    }
}

/// Remove `§x` formatting codes (and stray ANSI escapes) from server output.
pub fn strip_codes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '§' {
            it.next();
        } else if c == '\u{1b}' {
            // ESC [ ... letter
            if it.peek() == Some(&'[') {
                it.next();
                for n in it.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

static LIST_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)There are (\d+)(?: of a max of |/| out of maximum )(\d+) players online[.:]?\s*(.*)").unwrap());
static FORGE_TPS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Overall: ([0-9.]+) TPS \(([0-9.]+) ms/tick\)").unwrap());
static MEAN_TPS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Mean TPS: ([0-9.]+)").unwrap());
static MSPT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(?:Average time per tick|Avg\.? tick time|MSPT|Mean tick time)[^0-9]*([0-9.]+)\s*ms").unwrap());

/// Parse `list`: "There are 2 of a max of 20 players online: Steve, Alex" (also older
/// "There are 2/20 players online:" forms). Returns (online, max, names).
pub fn parse_list(reply: &str) -> Option<(u32, u32, Vec<String>)> {
    let t = strip_codes(reply);
    let c = LIST_RE.captures(&t)?;
    let names = c[3]
        .split([',', '\n'])
        .map(|n| n.trim().trim_end_matches('.').to_string())
        // Some servers print "name (uuid)" with `list uuids`
        .map(|n| n.split(' ').next().unwrap_or("").to_string())
        .filter(|n| !n.is_empty())
        .collect();
    Some((c[1].parse().ok()?, c[2].parse().ok()?, names))
}

/// Parse Paper's `tps` ("TPS from last 1m, 5m, 15m: 20.0, 19.98, 20.0") or vanilla's
/// `tick query` ("Average time per tick: 1.2ms"). Returns (tps, mspt).
pub fn parse_tps(reply: &str) -> (Option<f64>, Option<f64>) {
    let t = strip_codes(reply);
    let num = |s: &str| s.trim().trim_start_matches('*').trim_end_matches(',').parse::<f64>().ok();
    let mut tps = None;
    let mut mspt = None;
    if let Some(i) = t.find("TPS from last") {
        let rest = &t[i..];
        if let Some(c) = rest.find(':') {
            tps = rest[c + 1..].split(',').next().and_then(num);
        }
    }
    // NeoForge / Forge: "Overall: 20.000 TPS (0.120 ms/tick)" and older "Overall: Mean tick time: 3.4 ms. Mean TPS: 20.000"
    if tps.is_none() {
        if let Some(c) = FORGE_TPS_RE.captures(&t) {
            tps = c[1].parse().ok();
            mspt = c[2].parse().ok();
        } else if let Some(c) = MEAN_TPS_RE.captures(&t) {
            tps = c[1].parse().ok();
        }
    }
    if mspt.is_none() {
        if let Some(c) = MSPT_RE.captures(&t) {
            mspt = c[1].parse().ok();
        }
    }
    if tps.is_none() {
        if let Some(m) = mspt {
            // Vanilla runs at most 20 TPS; slower ticks drop it proportionally.
            tps = Some(if m <= 50.0 { 20.0 } else { 1000.0 / m });
        }
    }
    (tps.map(|v: f64| v.min(20.0)), mspt)
}

// ----------------------------------------------------------------------- Server List Ping

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub online: bool,
    pub address: String,
    pub latency_ms: Option<u64>,
    pub version: String,
    pub protocol: i64,
    pub players_online: i64,
    pub players_max: i64,
    pub sample: Vec<String>,
    /// MOTD as plain text and with § codes (for the coloured preview).
    pub motd: String,
    pub motd_raw: String,
    pub favicon: Option<String>,
    pub error: Option<String>,
}

fn varint(mut v: i32, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v = ((v as u32) >> 7) as i32;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

async fn read_varint(s: &mut TcpStream) -> std::io::Result<i32> {
    let mut n = 0i32;
    for i in 0..5 {
        let b = s.read_u8().await?;
        n |= ((b & 0x7f) as i32) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(n);
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "varint too long"))
}

/// Flatten a chat component (string or JSON text) into `§`-coded text.
fn chat_to_legacy(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(o) => {
            let mut out = String::new();
            if let Some(c) = o.get("color").and_then(|c| c.as_str()) {
                let code = match c {
                    "black" => "0",
                    "dark_blue" => "1",
                    "dark_green" => "2",
                    "dark_aqua" => "3",
                    "dark_red" => "4",
                    "dark_purple" => "5",
                    "gold" => "6",
                    "gray" => "7",
                    "dark_gray" => "8",
                    "blue" => "9",
                    "green" => "a",
                    "aqua" => "b",
                    "red" => "c",
                    "light_purple" => "d",
                    "yellow" => "e",
                    "white" => "f",
                    _ => "",
                };
                if !code.is_empty() {
                    out.push('§');
                    out.push_str(code);
                }
            }
            for (k, code) in [("bold", "l"), ("italic", "o"), ("underlined", "n"), ("strikethrough", "m"), ("obfuscated", "k")] {
                if o.get(k).and_then(|b| b.as_bool()) == Some(true) {
                    out.push('§');
                    out.push_str(code);
                }
            }
            if let Some(t) = o.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
            if let Some(extra) = o.get("extra").and_then(|e| e.as_array()) {
                for e in extra {
                    out.push_str(&chat_to_legacy(e));
                }
            }
            out
        }
        serde_json::Value::Array(a) => a.iter().map(chat_to_legacy).collect(),
        _ => String::new(),
    }
}

async fn ping_inner(host: &str, port: u16) -> AppResult<PingResult> {
    let t0 = Instant::now();
    let mut s = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect((host, port)))
        .await
        .map_err(|_| AppError::msg("Connection timed out"))?
        .map_err(|e| AppError::msg(format!("Connection failed: {e}")))?;
    let latency = t0.elapsed().as_millis() as u64;
    let mut hs = vec![];
    varint(0x00, &mut hs);
    varint(-1, &mut hs); // protocol: "any" for status
    varint(host.len() as i32, &mut hs);
    hs.extend_from_slice(host.as_bytes());
    hs.extend_from_slice(&port.to_be_bytes());
    varint(1, &mut hs); // next state: status
    let mut pkt = vec![];
    varint(hs.len() as i32, &mut pkt);
    pkt.extend_from_slice(&hs);
    pkt.extend_from_slice(&[1, 0]); // status request
    s.write_all(&pkt).await?;
    let read = async {
        let _len = read_varint(&mut s).await?;
        let _id = read_varint(&mut s).await?;
        let n = read_varint(&mut s).await?;
        if !(0..=2_097_152).contains(&n) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad status length"));
        }
        let mut buf = vec![0u8; n as usize];
        s.read_exact(&mut buf).await?;
        Ok::<Vec<u8>, std::io::Error>(buf)
    };
    let buf = tokio::time::timeout(Duration::from_secs(6), read).await.map_err(|_| AppError::msg("The server didn't answer the status request"))??;
    let v: serde_json::Value = serde_json::from_slice(&buf)?;
    let raw = chat_to_legacy(&v["description"]);
    Ok(PingResult {
        online: true,
        address: format!("{host}:{port}"),
        latency_ms: Some(latency),
        version: v["version"]["name"].as_str().unwrap_or_default().to_string(),
        protocol: v["version"]["protocol"].as_i64().unwrap_or(0),
        players_online: v["players"]["online"].as_i64().unwrap_or(0),
        players_max: v["players"]["max"].as_i64().unwrap_or(0),
        sample: v["players"]["sample"].as_array().map(|a| a.iter().filter_map(|p| p["name"].as_str().map(String::from)).collect()).unwrap_or_default(),
        motd: strip_codes(&raw),
        motd_raw: raw,
        favicon: v["favicon"].as_str().map(String::from),
        error: None,
    })
}

/// Status ping that never fails: errors land in `error`.
pub async fn ping(host: &str, port: u16) -> PingResult {
    match ping_inner(host, port).await {
        Ok(r) => r,
        Err(e) => PingResult { online: false, address: format!("{host}:{port}"), error: Some(e.to_string()), ..Default::default() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_list_and_tps() {
        let (n, m, names) = parse_list("There are 2 of a max of 20 players online: Steve, Alex").unwrap();
        assert_eq!((n, m), (2, 20));
        assert_eq!(names, vec!["Steve", "Alex"]);
        let (n, _, names) = parse_list("There are 0 of a max of 10 players online: ").unwrap();
        assert_eq!(n, 0);
        assert!(names.is_empty());
        assert_eq!(parse_list("§6There are §c1§6 out of maximum §c50§6 players online.").map(|x| x.0), Some(1));
        let (t, _) = parse_tps("§6TPS from last 1m, 5m, 15m: §a*20.0, §a19.97, §a20.0");
        assert_eq!(t, Some(20.0));
        let (t, ms) = parse_tps("The game is running normally\nTarget tick rate: 20.0 per second.\nAverage time per tick: 3.4ms (Target: 50.0ms)");
        assert_eq!((t, ms), (Some(20.0), Some(3.4)));
        let (t, ms) = parse_tps("Overworld: 19.000 TPS (52.1 ms/tick)
Overall: 19.500 TPS (51.300 ms/tick)");
        assert_eq!((t, ms), (Some(19.5), Some(51.3)));
        assert_eq!(parse_tps("Overall: Mean tick time: 3.456 ms. Mean TPS: 20.000").0, Some(20.0));
        let (t, _) = parse_tps("Average time per tick: 100.0ms (Target: 50.0ms)");
        assert_eq!(t, Some(10.0));
        assert_eq!(strip_codes("§aHi \u{1b}[0;32mthere\u{1b}[m"), "Hi there");
    }
    #[test]
    fn chat_component() {
        let v: serde_json::Value = serde_json::json!({"text":"","extra":[{"text":"A ","color":"green"},{"text":"B","bold":true}]});
        assert_eq!(chat_to_legacy(&v), "§aA §lB");
    }
}
