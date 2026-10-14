//! Player lists (whitelist, ops, bans) and UUID lookups. While the server runs it owns these files,
//! so changes go through console commands; when it's stopped the JSON files are edited directly.

use crate::error::{AppError, AppResult};
use crate::models::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

pub const LISTS: &[(&str, &str)] = &[("whitelist", "whitelist.json"), ("ops", "ops.json"), ("banned-players", "banned-players.json"), ("banned-ips", "banned-ips.json")];

fn file_for(list: &str) -> AppResult<&'static str> {
    LISTS.iter().find(|(l, _)| *l == list).map(|(_, f)| *f).ok_or_else(|| AppError::msg(format!("Unknown list {list}")))
}

fn read_array(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Vec<Value>>(&t).ok()).unwrap_or_default()
}

fn write_array(path: &Path, v: &[Value]) -> AppResult<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(v)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerLists {
    pub whitelist: Vec<Value>,
    pub ops: Vec<Value>,
    pub banned_players: Vec<Value>,
    pub banned_ips: Vec<Value>,
    /// Everyone the server has seen (usercache.json), newest first.
    pub known: Vec<Value>,
}

pub fn read_lists(inst: &ServerInstance) -> PlayerLists {
    let d = &inst.install_dir;
    let mut known = read_array(&d.join("usercache.json"));
    known.sort_by(|a, b| b["expiresOn"].as_str().unwrap_or("").cmp(a["expiresOn"].as_str().unwrap_or("")));
    PlayerLists {
        whitelist: read_array(&d.join("whitelist.json")),
        ops: read_array(&d.join("ops.json")),
        banned_players: read_array(&d.join("banned-players.json")),
        banned_ips: read_array(&d.join("banned-ips.json")),
        known,
    }
}

/// Java's `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)` – the id offline-mode servers use.
pub fn offline_uuid(name: &str) -> String {
    let mut h = md5::compute(format!("OfflinePlayer:{name}").as_bytes()).0;
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(h).to_string()
}

pub fn dashed(id: &str) -> String {
    uuid::Uuid::parse_str(id).map(|u| u.to_string()).unwrap_or_else(|_| id.to_string())
}

/// Resolve a player name to (uuid, exact name). Online-mode servers need the Mojang account id;
/// offline-mode ones derive it from the name.
pub async fn lookup(http: &reqwest::Client, name: &str, online_mode: bool) -> AppResult<(String, String)> {
    let name = name.trim();
    if name.is_empty() || name.len() > 16 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(AppError::msg(format!("“{name}” isn't a valid Java Edition player name (3–16 letters, digits or _).")));
    }
    if !online_mode {
        return Ok((offline_uuid(name), name.to_string()));
    }
    let r = http.get(format!("https://api.mojang.com/users/profiles/minecraft/{}", urlencoding::encode(name))).timeout(std::time::Duration::from_secs(10)).send().await?;
    if r.status().as_u16() == 404 || r.status().as_u16() == 204 {
        return Err(AppError::msg(format!("No Minecraft account is called “{name}”.")));
    }
    let v: Value = r.error_for_status()?.json().await?;
    Ok((dashed(v["id"].as_str().unwrap_or_default()), v["name"].as_str().unwrap_or(name).to_string()))
}

fn now_str() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z").to_string()
}

/// Edit a list file directly (server stopped). `target` is a name (players) or IP (banned-ips).
pub fn edit_file(inst: &ServerInstance, list: &str, add: bool, target: &str, uuid: Option<&str>, reason: &str, op_level: i64) -> AppResult<()> {
    let path = inst.install_dir.join(file_for(list)?);
    let mut arr = read_array(&path);
    let key_match = |v: &Value| -> bool {
        if list == "banned-ips" {
            v["ip"].as_str() == Some(target)
        } else {
            v["name"].as_str().map(|n| n.eq_ignore_ascii_case(target)).unwrap_or(false) || uuid.map(|u| v["uuid"].as_str() == Some(u)).unwrap_or(false)
        }
    };
    arr.retain(|v| !key_match(v));
    if add {
        let entry = match list {
            "whitelist" => json!({ "uuid": uuid.unwrap_or_default(), "name": target }),
            "ops" => json!({ "uuid": uuid.unwrap_or_default(), "name": target, "level": op_level, "bypassesPlayerLimit": false }),
            "banned-players" => json!({ "uuid": uuid.unwrap_or_default(), "name": target, "created": now_str(), "source": "Minecraft Server Manager", "expires": "forever", "reason": if reason.is_empty() { "Banned by an operator." } else { reason } }),
            _ => json!({ "ip": target, "created": now_str(), "source": "Minecraft Server Manager", "expires": "forever", "reason": if reason.is_empty() { "Banned by an operator." } else { reason } }),
        };
        arr.push(entry);
    }
    write_array(&path, &arr)
}

/// Console command for a list change on a running server.
pub fn command_for(list: &str, add: bool, target: &str, reason: &str) -> AppResult<String> {
    let r = if reason.trim().is_empty() { String::new() } else { format!(" {}", reason.trim()) };
    Ok(match (list, add) {
        ("whitelist", true) => format!("whitelist add {target}"),
        ("whitelist", false) => format!("whitelist remove {target}"),
        ("ops", true) => format!("op {target}"),
        ("ops", false) => format!("deop {target}"),
        ("banned-players", true) => format!("ban {target}{r}"),
        ("banned-players", false) => format!("pardon {target}"),
        ("banned-ips", true) => format!("ban-ip {target}{r}"),
        ("banned-ips", false) => format!("pardon-ip {target}"),
        _ => return Err(AppError::msg(format!("Unknown list {list}"))),
    })
}

/// Per-player statistics from the world's `stats/<uuid>.json` (what the game shows under
/// Statistics): play time, deaths, kills, distance, blocks mined.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStats {
    pub uuid: String,
    pub name: String,
    /// Ticks (20 per second).
    pub play_ticks: u64,
    pub deaths: u64,
    pub mob_kills: u64,
    pub player_kills: u64,
    /// Centimetres travelled (walking, sprinting, flying, swimming, riding…).
    pub distance_cm: u64,
    pub jumps: u64,
    pub blocks_mined: u64,
    pub items_crafted: u64,
    /// When the stats file was last written (the server saves it when the player leaves / autosaves).
    pub last_seen: i64,
}

/// Stats folders of the active world: `<world>/stats` (all versions so far) and `<world>/players/stats`.
fn stats_dirs(inst: &ServerInstance) -> Vec<std::path::PathBuf> {
    let w = inst.world_dir();
    [w.join("stats"), w.join("players").join("stats")].into_iter().filter(|d| d.is_dir()).collect()
}

/// Parse one stats file. 1.13+ nests `stats."minecraft:custom"."minecraft:play_time"`; older
/// versions use flat `stat.playOneMinute` keys.
pub fn parse_stats(v: &Value) -> PlayerStats {
    let mut s = PlayerStats::default();
    let num = |x: &Value| x.as_u64().or_else(|| x.as_f64().map(|f| f as u64)).unwrap_or(0);
    let sum = |x: &Value| x.as_object().map(|o| o.values().map(num).sum::<u64>()).unwrap_or(0);
    if let Some(stats) = v.get("stats").and_then(|x| x.as_object()) {
        let c = &stats.get("minecraft:custom").cloned().unwrap_or(Value::Null);
        s.play_ticks = num(&c["minecraft:play_time"]).max(num(&c["minecraft:play_one_minute"]));
        s.deaths = num(&c["minecraft:deaths"]);
        s.mob_kills = num(&c["minecraft:mob_kills"]);
        s.player_kills = num(&c["minecraft:player_kills"]);
        s.jumps = num(&c["minecraft:jump"]);
        s.distance_cm = c.as_object().map(|o| o.iter().filter(|(k, _)| k.ends_with("_one_cm")).map(|(_, v)| num(v)).sum()).unwrap_or(0);
        s.blocks_mined = stats.get("minecraft:mined").map(sum).unwrap_or(0);
        s.items_crafted = stats.get("minecraft:crafted").map(sum).unwrap_or(0);
    } else if let Some(o) = v.as_object() {
        let g = |k: &str| o.get(k).map(num).unwrap_or(0);
        s.play_ticks = g("stat.playOneMinute");
        s.deaths = g("stat.deaths");
        s.mob_kills = g("stat.mobKills");
        s.player_kills = g("stat.playerKills");
        s.jumps = g("stat.jump");
        s.distance_cm = o.iter().filter(|(k, _)| k.starts_with("stat.") && k.ends_with("OneCm")).map(|(_, v)| num(v)).sum();
        s.blocks_mined = o.iter().filter(|(k, _)| k.starts_with("stat.mineBlock.")).map(|(_, v)| num(v)).sum();
        s.items_crafted = o.iter().filter(|(k, _)| k.starts_with("stat.craftItem.")).map(|(_, v)| num(v)).sum();
    }
    s
}

/// Everyone with a stats file in the active world, most played first. Names come from
/// usercache.json (the server's own uuid → name cache).
pub fn stats(inst: &ServerInstance) -> Vec<PlayerStats> {
    let names: std::collections::HashMap<String, String> = read_array(&inst.install_dir.join("usercache.json"))
        .into_iter()
        .filter_map(|v| Some((v["uuid"].as_str()?.to_lowercase(), v["name"].as_str()?.to_string())))
        .collect();
    let mut out: Vec<PlayerStats> = vec![];
    for dir in stats_dirs(inst) {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x != "json").unwrap_or(true) {
                continue;
            }
            let uuid = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            if out.iter().any(|s| s.uuid == uuid) {
                continue;
            }
            let Some(v) = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { continue };
            let mut s = parse_stats(&v);
            s.name = names.get(&uuid).cloned().unwrap_or_default();
            s.uuid = uuid;
            s.last_seen = e.metadata().ok().and_then(|m| m.modified().ok()).map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()).unwrap_or(0);
            out.push(s);
        }
    }
    out.sort_by(|a, b| b.play_ticks.cmp(&a.play_ticks));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_uuids_match_java() {
        // Known value: UUID.nameUUIDFromBytes("OfflinePlayer:Notch")
        assert_eq!(offline_uuid("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(dashed("069a79f444e94726a5befca90e38aaf5"), "069a79f4-44e9-4726-a5be-fca90e38aaf5");
    }
    #[test]
    fn parses_stats_both_formats() {
        let modern = json!({"stats": {"minecraft:custom": {"minecraft:play_time": 72000, "minecraft:deaths": 2, "minecraft:walk_one_cm": 1000, "minecraft:sprint_one_cm": 500, "minecraft:jump": 7}, "minecraft:mined": {"minecraft:stone": 30, "minecraft:dirt": 12}}, "DataVersion": 4440});
        let s = parse_stats(&modern);
        assert_eq!((s.play_ticks, s.deaths, s.distance_cm, s.jumps, s.blocks_mined), (72000, 2, 1500, 7, 42));
        let legacy = json!({"stat.playOneMinute": 2400, "stat.walkOneCm": 300, "stat.mineBlock.minecraft.stone": 5, "stat.deaths": 1});
        let s = parse_stats(&legacy);
        assert_eq!((s.play_ticks, s.distance_cm, s.blocks_mined, s.deaths), (2400, 300, 5, 1));
    }
    #[test]
    fn edits_lists() {
        let root = std::env::temp_dir().join(format!("mcsm-pl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let inst = ServerInstance { install_dir: root.clone(), ..Default::default() };
        edit_file(&inst, "whitelist", true, "Steve", Some("u-1"), "", 4).unwrap();
        edit_file(&inst, "whitelist", true, "Alex", Some("u-2"), "", 4).unwrap();
        edit_file(&inst, "whitelist", true, "steve", Some("u-1"), "", 4).unwrap();
        assert_eq!(read_lists(&inst).whitelist.len(), 2);
        edit_file(&inst, "whitelist", false, "Alex", None, "", 4).unwrap();
        assert_eq!(read_lists(&inst).whitelist.len(), 1);
        edit_file(&inst, "banned-ips", true, "1.2.3.4", None, "spam", 4).unwrap();
        assert_eq!(read_lists(&inst).banned_ips[0]["reason"], "spam");
        assert_eq!(command_for("banned-players", true, "Bob", "griefing").unwrap(), "ban Bob griefing");
        let _ = std::fs::remove_dir_all(root);
    }
}
