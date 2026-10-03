//! Small, bounded JSON codec for diagnostics and the dependency-free command line.
//! Object keys are sorted. Duplicate object keys are rejected. Non-finite numbers
//! and lone surrogate escapes are rejected. This is not a general streaming parser.
use crate::{Error, ErrorCode, Result};
use std::collections::BTreeMap;
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_NODES: usize = 1_000_000;
/// Owned JSON data; QR result APIs expose shared references only.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}
impl Value {
    pub fn object<const N: usize>(pairs: [(&str, Value); N]) -> Self {
        Self::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }
    pub fn array(values: impl IntoIterator<Item = Value>) -> Self {
        Self::Array(values.into_iter().collect())
    }
    pub fn get(&self, key: &str) -> Option<&Self> {
        self.as_object()?.get(key)
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Self::String(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Bool(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let Self::Number(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        let v = self.as_f64()?;
        if v.is_finite() && (0.0..18_446_744_073_709_551_616.0).contains(&v) && v.fract() == 0.0 {
            Some(v as u64)
        } else {
            None
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        let v = self.as_f64()?;
        if v.is_finite()
            && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&v)
            && v.fract() == 0.0
        {
            Some(v as i64)
        } else {
            None
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        if let Self::Array(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        if let Self::Object(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) -> Result<()> {
        if let Self::Object(map) = self {
            map.insert(key.into(), value.into());
            Ok(())
        } else {
            Err(bad("Value is not an object"))
        }
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::String(v.into())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}
macro_rules! number {($($ty:ty),*)=>{$(impl From<$ty> for Value {fn from(v:$ty)->Self {Self::Number(v as f64)}})*};}
number!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f64);
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        v.map(Into::into).unwrap_or(Self::Null)
    }
}
fn bad(message: &str) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}
/// Parse a single bounded JSON value.
pub fn parse(text: &str) -> Result<Value> {
    if text.len() > MAX_BYTES {
        return Err(bad("JSON exceeds the 4 MiB input budget"));
    }
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        nodes: 0,
    };
    let value = parser.value(0)?;
    parser.space();
    if parser.pos != parser.bytes.len() {
        return Err(bad("Trailing JSON data"));
    }
    Ok(value)
}
struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    nodes: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b' ' | b'\n' | b'\r' | b'\t')
        {
            self.pos += 1;
        }
    }
    fn take(&mut self) -> Result<u8> {
        let b = self
            .bytes
            .get(self.pos)
            .copied()
            .ok_or_else(|| bad("Unexpected end of JSON"))?;
        self.pos += 1;
        Ok(b)
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        self.space();
        self.nodes += 1;
        if depth > MAX_DEPTH || self.nodes > MAX_NODES {
            return Err(bad("JSON nesting or item budget exceeded"));
        }
        match self.bytes.get(self.pos).copied() {
            Some(b'n') => {
                self.literal(b"null")?;
                Ok(Value::Null)
            }
            Some(b't') => {
                self.literal(b"true")?;
                Ok(Value::Bool(true))
            }
            Some(b'f') => {
                self.literal(b"false")?;
                Ok(Value::Bool(false))
            }
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b'[') => {
                self.pos += 1;
                self.space();
                let mut values = Vec::new();
                if self.bytes.get(self.pos) == Some(&b']') {
                    self.pos += 1;
                    return Ok(Value::Array(values));
                }
                loop {
                    values.push(self.value(depth + 1)?);
                    self.space();
                    match self.take()? {
                        b']' => break,
                        b',' => {}
                        _ => return Err(bad("Expected comma or closing array")),
                    }
                }
                Ok(Value::Array(values))
            }
            Some(b'{') => {
                self.pos += 1;
                self.space();
                let mut values = BTreeMap::new();
                if self.bytes.get(self.pos) == Some(&b'}') {
                    self.pos += 1;
                    return Ok(Value::Object(values));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    self.space();
                    if self.take()? != b':' {
                        return Err(bad("Expected object colon"));
                    }
                    let value = self.value(depth + 1)?;
                    if values.insert(key, value).is_some() {
                        return Err(bad("Duplicate JSON object key"));
                    }
                    self.space();
                    match self.take()? {
                        b'}' => break,
                        b',' => {}
                        _ => return Err(bad("Expected comma or closing object")),
                    }
                }
                Ok(Value::Object(values))
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(bad("Invalid JSON value")),
        }
    }
    fn literal(&mut self, text: &[u8]) -> Result<()> {
        if self.bytes.get(self.pos..self.pos + text.len()) != Some(text) {
            return Err(bad("Invalid JSON literal"));
        }
        self.pos += text.len();
        Ok(())
    }
    fn hex4(&mut self) -> Result<u16> {
        let mut value = 0;
        for _ in 0..4 {
            let c = self.take()?;
            let d = match c {
                b'0'..=b'9' => c - b'0',
                b'a'..=b'f' => c - b'a' + 10,
                b'A'..=b'F' => c - b'A' + 10,
                _ => return Err(bad("Invalid Unicode escape")),
            };
            value = value * 16 + u16::from(d);
        }
        Ok(value)
    }
    fn string(&mut self) -> Result<String> {
        if self.take()? != b'"' {
            return Err(bad("Expected JSON string"));
        }
        let mut output = Vec::new();
        loop {
            match self.take()? {
                b'"' => break,
                0..=31 => return Err(bad("Unescaped control in JSON string")),
                b'\\' => match self.take()? {
                    b'"' => output.push(b'"'),
                    b'\\' => output.push(b'\\'),
                    b'/' => output.push(b'/'),
                    b'b' => output.push(8),
                    b'f' => output.push(12),
                    b'n' => output.push(b'\n'),
                    b'r' => output.push(b'\r'),
                    b't' => output.push(b'\t'),
                    b'u' => {
                        let first = self.hex4()?;
                        let scalar = if (0xd800..=0xdbff).contains(&first) {
                            if self.take()? != b'\\' || self.take()? != b'u' {
                                return Err(bad("Lone high surrogate"));
                            }
                            let low = self.hex4()?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return Err(bad("Invalid surrogate pair"));
                            }
                            0x10000
                                + ((u32::from(first) - 0xd800) << 10)
                                + (u32::from(low) - 0xdc00)
                        } else {
                            u32::from(first)
                        };
                        let ch =
                            char::from_u32(scalar).ok_or_else(|| bad("Invalid Unicode scalar"))?;
                        let mut bytes = [0; 4];
                        output.extend_from_slice(ch.encode_utf8(&mut bytes).as_bytes());
                    }
                    _ => return Err(bad("Invalid JSON escape")),
                },
                byte => output.push(byte),
            }
        }
        String::from_utf8(output).map_err(|_| bad("Invalid UTF-8 JSON string"))
    }
    fn number(&mut self) -> Result<Value> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        match self.bytes.get(self.pos) {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(bad("Invalid JSON number")),
        }
        if self.bytes.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            let before = self.pos;
            while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            if before == self.pos {
                return Err(bad("Missing fractional digits"));
            }
        }
        if matches!(self.bytes.get(self.pos), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.bytes.get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let before = self.pos;
            while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            if before == self.pos {
                return Err(bad("Missing exponent digits"));
            }
        }
        let text =
            std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|_| bad("Invalid number"))?;
        let value = text.parse::<f64>().map_err(|_| bad("Invalid number"))?;
        if !value.is_finite() {
            return Err(bad("JSON number is not finite"));
        }
        Ok(Value::Number(value))
    }
}
/// Serialize a value after enforcing finite numbers and bounded nesting/output.
pub fn stringify(value: &Value) -> Result<String> {
    let mut out = String::new();
    let mut nodes = 0;
    write_value(value, &mut out, 0, &mut nodes)?;
    Ok(out)
}
fn quoted(text: &str, out: &mut String) -> Result<()> {
    if text.len() > MAX_BYTES || out.len().saturating_add(text.len()).saturating_add(2) > MAX_BYTES
    {
        return Err(bad("JSON output budget exceeded"));
    }
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch < ' ' => {
                use std::fmt::Write;
                write!(out, "\\u{:04x}", ch as u32).map_err(|_| bad("JSON formatting failed"))?;
            }
            ch => out.push(ch),
        }
        if out.len() > MAX_BYTES {
            return Err(bad("JSON output budget exceeded"));
        }
    }
    out.push('"');
    Ok(())
}
fn write_value(value: &Value, out: &mut String, depth: usize, nodes: &mut usize) -> Result<()> {
    *nodes += 1;
    if depth > MAX_DEPTH || *nodes > MAX_NODES || out.len() > MAX_BYTES {
        return Err(bad("JSON output budget exceeded"));
    }
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        Value::Number(v) => {
            if !v.is_finite() {
                return Err(bad("JSON number is not finite"));
            }
            out.push_str(&v.to_string());
        }
        Value::String(v) => quoted(v, out)?,
        Value::Array(values) => {
            out.push('[');
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(v, out, depth + 1, nodes)?;
            }
            out.push(']');
        }
        Value::Object(values) => {
            out.push('{');
            for (i, (k, v)) in values.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                quoted(k, out)?;
                out.push(':');
                write_value(v, out, depth + 1, nodes)?;
            }
            out.push('}');
        }
    }
    if out.len() > MAX_BYTES {
        return Err(bad("JSON output budget exceeded"));
    }
    Ok(())
}
