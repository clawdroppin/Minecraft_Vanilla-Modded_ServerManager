//! Lossless `server.properties` document: comments, order and unknown keys survive every edit.
//! Values are written in the Java properties format the server reads (ISO-8859-1 with `\uXXXX`
//! escapes for anything else), so MOTD colour codes (`§`) and non-Latin text round-trip.

use crate::error::AppResult;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
enum Line {
    Raw(String),
    Kv { key: String, value: String },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropsDoc {
    lines: Vec<Line>,
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('f') => out.push('\u{c}'),
            Some('u') => {
                let hex: String = (0..4).filter_map(|_| it.next()).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(ch) => out.push(ch),
                    None => {
                        out.push_str("\\u");
                        out.push_str(&hex);
                    }
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    // Surrogate pairs written as two \u escapes arrive as replacement chars above; good enough for
    // a properties file (MC only writes BMP characters).
    out
}

fn escape(s: &str, is_key: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{c}' => out.push_str("\\f"),
            '=' | ':' if is_key => {
                out.push('\\');
                out.push(c);
            }
            ' ' if is_key || i == 0 => out.push_str("\\ "),
            '#' | '!' if i == 0 => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04X}", u));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Split `key=value` / `key: value` / `key value` at the first unescaped separator.
fn split_kv(line: &str) -> (String, String) {
    let t = line.trim_start();
    let mut key_end = t.len();
    let mut value_start = t.len();
    let mut escaped = false;
    for (i, c) in t.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '=' || c == ':' || c.is_whitespace() {
            key_end = i;
            let rest = &t[i..];
            let rest_trim = rest.trim_start();
            let skip = rest.len() - rest_trim.len();
            let mut j = i + skip;
            if !c.is_whitespace() {
                j = i + 1;
            } else if rest_trim.starts_with('=') || rest_trim.starts_with(':') {
                j = i + skip + 1;
            }
            let after = &t[j..];
            value_start = j + (after.len() - after.trim_start().len());
            break;
        }
    }
    (unescape(&t[..key_end]), unescape(&t[value_start.min(t.len())..]))
}

impl PropsDoc {
    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .map(|l| {
                let t = l.trim_start();
                if t.is_empty() || t.starts_with('#') || t.starts_with('!') {
                    Line::Raw(l.to_string())
                } else {
                    let (key, value) = split_kv(l);
                    Line::Kv { key, value }
                }
            })
            .collect();
        Self { lines }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn get(&self, key: &str) -> Option<String> {
        self.lines.iter().rev().find_map(|l| match l {
            Line::Kv { key: k, value } if k == key => Some(value.clone()),
            _ => None,
        })
    }

    pub fn set(&mut self, key: &str, value: &str) {
        let mut found = false;
        for l in self.lines.iter_mut() {
            if let Line::Kv { key: k, value: v } = l {
                if k == key {
                    *v = value.to_string();
                    found = true;
                }
            }
        }
        if !found {
            self.lines.push(Line::Kv { key: key.to_string(), value: value.to_string() });
        }
    }

    pub fn map(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        for l in &self.lines {
            if let Line::Kv { key, value } = l {
                m.insert(key.clone(), value.clone());
            }
        }
        m
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for l in &self.lines {
            match l {
                Line::Raw(r) => out.push_str(r),
                Line::Kv { key, value } => {
                    out.push_str(&escape(key, true));
                    out.push('=');
                    out.push_str(&escape(value, false));
                }
            }
            out.push('\n');
        }
        out
    }
}

/// Read a properties file (missing = empty). Bytes that aren't UTF-8 are taken as Latin-1, which is
/// what Java's `Properties.load` assumes.
pub fn read(path: &Path) -> AppResult<PropsDoc> {
    match std::fs::read(path) {
        Ok(b) => {
            let text = match String::from_utf8(b) {
                Ok(t) => t,
                Err(e) => e.into_bytes().iter().map(|b| *b as char).collect(),
            };
            Ok(PropsDoc::parse(text.trim_start_matches('\u{feff}')))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(PropsDoc::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn write(path: &Path, doc: &PropsDoc) -> AppResult<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("properties.tmp");
    std::fs::write(&tmp, doc.to_text())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Merge `values` into the file at `path` (keeps comments, order and keys not in the map).
pub fn merge_into(path: &Path, values: &BTreeMap<String, String>) -> AppResult<()> {
    let mut doc = read(path)?;
    if doc.lines.is_empty() {
        doc.lines.push(Line::Raw("#Minecraft server properties".into()));
        doc.lines.push(Line::Raw(format!("#Written by Minecraft Server Manager {}", chrono::Local::now().format("%Y-%m-%d %H:%M"))));
    }
    for (k, v) in values {
        doc.set(k, v);
    }
    write(path, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_escapes() {
        let text = "#Minecraft server properties\n#Mon Oct 13 2026\nmotd=\\u00A7aHello \\u00A7lWorld\nserver-port=25565\ngenerator-settings={}\nlevel-seed=\n";
        let mut d = PropsDoc::parse(text);
        assert_eq!(d.get("motd").unwrap(), "§aHello §lWorld");
        assert_eq!(d.get("level-seed").unwrap(), "");
        assert_eq!(d.to_text(), text.replace("\\u00A7", "\\u00A7"));
        d.set("motd", "Ünïcødé ✓");
        d.set("new-key", "a:b=c");
        let t = d.to_text();
        assert!(t.contains("motd=\\u00DCn\\u00EFc\\u00F8d\\u00E9 \\u2713"));
        assert!(t.contains("new-key=a:b=c"));
        let back = PropsDoc::parse(&t);
        assert_eq!(back.get("motd").unwrap(), "Ünïcødé ✓");
        assert!(t.starts_with("#Minecraft server properties\n#Mon Oct 13 2026\n"));
    }

    #[test]
    fn odd_separators() {
        let d = PropsDoc::parse("a = 1\nb:2\nc 3\nkey\\=x=y\n  spaced=  v \n");
        assert_eq!(d.get("a").unwrap(), "1");
        assert_eq!(d.get("b").unwrap(), "2");
        assert_eq!(d.get("c").unwrap(), "3");
        assert_eq!(d.get("key=x").unwrap(), "y");
        assert_eq!(d.get("spaced").unwrap(), "v ");
    }
}
