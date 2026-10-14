//! Tiny read-only NBT parser – enough to read `level.dat` (gzip, big-endian).

use crate::error::{AppError, AppResult};
use std::io::Read;

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    End,
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(usize),
    String(String),
    List(Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(usize),
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
    depth: u32,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> AppResult<&[u8]> {
        if self.i + n > self.b.len() {
            return Err(AppError::msg("NBT data ended early"));
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self) -> AppResult<u8> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> AppResult<i16> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> AppResult<i32> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> AppResult<i64> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn len(&mut self) -> AppResult<usize> {
        let n = self.i32()?;
        if n < 0 || n as usize > self.b.len() * 8 {
            return Err(AppError::msg("bad NBT length"));
        }
        Ok(n as usize)
    }
    fn string(&mut self) -> AppResult<String> {
        let n = self.i16()? as u16 as usize;
        // Java "modified UTF-8" – plain UTF-8 for everything a world name realistically contains.
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn payload(&mut self, kind: u8) -> AppResult<Tag> {
        Ok(match kind {
            0 => Tag::End,
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.len()?;
                self.take(n)?;
                Tag::ByteArray(n)
            }
            8 => Tag::String(self.string()?),
            9 => {
                self.depth += 1;
                if self.depth > 64 {
                    return Err(AppError::msg("NBT nested too deep"));
                }
                let inner = self.u8()?;
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(self.payload(inner)?);
                }
                self.depth -= 1;
                Tag::List(v)
            }
            10 => {
                self.depth += 1;
                if self.depth > 64 {
                    return Err(AppError::msg("NBT nested too deep"));
                }
                let mut v = vec![];
                loop {
                    let k = self.u8()?;
                    if k == 0 {
                        break;
                    }
                    let name = self.string()?;
                    v.push((name, self.payload(k)?));
                }
                self.depth -= 1;
                Tag::Compound(v)
            }
            11 => {
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(self.i32()?);
                }
                Tag::IntArray(v)
            }
            12 => {
                let n = self.len()?;
                self.take(n * 8)?;
                Tag::LongArray(n)
            }
            k => return Err(AppError::msg(format!("unknown NBT tag {k}"))),
        })
    }
}

/// Parse a (gzip-compressed or raw) NBT file's root compound.
pub fn parse(bytes: &[u8]) -> AppResult<Tag> {
    let mut data = vec![];
    let raw = if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(bytes).take(64 * 1024 * 1024).read_to_end(&mut data)?;
        &data[..]
    } else {
        bytes
    };
    let mut r = Reader { b: raw, i: 0, depth: 0 };
    let kind = r.u8()?;
    if kind != 10 {
        return Err(AppError::msg("NBT root isn't a compound"));
    }
    let _ = r.string()?;
    r.payload(10)
}

impl Tag {
    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(v) => v.iter().find(|(k, _)| k == key).map(|(_, t)| t),
            _ => None,
        }
    }
    pub fn path(&self, keys: &[&str]) -> Option<&Tag> {
        keys.iter().try_fold(self, |t, k| t.get(k))
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Tag::Byte(v) => Some(*v as i64),
            Tag::Short(v) => Some(*v as i64),
            Tag::Int(v) => Some(*v as i64),
            Tag::Long(v) => Some(*v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_nested_compound() {
        // root{ Data{ LevelName:"Test", RandomSeed:L 42, Version{ Name:"1.21.8" } } }
        let mut b = vec![10, 0, 0];
        b.extend([10, 0, 4]);
        b.extend(b"Data");
        b.extend([8, 0, 9]);
        b.extend(b"LevelName");
        b.extend([0, 4]);
        b.extend(b"Test");
        b.extend([4, 0, 10]);
        b.extend(b"RandomSeed");
        b.extend(42i64.to_be_bytes());
        b.extend([10, 0, 7]);
        b.extend(b"Version");
        b.extend([8, 0, 4]);
        b.extend(b"Name");
        b.extend([0, 6]);
        b.extend(b"1.21.8");
        b.extend([0, 0, 0]);
        let t = parse(&b).unwrap();
        assert_eq!(t.path(&["Data", "LevelName"]).and_then(|t| t.as_str()), Some("Test"));
        assert_eq!(t.path(&["Data", "RandomSeed"]).and_then(|t| t.as_i64()), Some(42));
        assert_eq!(t.path(&["Data", "Version", "Name"]).and_then(|t| t.as_str()), Some("1.21.8"));
    }
}
