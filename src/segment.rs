//! Validated immutable data/control segments and deterministic mixed-mode planning.
//!
//! Wire behavior matches user-owned SpecQR commit
//! `15ad15e5c770ea0e39072f8f88b2733018f02ffd`.
//! `src/encoding/modes.js` SHA-256:
//! `204a3a4df0a81b1e8decc63299cd099812ee4708882feb4ccb47f97ae68333e4`.
//! `src/encoding/control-segments.js` SHA-256:
//! `fa3de4161e4d3ce44ea00f98070b424b6d0a03e76c34f87ec8fd057abcab5993`.

use crate::{Error, ErrorCode, Result, kanji, tables};
use std::{fmt, str::FromStr};

pub const MAX_PAYLOAD_UNITS: usize = 1_000_000;
pub const MAX_MANUAL_SEGMENTS: usize = 16_384;
pub const MAX_SINGLE_SYMBOL_CHARACTERS: usize = 7_089;
pub const MAX_SINGLE_SYMBOL_DATA_BITS: usize = 23_648;
pub const ALPHANUMERIC: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

/// Data and control mode indicators. Automatic selection is represented by `None`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Mode {
    Numeric,
    Alphanumeric,
    Byte,
    Kanji,
    Eci,
    Fnc1,
    Fnc1Second,
    StructuredAppend,
}
impl Mode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::Alphanumeric => "alphanumeric",
            Self::Byte => "byte",
            Self::Kanji => "kanji",
            Self::Eci => "eci",
            Self::Fnc1 => "fnc1",
            Self::Fnc1Second => "fnc1-second",
            Self::StructuredAppend => "structured-append",
        }
    }
    pub const fn is_control(self) -> bool {
        matches!(
            self,
            Self::Eci | Self::Fnc1 | Self::Fnc1Second | Self::StructuredAppend
        )
    }
    const fn indicator(self) -> u32 {
        match self {
            Self::Numeric => 1,
            Self::Alphanumeric => 2,
            Self::StructuredAppend => 3,
            Self::Byte => 4,
            Self::Fnc1 => 5,
            Self::Eci => 7,
            Self::Kanji => 8,
            Self::Fnc1Second => 9,
        }
    }
}
impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for Mode {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "numeric" => Ok(Self::Numeric),
            "alphanumeric" => Ok(Self::Alphanumeric),
            "byte" => Ok(Self::Byte),
            "kanji" => Ok(Self::Kanji),
            "eci" => Ok(Self::Eci),
            "fnc1" => Ok(Self::Fnc1),
            "fnc1-second" => Ok(Self::Fnc1Second),
            "structured-append" => Ok(Self::StructuredAppend),
            _ => Err(Error::new(
                ErrorCode::InvalidMode,
                "Unknown QR segment mode",
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Payload {
    Text(String),
    Bytes(Vec<u8>),
    Eci(u32),
    Fnc1,
    Fnc1Second(String),
    StructuredAppend { index: u8, total: u8, parity: u8 },
}

/// An owned immutable segment with validated content and control parameters.
///
/// Text is always valid Unicode and its logical bytes are UTF-8. ECI labels the
/// following bytes without transcoding them. Kanji's logical bytes are also UTF-8,
/// including when computing Structured Append parity. Manual FNC1 alphanumeric
/// percent escaping is preserved exactly as supplied.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Segment {
    mode: Mode,
    payload: Payload,
    characters: usize,
}
impl Segment {
    pub fn numeric(text: &str) -> Result<Self> {
        Self::from_text(Mode::Numeric, text)
    }
    pub fn alphanumeric(text: &str) -> Result<Self> {
        Self::from_text(Mode::Alphanumeric, text)
    }
    pub fn utf8(text: &str) -> Result<Self> {
        Self::from_text(Mode::Byte, text)
    }
    pub fn kanji(text: &str) -> Result<Self> {
        Self::from_text(Mode::Kanji, text)
    }
    pub fn bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_PAYLOAD_UNITS {
            return Err(payload_too_long());
        }
        Ok(Self {
            mode: Mode::Byte,
            payload: Payload::Bytes(bytes.to_vec()),
            characters: 0,
        })
    }
    pub fn from_text(mode: Mode, text: &str) -> Result<Self> {
        if mode.is_control() {
            return Err(Error::new(
                ErrorCode::InvalidMode,
                "Text requires a data segment mode",
            ));
        }
        let characters = validate_text(text)?;
        for character in text.chars() {
            let valid = match mode {
                Mode::Numeric => character.is_ascii_digit(),
                Mode::Alphanumeric => alphanumeric_value(character).is_some(),
                Mode::Kanji => kanji::can_encode(character),
                Mode::Byte => true,
                _ => false,
            };
            if !valid {
                return Err(Error::new(
                    ErrorCode::InvalidMode,
                    format!("{} mode cannot encode U+{:04X}", mode, u32::from(character)),
                ));
            }
        }
        Ok(Self {
            mode,
            payload: Payload::Text(text.to_owned()),
            characters,
        })
    }
    pub fn eci(assignment: u32) -> Result<Self> {
        if assignment > 999_999 {
            return Err(Error::new(
                ErrorCode::InvalidEci,
                "ECI assignment must be from 0 to 999999",
            ));
        }
        Ok(Self {
            mode: Mode::Eci,
            payload: Payload::Eci(assignment),
            characters: 0,
        })
    }
    pub const fn fnc1() -> Self {
        Self {
            mode: Mode::Fnc1,
            payload: Payload::Fnc1,
            characters: 0,
        }
    }
    pub fn fnc1_second(indicator: &str) -> Result<Self> {
        let bytes = indicator.as_bytes();
        if !((bytes.len() == 2 && bytes.iter().all(u8::is_ascii_digit))
            || (bytes.len() == 1 && bytes[0].is_ascii_alphabetic()))
        {
            return Err(Error::new(
                ErrorCode::InvalidMode,
                "FNC1 second indicator must be two ASCII digits or one ASCII letter",
            ));
        }
        Ok(Self {
            mode: Mode::Fnc1Second,
            payload: Payload::Fnc1Second(indicator.to_owned()),
            characters: 0,
        })
    }
    /// Create a 1-based Structured Append header, with a total of 2–16 symbols.
    pub fn structured_append(index: u8, total: u8, parity: u8) -> Result<Self> {
        if !(2..=16).contains(&total) || index == 0 || index > total {
            return Err(Error::new(
                ErrorCode::InvalidMode,
                "Structured Append requires 1 <= index <= total and 2 <= total <= 16",
            ));
        }
        Ok(Self {
            mode: Mode::StructuredAppend,
            payload: Payload::StructuredAppend {
                index,
                total,
                parity,
            },
            characters: 0,
        })
    }
    pub const fn mode(&self) -> Mode {
        self.mode
    }
    pub fn text(&self) -> Option<&str> {
        match &self.payload {
            Payload::Text(text) => Some(text),
            _ => None,
        }
    }
    pub fn binary(&self) -> Option<&[u8]> {
        match &self.payload {
            Payload::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }
    pub fn logical_bytes(&self) -> &[u8] {
        match &self.payload {
            Payload::Text(text) => text.as_bytes(),
            Payload::Bytes(bytes) => bytes,
            _ => &[],
        }
    }
    pub fn count(&self) -> usize {
        if self.mode == Mode::Byte {
            self.logical_bytes().len()
        } else {
            self.characters
        }
    }
    pub const fn character_count(&self) -> usize {
        self.characters
    }
    pub fn byte_count(&self) -> usize {
        if self.mode == Mode::Kanji {
            self.characters * 2
        } else {
            self.logical_bytes().len()
        }
    }
    pub fn payload_units(&self) -> usize {
        match &self.payload {
            Payload::Bytes(bytes) => bytes.len(),
            _ => self.characters,
        }
    }
    pub const fn is_control(&self) -> bool {
        self.mode.is_control()
    }
    pub const fn assignment(&self) -> Option<u32> {
        match self.payload {
            Payload::Eci(assignment) => Some(assignment),
            _ => None,
        }
    }
    pub fn application_indicator(&self) -> Option<&str> {
        match &self.payload {
            Payload::Fnc1Second(value) => Some(value),
            _ => None,
        }
    }
    pub fn application_indicator_codeword(&self) -> Option<u8> {
        let bytes = self.application_indicator()?.as_bytes();
        Some(if bytes.len() == 2 {
            (bytes[0] - b'0') * 10 + bytes[1] - b'0'
        } else {
            bytes[0] + 100
        })
    }
    pub const fn index(&self) -> Option<u8> {
        match self.payload {
            Payload::StructuredAppend { index, .. } => Some(index),
            _ => None,
        }
    }
    pub const fn total(&self) -> Option<u8> {
        match self.payload {
            Payload::StructuredAppend { total, .. } => Some(total),
            _ => None,
        }
    }
    pub const fn parity(&self) -> Option<u8> {
        match self.payload {
            Payload::StructuredAppend { parity, .. } => Some(parity),
            _ => None,
        }
    }
    /// Payload/designator bit length, excluding mode and count headers.
    pub fn data_bit_length(&self) -> usize {
        let count = self.count();
        match self.mode {
            Mode::Numeric => count / 3 * 10 + [0, 4, 7][count % 3],
            Mode::Alphanumeric => count / 2 * 11 + count % 2 * 6,
            Mode::Byte => count * 8,
            Mode::Kanji => count * 13,
            Mode::Eci => match self.assignment() {
                Some(0..=127) => 8,
                Some(128..=16_383) => 16,
                _ => 24,
            },
            Mode::Fnc1 => 0,
            Mode::Fnc1Second => 8,
            Mode::StructuredAppend => 16,
        }
    }
    /// Arithmetic bit length, also available for oversized unencodable payloads.
    pub fn total_bits(&self, version: u8) -> Result<usize> {
        tables::validate_version(version)?;
        let header = if self.is_control() {
            0
        } else {
            usize::from(tables::character_count_bits(version, self.mode)?)
        };
        Ok(4 + header + self.data_bit_length())
    }
    pub fn bits(&self, version: u8) -> Result<Vec<u8>> {
        let length = self.total_bits(version)?;
        self.validate_materialization(version, length)?;
        let mut result = Vec::with_capacity(length);
        self.append_bits(&mut result, version)?;
        Ok(result)
    }
    pub(crate) fn validate_materialization(&self, version: u8, length: usize) -> Result<()> {
        if !self.is_control()
            && self.count() >= (1usize << tables::character_count_bits(version, self.mode)?)
        {
            return Err(Error::new(
                ErrorCode::DataTooLong,
                "Segment count exceeds the selected version's count field",
            ));
        }
        if length > MAX_SINGLE_SYMBOL_DATA_BITS {
            return Err(Error::new(
                ErrorCode::DataTooLong,
                "Segment exceeds maximum single-symbol data capacity",
            ));
        }
        Ok(())
    }
    fn append_bits(&self, result: &mut Vec<u8>, version: u8) -> Result<()> {
        append(result, self.mode.indicator(), 4);
        if !self.is_control() {
            append(
                result,
                self.count() as u32,
                tables::character_count_bits(version, self.mode)?,
            );
        }
        match &self.payload {
            Payload::Eci(assignment) => {
                if *assignment < 128 {
                    append(result, *assignment, 8);
                } else if *assignment < 16_384 {
                    append(result, 0x8000 | assignment, 16);
                } else {
                    append(result, 0xc0_0000 | assignment, 24);
                }
            }
            Payload::Fnc1 => {}
            Payload::Fnc1Second(_) => {
                if let Some(value) = self.application_indicator_codeword() {
                    append(result, u32::from(value), 8);
                }
            }
            Payload::StructuredAppend {
                index,
                total,
                parity,
            } => {
                append(result, u32::from(index - 1), 4);
                append(result, u32::from(total - 1), 4);
                append(result, u32::from(*parity), 8);
            }
            Payload::Bytes(bytes) => {
                for &byte in bytes {
                    append(result, u32::from(byte), 8);
                }
            }
            Payload::Text(text) => match self.mode {
                Mode::Numeric => {
                    for chunk in text.as_bytes().chunks(3) {
                        let value = chunk
                            .iter()
                            .fold(0u32, |value, byte| value * 10 + u32::from(byte - b'0'));
                        append(result, value, [0, 4, 7, 10][chunk.len()]);
                    }
                }
                Mode::Alphanumeric => {
                    for chunk in text.as_bytes().chunks(2) {
                        let value = chunk.iter().fold(0u32, |value, &byte| {
                            value * 45
                                + u32::from(alphanumeric_value(char::from(byte)).unwrap_or(0))
                        });
                        append(result, value, if chunk.len() == 2 { 11 } else { 6 });
                    }
                }
                Mode::Byte => {
                    for &byte in text.as_bytes() {
                        append(result, u32::from(byte), 8);
                    }
                }
                Mode::Kanji => {
                    for character in text.chars() {
                        let value = kanji::value(character).ok_or_else(|| {
                            Error::new(ErrorCode::InvalidMode, "Invalid Kanji character")
                        })?;
                        append(result, u32::from(value), 13);
                    }
                }
                _ => {
                    return Err(Error::new(
                        ErrorCode::InvalidMode,
                        "Text requires a data mode",
                    ));
                }
            },
        }
        Ok(())
    }
}

fn append(bits: &mut Vec<u8>, value: u32, width: u8) {
    for shift in (0..width).rev() {
        bits.push(((value >> shift) & 1) as u8);
    }
}
fn payload_too_long() -> Error {
    Error::new(
        ErrorCode::DataTooLong,
        "Payload exceeds the 1000000-unit resource limit",
    )
}
pub fn alphanumeric_value(character: char) -> Option<u8> {
    if !character.is_ascii() {
        return None;
    }
    ALPHANUMERIC
        .as_bytes()
        .iter()
        .position(|&byte| byte == character as u8)
        .map(|position| position as u8)
}
pub fn validate_text(text: &str) -> Result<usize> {
    if text.len() > MAX_PAYLOAD_UNITS * 4 {
        return Err(payload_too_long());
    }
    let count = text.chars().count();
    if count > MAX_PAYLOAD_UNITS {
        return Err(payload_too_long());
    }
    Ok(count)
}

/// Construct text segments. `None` selects automatic mode. Ties prefer fewer
/// segments, then SpecQR's stable numeric/alphanumeric/Kanji/byte traversal order.
pub fn create_segments(
    text: &str,
    version: u8,
    mode: Option<Mode>,
    optimize: bool,
    allow_kanji: bool,
) -> Result<Vec<Segment>> {
    tables::validate_version(version)?;
    let count = validate_text(text)?;
    if let Some(mode) = mode {
        return Ok(vec![Segment::from_text(mode, text)?]);
    }
    if optimize {
        if count > MAX_SINGLE_SYMBOL_CHARACTERS {
            return Err(Error::new(
                ErrorCode::DataTooLong,
                "Text exceeds maximum single-symbol character capacity",
            ));
        }
        return optimize_segments(text, count, version, allow_kanji);
    }
    let selected = if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()) {
        Mode::Numeric
    } else if !text.is_empty() && text.chars().all(|c| alphanumeric_value(c).is_some()) {
        Mode::Alphanumeric
    } else if !text.is_empty() && allow_kanji && text.chars().all(kanji::can_encode) {
        Mode::Kanji
    } else {
        Mode::Byte
    };
    Ok(vec![Segment::from_text(selected, text)?])
}

/// Validate manual boundaries, control ordering, mutual exclusion, and resources.
/// Repeated ECI changes are supported; other control families cannot be combined.
pub fn validate_segments(segments: &[Segment]) -> Result<()> {
    if segments.len() > MAX_MANUAL_SEGMENTS {
        return Err(Error::new(
            ErrorCode::DataTooLong,
            "Manual segments exceed the 16384-segment resource limit",
        ));
    }
    let mut units = 0usize;
    let mut controls = [false; 4];
    for (position, segment) in segments.iter().enumerate() {
        units = units
            .checked_add(segment.payload_units())
            .ok_or_else(payload_too_long)?;
        if units > MAX_PAYLOAD_UNITS {
            return Err(payload_too_long());
        }
        let control = match segment.mode {
            Mode::Fnc1 => Some(0),
            Mode::Fnc1Second => Some(1),
            Mode::StructuredAppend => Some(2),
            Mode::Eci => Some(3),
            _ => None,
        };
        if let Some(control) = control {
            if control != 3 && (position != 0 || controls[control]) {
                return Err(Error::new(
                    if control == 0 {
                        ErrorCode::InvalidGs1
                    } else {
                        ErrorCode::InvalidMode
                    },
                    "FNC1 and Structured Append headers must occur once at the start",
                ));
            }
            controls[control] = true;
        }
    }
    if controls.iter().filter(|&&present| present).count() > 1 {
        return Err(Error::new(
            if controls[0] {
                ErrorCode::InvalidGs1
            } else {
                ErrorCode::InvalidMode
            },
            "FNC1, FNC1 second, Structured Append, and ECI cannot be combined",
        ));
    }
    Ok(())
}
/// Take an independent owned snapshot of a valid manual sequence.
pub fn normalize_segments(segments: &[Segment]) -> Result<Vec<Segment>> {
    validate_segments(segments)?;
    Ok(segments.to_vec())
}
pub fn bit_length(segments: &[Segment], version: u8) -> Result<usize> {
    tables::validate_version(version)?;
    validate_segments(segments)?;
    segments.iter().try_fold(0usize, |length, segment| {
        length
            .checked_add(segment.total_bits(version)?)
            .ok_or_else(payload_too_long)
    })
}
/// Materialize unpadded bits only after checking all count and allocation bounds.
pub fn bits(segments: &[Segment], version: u8) -> Result<Vec<u8>> {
    let length = bit_length(segments, version)?;
    if length > MAX_SINGLE_SYMBOL_DATA_BITS {
        return Err(Error::new(
            ErrorCode::DataTooLong,
            "Segments exceed maximum single-symbol data capacity",
        ));
    }
    for segment in segments {
        segment.validate_materialization(version, segment.total_bits(version)?)?;
    }
    let mut result = Vec::with_capacity(length);
    for segment in segments {
        segment.append_bits(&mut result, version)?;
    }
    Ok(result)
}

const DATA_MODES: [Mode; 4] = [Mode::Numeric, Mode::Alphanumeric, Mode::Kanji, Mode::Byte];
#[derive(Clone, Copy, Debug)]
struct State {
    key: i8,
    cost: usize,
    segments: usize,
    mode: i8,
    remainder: u8,
    previous: i8,
}
impl State {
    fn better_than(self, other: Self) -> bool {
        self.cost < other.cost || (self.cost == other.cost && self.segments < other.segments)
    }
}
fn initial_states() -> Vec<State> {
    vec![State {
        key: -1,
        cost: 0,
        segments: 0,
        mode: -1,
        remainder: 0,
        previous: -1,
    }]
}
fn advance(
    states: &[State],
    character: char,
    version: u8,
    allow_kanji: bool,
) -> Result<Vec<State>> {
    let eligible = [
        character.is_ascii_digit(),
        alphanumeric_value(character).is_some(),
        allow_kanji && kanji::can_encode(character),
        true,
    ];
    let mut next: Vec<State> = Vec::with_capacity(7);
    for state in states {
        for (mode, &data_mode) in DATA_MODES.iter().enumerate() {
            if !eligible[mode] {
                continue;
            }
            let same = state.mode == mode as i8;
            let remainder = if same { state.remainder } else { 0 };
            let payload = match mode {
                0 => {
                    if remainder == 0 {
                        4
                    } else {
                        3
                    }
                }
                1 => {
                    if remainder == 0 {
                        6
                    } else {
                        5
                    }
                }
                2 => 13,
                _ => character.len_utf8() * 8,
            };
            let next_remainder = match mode {
                0 => (remainder + 1) % 3,
                1 => (remainder + 1) % 2,
                _ => 0,
            };
            let key = mode as i8 * 3 + next_remainder as i8;
            let candidate = State {
                key,
                cost: state.cost
                    + payload
                    + if same {
                        0
                    } else {
                        4 + usize::from(tables::character_count_bits(version, data_mode)?)
                    },
                segments: state.segments + usize::from(!same),
                mode: mode as i8,
                remainder: next_remainder,
                previous: state.key,
            };
            // Updating in place preserves first-insertion order, including exact ties.
            if let Some(current) = next.iter_mut().find(|state| state.key == key) {
                if candidate.better_than(*current) {
                    *current = candidate;
                }
            } else {
                next.push(candidate);
            }
        }
    }
    Ok(next)
}
fn best(states: &[State]) -> Result<State> {
    states
        .iter()
        .copied()
        .reduce(|current, candidate| {
            if candidate.better_than(current) {
                candidate
            } else {
                current
            }
        })
        .ok_or_else(|| Error::new(ErrorCode::InvalidInput, "No encoding state is available"))
}
fn optimize_segments(
    text: &str,
    count: usize,
    version: u8,
    allow_kanji: bool,
) -> Result<Vec<Segment>> {
    if count == 0 {
        return Ok(vec![Segment::utf8("")?]);
    }
    let mut offsets = Vec::with_capacity(count + 1);
    let mut layers = Vec::with_capacity(count + 1);
    layers.push(initial_states());
    for (offset, character) in text.char_indices() {
        offsets.push(offset);
        layers.push(advance(
            &layers[layers.len() - 1],
            character,
            version,
            allow_kanji,
        )?);
    }
    offsets.push(text.len());
    let mut key = best(&layers[count])?.key;
    let mut assignments = vec![0usize; count];
    for index in (1..=count).rev() {
        let state = layers[index]
            .iter()
            .find(|state| state.key == key)
            .ok_or_else(|| Error::new(ErrorCode::InvalidInput, "Missing encoding state"))?;
        assignments[index - 1] = state.mode as usize;
        key = state.previous;
    }
    let mut result = Vec::new();
    let mut start = 0;
    for end in 1..=count {
        if end == count || assignments[end] != assignments[start] {
            result.push(Segment::from_text(
                DATA_MODES[assignments[start]],
                &text[offsets[start]..offsets[end]],
            )?);
            start = end;
        }
    }
    Ok(result)
}

/// Constant-memory exact optimal bit length for successive Unicode-scalar prefixes.
#[derive(Clone, Debug)]
pub struct OptimizationTracker {
    version: u8,
    allow_kanji: bool,
    characters: usize,
    states: Vec<State>,
}
impl OptimizationTracker {
    pub fn new(version: u8, allow_kanji: bool) -> Result<Self> {
        tables::validate_version(version)?;
        Ok(Self {
            version,
            allow_kanji,
            characters: 0,
            states: initial_states(),
        })
    }
    pub fn append(&mut self, character: char) -> Result<usize> {
        if self.characters >= MAX_PAYLOAD_UNITS {
            return Err(payload_too_long());
        }
        let states = advance(&self.states, character, self.version, self.allow_kanji)?;
        let cost = best(&states)?.cost;
        self.states = states;
        self.characters += 1;
        Ok(cost)
    }
}
