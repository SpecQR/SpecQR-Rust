//! Deterministic 2..16-symbol Structured Append and validated reassembly.
//!
//! Public indexes are one-based. Parity is the XOR of the original UTF-8 or raw
//! bytes, not authentication. Text splits preserve Unicode scalar boundaries;
//! manual numeric, alphanumeric, and Kanji segments remain indivisible.

use crate::{
    Error, ErrorCode, Mode, Options, QrCode, Result, Segment, json::Value, segment, tables,
};
use segment::{MAX_MANUAL_SEGMENTS, MAX_PAYLOAD_UNITS};

/// Level of manual split-unit diagnostic detail.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplitUnits {
    #[default]
    Summary,
    Full,
}
/// Whether to include expanded symbol-result diagnostic warnings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SymbolResults {
    #[default]
    Output,
    Diagnostics,
}
/// Diagnostic presentation controls; these never change encoded symbols.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticOptions {
    pub split_units: SplitUnits,
    pub symbol_results: SymbolResults,
}
impl DiagnosticOptions {
    pub const fn full() -> Self {
        Self {
            split_units: SplitUnits::Full,
            symbol_results: SymbolResults::Diagnostics,
        }
    }
}

/// An immutable generated Structured Append set.
#[derive(Clone, Debug)]
pub struct SaResult {
    symbols: Vec<QrCode>,
    total: u8,
    parity: u8,
    input_length: usize,
    byte_length: usize,
    diagnostics: Value,
}
impl SaResult {
    pub fn symbols(&self) -> &[QrCode] {
        &self.symbols
    }
    pub const fn total(&self) -> u8 {
        self.total
    }
    pub const fn parity(&self) -> u8 {
        self.parity
    }
    pub const fn input_length(&self) -> usize {
        self.input_length
    }
    pub const fn byte_length(&self) -> usize {
        self.byte_length
    }
    pub fn diagnostics(&self) -> &Value {
        &self.diagnostics
    }
}

/// Original decoded payload, before concatenation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartData {
    Text(String),
    Bytes(Vec<u8>),
}
impl PartData {
    fn logical_bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Bytes(data) => data,
        }
    }
    fn data_type(&self) -> &'static str {
        match self {
            Self::Text(_) => "string",
            Self::Bytes(_) => "binary",
        }
    }
    fn units(&self) -> Result<usize> {
        match self {
            Self::Text(text) => segment::validate_text(text),
            Self::Bytes(bytes) => {
                if bytes.len() > MAX_PAYLOAD_UNITS {
                    return Err(too_long("Part exceeds the payload resource limit"));
                }
                Ok(bytes.len())
            }
        }
    }
}
impl From<String> for PartData {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}
impl From<&str> for PartData {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}
impl From<Vec<u8>> for PartData {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Bytes(bytes)
    }
}
impl From<&[u8]> for PartData {
    fn from(bytes: &[u8]) -> Self {
        Self::Bytes(bytes.to_vec())
    }
}

/// A validated, owned decoded Structured Append part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    index: u8,
    total: u8,
    parity: u8,
    data: PartData,
}
impl Part {
    pub fn new(index: u8, total: u8, parity: u8, data: PartData) -> Result<Self> {
        validate_metadata(index, total)?;
        data.units()?;
        Ok(Self {
            index,
            total,
            parity,
            data,
        })
    }
    pub const fn index(&self) -> u8 {
        self.index
    }
    pub const fn total(&self) -> u8 {
        self.total
    }
    pub const fn parity(&self) -> u8 {
        self.parity
    }
    pub fn data(&self) -> &PartData {
        &self.data
    }
}
/// Metadata for a reassembled part, sorted by one-based index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartInfo {
    index: u8,
    total: u8,
    parity: u8,
    data_type: &'static str,
    byte_length: usize,
}
impl PartInfo {
    pub const fn index(&self) -> u8 {
        self.index
    }
    pub const fn total(&self) -> u8 {
        self.total
    }
    pub const fn parity(&self) -> u8 {
        self.parity
    }
    pub const fn data_type(&self) -> &'static str {
        self.data_type
    }
    pub const fn byte_length(&self) -> usize {
        self.byte_length
    }
}
/// Immutable reassembled data and validated parity/ordering diagnostics.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeResult {
    data: PartData,
    total: u8,
    parity: u8,
    parts: Vec<PartInfo>,
    diagnostics: Value,
}
impl MergeResult {
    pub fn data(&self) -> &PartData {
        &self.data
    }
    pub fn text(&self) -> Option<&str> {
        if let PartData::Text(text) = &self.data {
            Some(text)
        } else {
            None
        }
    }
    pub fn bytes(&self) -> Option<&[u8]> {
        if let PartData::Bytes(bytes) = &self.data {
            Some(bytes)
        } else {
            None
        }
    }
    pub const fn total(&self) -> u8 {
        self.total
    }
    pub const fn parity(&self) -> u8 {
        self.parity
    }
    pub fn parts(&self) -> &[PartInfo] {
        &self.parts
    }
    pub fn diagnostics(&self) -> &Value {
        &self.diagnostics
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}
fn mode_error(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidMode, message)
}
fn too_long(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::DataTooLong, message)
}
fn validate_metadata(index: u8, total: u8) -> Result<()> {
    if !(2..=16).contains(&total) || index == 0 || index > total {
        return Err(invalid(
            "Structured Append requires 1 <= index <= total and 2 <= total <= 16",
        ));
    }
    Ok(())
}
fn xor(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |a, &b| a ^ b)
}

/// Canonical original UTF-8 byte XOR; the empty string has parity zero.
pub fn calculate_parity(text: &str) -> Result<u8> {
    segment::validate_text(text)?;
    Ok(xor(text.as_bytes()))
}
/// Canonical raw-byte XOR; the empty input has parity zero.
pub fn calculate_bytes_parity(bytes: &[u8]) -> Result<u8> {
    if bytes.len() > MAX_PAYLOAD_UNITS {
        return Err(too_long("Payload exceeds the 1000000-unit resource limit"));
    }
    Ok(xor(bytes))
}
fn validate_manual(segments: &[Segment], limit: usize) -> Result<()> {
    if segments.is_empty() {
        return Err(invalid(
            "Structured Append requires non-empty data segments",
        ));
    }
    if segments.len() > MAX_MANUAL_SEGMENTS {
        return Err(too_long(
            "Manual segments exceed the 16384-segment resource limit",
        ));
    }
    if segments.len() > limit {
        return Err(too_long(
            "Input segments exceed the selected Structured Append capacity",
        ));
    }
    let mut units = 0;
    for segment in segments {
        if segment.mode() == Mode::Fnc1 {
            return Err(Error::new(
                ErrorCode::InvalidGs1,
                "Structured Append cannot be combined with manual FNC1 first position",
            ));
        }
        if segment.is_control() {
            return Err(mode_error(format!(
                "Structured Append cannot be combined with manual {} controls",
                segment.mode()
            )));
        }
        let count = segment.payload_units();
        if count == 0 {
            return Err(invalid(
                "Structured Append segments must contain non-empty data",
            ));
        }
        units += count;
        if units > limit {
            return Err(too_long(
                "Input segments exceed the selected Structured Append capacity",
            ));
        }
    }
    Ok(())
}
/// XOR canonical original UTF-8/raw bytes in manual data segments.
pub fn calculate_segments_parity(segments: &[Segment]) -> Result<u8> {
    validate_manual(segments, MAX_PAYLOAD_UNITS)?;
    Ok(segments
        .iter()
        .fold(0, |a, segment| a ^ xor(segment.logical_bytes())))
}
fn validate_options(options: &Options, maximum: u8, manual: bool) -> Result<()> {
    options.validate()?;
    if !(2..=16).contains(&maximum) {
        return Err(mode_error("max_symbols must be from 2 to 16"));
    }
    if options.structured_append.is_some() {
        return Err(mode_error("Structured Append generation owns its header"));
    }
    if options.eci.is_some() || options.fnc1_second.is_some() {
        return Err(mode_error(
            "Structured Append cannot be combined with ECI or FNC1 second position",
        ));
    }
    if options.gs1 {
        return Err(Error::new(
            ErrorCode::InvalidGs1,
            "Structured Append cannot be combined with GS1",
        ));
    }
    if options.boost_ecc {
        return Err(mode_error(
            "Structured Append does not support error-correction boosting",
        ));
    }
    if manual && (options.mode.is_some() || !options.optimize_segments) {
        return Err(mode_error(
            "Manual Structured Append preserves caller segment modes",
        ));
    }
    Ok(())
}
fn capacity(options: &Options, version: u8) -> Result<usize> {
    Ok(tables::data_codeword_count(version, options.ecc)? * 8)
}
fn maximum_version(options: &Options) -> u8 {
    options.version.unwrap_or(options.max_version)
}
fn unit_budget(options: &Options, maximum: u8) -> Result<usize> {
    Ok(usize::from(maximum) * ((capacity(options, maximum_version(options))? - 20) * 3 / 10))
}
fn numeric_bits(length: usize) -> usize {
    length / 3 * 10 + [0, 4, 7][length % 3]
}
fn payload_bits(mode: Mode, length: usize, byte_length: usize) -> usize {
    match mode {
        Mode::Numeric => numeric_bits(length),
        Mode::Alphanumeric => length / 2 * 11 + length % 2 * 6,
        Mode::Kanji => length * 13,
        _ => byte_length * 8,
    }
}
fn segment_bits(mode: Mode, length: usize, bytes: usize, version: u8) -> Result<usize> {
    let width = tables::character_count_bits(version, mode)?;
    let count = if mode == Mode::Byte { bytes } else { length };
    if count >= (1 << width) {
        return Ok(usize::MAX / 1024);
    }
    Ok(4 + usize::from(width) + payload_bits(mode, length, bytes))
}

// One byte offset per 64 Unicode scalars, so sparse indexing is bounded well
// below payload size. Rust str makes invalid Unicode and surrogate splits
// unrepresentable without using unsafe code.
struct TextIndex<'a> {
    text: &'a str,
    length: usize,
    offsets: Vec<usize>,
}
impl<'a> TextIndex<'a> {
    fn new(text: &'a str) -> Result<Self> {
        let length = segment::validate_text(text)?;
        let mut offsets = Vec::with_capacity(length / 64 + 1);
        for (i, (offset, _)) in text.char_indices().enumerate() {
            if i % 64 == 0 {
                offsets.push(offset);
            }
        }
        if length % 64 == 0 {
            offsets.push(text.len());
        }
        Ok(Self {
            text,
            length,
            offsets,
        })
    }
    fn offset(&self, scalar: usize) -> usize {
        let checkpoint = scalar / 64;
        let offset = self.offsets[checkpoint];
        let rest = scalar % 64;
        if rest == 0 {
            offset
        } else {
            offset
                + self.text[offset..]
                    .char_indices()
                    .nth(rest)
                    .map_or(self.text.len() - offset, |(at, _)| at)
        }
    }
    fn slice(&self, start: usize, length: usize) -> &'a str {
        &self.text[self.offset(start)..self.offset(start + length)]
    }
    fn byte_length(&self, start: usize, length: usize) -> usize {
        self.offset(start + length) - self.offset(start)
    }
}
struct Descriptor<'a> {
    segment: &'a Segment,
    source_index: usize,
    split_start: usize,
    split_count: usize,
    byte_start: usize,
    text: Option<TextIndex<'a>>,
}
impl Descriptor<'_> {
    fn local_byte_offset(&self, start: usize) -> usize {
        if self.segment.mode() != Mode::Byte {
            0
        } else {
            self.text.as_ref().map_or(start, |text| text.offset(start))
        }
    }
    fn local_byte_length(&self, start: usize, length: usize) -> usize {
        if self.segment.mode() != Mode::Byte {
            self.segment.logical_bytes().len()
        } else {
            self.text
                .as_ref()
                .map_or(length, |text| text.byte_length(start, length))
        }
    }
    fn slice(&self, start: usize, length: usize) -> Result<Segment> {
        if self.segment.mode() != Mode::Byte {
            Ok(self.segment.clone())
        } else if let Some(text) = &self.text {
            Segment::utf8(text.slice(start, length))
        } else {
            Segment::bytes(&self.segment.logical_bytes()[start..start + length])
        }
    }
}
enum SourceData<'a> {
    Text(TextIndex<'a>),
    Bytes(&'a [u8]),
    Manual(Vec<Descriptor<'a>>),
}
struct Source<'a> {
    data: SourceData<'a>,
    length: usize,
    input_length: usize,
    byte_length: usize,
    parity: u8,
}
enum ChunkData {
    Text(String),
    Bytes(Vec<u8>),
    Manual(Vec<Segment>),
}
struct Chunk {
    data: ChunkData,
    offsets: Value,
}
impl<'a> Source<'a> {
    fn text(text: &'a str, options: &Options, maximum: u8) -> Result<Self> {
        let budget = unit_budget(options, maximum)?;
        if text.len() > 4 * budget {
            return Err(too_long(
                "Input exceeds the selected Structured Append capacity",
            ));
        }
        let index = TextIndex::new(text)?;
        if index.length > budget {
            return Err(too_long(
                "Input exceeds the selected Structured Append capacity",
            ));
        }
        if index.length == 0 {
            return Err(invalid(
                "Structured Append requires at least two non-empty symbols",
            ));
        }
        if let Some(mode) = options.mode {
            Segment::from_text(mode, text)?;
        }
        let source = Self {
            length: index.length,
            input_length: index.length,
            byte_length: text.len(),
            parity: xor(text.as_bytes()),
            data: SourceData::Text(index),
        };
        source.preflight(options, maximum)?;
        Ok(source)
    }
    fn bytes(bytes: &'a [u8], options: &Options, maximum: u8) -> Result<Self> {
        if !matches!(options.mode, None | Some(Mode::Byte)) {
            return Err(mode_error("Binary input only supports byte mode"));
        }
        if bytes.len() > unit_budget(options, maximum)? {
            return Err(too_long(
                "Input exceeds the selected Structured Append capacity",
            ));
        }
        if bytes.is_empty() {
            return Err(invalid(
                "Structured Append requires at least two non-empty symbols",
            ));
        }
        let source = Self {
            length: bytes.len(),
            input_length: bytes.len(),
            byte_length: bytes.len(),
            parity: xor(bytes),
            data: SourceData::Bytes(bytes),
        };
        source.preflight(options, maximum)?;
        Ok(source)
    }
    fn preflight(&self, options: &Options, maximum: u8) -> Result<()> {
        let version = maximum_version(options);
        let mode = if matches!(self.data, SourceData::Bytes(_)) {
            Some(Mode::Byte)
        } else {
            options.mode
        };
        let width = if let Some(mode) = mode {
            tables::character_count_bits(version, mode)?
        } else {
            [Mode::Numeric, Mode::Alphanumeric, Mode::Byte, Mode::Kanji]
                .into_iter()
                .map(|mode| tables::character_count_bits(version, mode))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .min()
                .unwrap()
        };
        let required = mode.map_or_else(
            || numeric_bits(self.length),
            |mode| payload_bits(mode, self.length, self.byte_length),
        );
        if required
            > usize::from(maximum)
                * capacity(options, version)?.saturating_sub(24 + usize::from(width))
        {
            return Err(too_long(
                "Input exceeds the selected Structured Append capacity",
            ));
        }
        Ok(())
    }
    fn manual(segments: &'a [Segment], options: &Options, maximum: u8) -> Result<Self> {
        validate_manual(
            segments,
            unit_budget(options, maximum)?.min(MAX_PAYLOAD_UNITS),
        )?;
        let (mut length, mut byte_length, mut parity, mut bits) = (0, 0, 0, 0);
        let version = maximum_version(options);
        let mut descriptors = Vec::with_capacity(segments.len());
        for (source_index, segment) in segments.iter().enumerate() {
            let text = segment.text().map(TextIndex::new).transpose()?;
            let split_count = if segment.mode() == Mode::Byte {
                segment.payload_units()
            } else {
                1
            };
            descriptors.push(Descriptor {
                segment,
                source_index,
                split_start: length,
                split_count,
                byte_start: byte_length,
                text,
            });
            length += split_count;
            byte_length += segment.logical_bytes().len();
            parity ^= xor(segment.logical_bytes());
            bits += 4
                + usize::from(tables::character_count_bits(version, segment.mode())?)
                + payload_bits(
                    segment.mode(),
                    segment.character_count(),
                    segment.logical_bytes().len(),
                );
        }
        if bits > usize::from(maximum) * (capacity(options, version)? - 20) {
            return Err(too_long(
                "Input segments exceed the selected Structured Append capacity",
            ));
        }
        Ok(Self {
            data: SourceData::Manual(descriptors),
            length,
            input_length: segments.len(),
            byte_length,
            parity,
        })
    }
    fn bits(&self, start: usize, length: usize, options: &Options, version: u8) -> Result<usize> {
        let available = capacity(options, version)?;
        match &self.data {
            SourceData::Bytes(_) => Ok(20 + segment_bits(Mode::Byte, length, length, version)?),
            SourceData::Text(text) => {
                if numeric_bits(length) > available - 20 {
                    return Ok(usize::MAX / 1024);
                }
                if let Some(mode) = options.mode {
                    return Ok(
                        20 + segment_bits(mode, length, text.byte_length(start, length), version)?
                    );
                }
                if options.optimize_segments {
                    let mut tracker = segment::OptimizationTracker::new(version, true)?;
                    let mut bits = 0;
                    for character in text.slice(start, length).chars() {
                        bits = tracker.append(character)?;
                        if bits + 20 > available {
                            break;
                        }
                    }
                    Ok(bits + 20)
                } else {
                    Ok(20
                        + segment::bit_length(
                            &segment::create_segments(
                                text.slice(start, length),
                                version,
                                None,
                                false,
                                true,
                            )?,
                            version,
                        )?)
                }
            }
            SourceData::Manual(descriptors) => {
                let end = start + length;
                let mut bits = 20;
                for d in descriptors {
                    let finish = d.split_start + d.split_count;
                    if finish <= start {
                        continue;
                    }
                    if d.split_start >= end {
                        break;
                    }
                    let local_start = start.max(d.split_start) - d.split_start;
                    let local_length = end.min(finish) - (d.split_start + local_start);
                    bits += segment_bits(
                        d.segment.mode(),
                        if d.segment.mode() == Mode::Byte {
                            local_length
                        } else {
                            d.segment.character_count()
                        },
                        d.local_byte_length(local_start, local_length),
                        version,
                    )?;
                    if bits > available {
                        break;
                    }
                }
                Ok(bits)
            }
        }
    }
    fn largest_prefix(
        &self,
        start: usize,
        maximum: usize,
        options: &Options,
        version: u8,
    ) -> Result<usize> {
        if let SourceData::Text(text) = &self.data {
            if options.mode.is_none() && options.optimize_segments {
                let mut tracker = segment::OptimizationTracker::new(version, true)?;
                let available = capacity(options, version)? - 20;
                for (i, character) in text.slice(start, maximum).chars().enumerate() {
                    if tracker.append(character)? > available {
                        return Ok(i);
                    }
                }
                return Ok(maximum);
            }
        }
        let (mut low, mut high, mut best) = (1, maximum, 0);
        let available = capacity(options, version)?;
        while low <= high {
            let length = low + (high - low) / 2;
            if self.bits(start, length, options, version)? <= available {
                best = length;
                low = length + 1;
            } else {
                high = length - 1;
            }
        }
        Ok(best)
    }
    fn chunk(&self, start: usize, length: usize) -> Result<Chunk> {
        match &self.data {
            SourceData::Text(text) => Ok(Chunk {
                data: ChunkData::Text(text.slice(start, length).to_owned()),
                offsets: Value::object([
                    ("inputStart", start.into()),
                    ("inputLength", length.into()),
                    ("byteStart", text.offset(start).into()),
                    ("byteLength", text.byte_length(start, length).into()),
                ]),
            }),
            SourceData::Bytes(bytes) => Ok(Chunk {
                data: ChunkData::Bytes(bytes[start..start + length].to_vec()),
                offsets: Value::object([
                    ("inputStart", start.into()),
                    ("inputLength", length.into()),
                    ("byteStart", start.into()),
                    ("byteLength", length.into()),
                ]),
            }),
            SourceData::Manual(descriptors) => {
                let (mut first, mut last, mut byte_start, mut byte_length) = (None, 0, 0, 0);
                let mut segments = Vec::new();
                let end = start + length;
                for d in descriptors {
                    let finish = d.split_start + d.split_count;
                    if finish <= start {
                        continue;
                    }
                    if d.split_start >= end {
                        break;
                    }
                    let local_start = start.max(d.split_start) - d.split_start;
                    let local_length = end.min(finish) - (d.split_start + local_start);
                    if first.is_none() {
                        first = Some(d.source_index);
                        byte_start = d.byte_start + d.local_byte_offset(local_start);
                    }
                    last = d.source_index + 1;
                    byte_length += d.local_byte_length(local_start, local_length);
                    segments.push(d.slice(local_start, local_length)?);
                }
                Ok(Chunk {
                    data: ChunkData::Manual(segments),
                    offsets: Value::object([
                        ("sourceSegmentStart", first.into()),
                        ("sourceSegmentEnd", last.into()),
                        ("splitUnitStart", start.into()),
                        ("splitUnitLength", length.into()),
                        ("byteStart", byte_start.into()),
                        ("byteLength", byte_length.into()),
                    ]),
                })
            }
        }
    }
    fn full_detail(&self) -> Value {
        let mut result = Vec::with_capacity(self.length);
        if let SourceData::Manual(descriptors) = &self.data {
            for d in descriptors {
                for unit in 0..d.split_count {
                    result.push(Value::object([
                        ("sourceSegmentIndex", d.source_index.into()),
                        ("mode", d.segment.mode().as_str().into()),
                        (
                            "unitStart",
                            (if d.segment.mode() == Mode::Byte {
                                unit
                            } else {
                                0
                            })
                            .into(),
                        ),
                        (
                            "unitLength",
                            (if d.segment.mode() == Mode::Byte {
                                1
                            } else {
                                d.segment.character_count()
                            })
                            .into(),
                        ),
                        (
                            "byteStart",
                            (d.byte_start + d.local_byte_offset(unit)).into(),
                        ),
                        ("byteLength", d.local_byte_length(unit, 1).into()),
                    ]));
                }
            }
        }
        Value::Array(result)
    }
}

struct Selection {
    version: u8,
    ranges: Vec<(usize, usize)>,
}
fn select(source: &Source<'_>, options: &Options, maximum: u8) -> Result<Selection> {
    let first = options.version.unwrap_or(options.min_version);
    let last = maximum_version(options);
    let mut saw_too_long = false;
    for version in first..=last {
        if source.bits(0, source.length, options, version)? <= capacity(options, version)? {
            continue;
        }
        let mut ranges = Vec::new();
        let mut start = 0;
        while start < source.length && ranges.len() < usize::from(maximum) {
            let limit = source.length - start - usize::from(ranges.is_empty());
            let length = source.largest_prefix(start, limit, options, version)?;
            if length == 0 {
                break;
            }
            ranges.push((start, length));
            start += length;
        }
        if start == source.length && ranges.len() >= 2 {
            return Ok(Selection { version, ranges });
        }
        saw_too_long = true;
    }
    if saw_too_long {
        Err(too_long(format!(
            "Input cannot be split into {maximum} or fewer Structured Append symbols in the selected version range"
        )))
    } else {
        Err(invalid(
            "Input fits in one symbol in the selected version range; use ordinary generation or a low-level Structured Append header",
        ))
    }
}
fn generate_source(
    source: Source<'_>,
    options: &Options,
    maximum: u8,
    detail: &DiagnosticOptions,
) -> Result<SaResult> {
    let selection = select(&source, options, maximum)?;
    let version = selection.version;
    let total = selection.ranges.len() as u8;
    let mut symbols = Vec::with_capacity(usize::from(total));
    let mut diagnostics = Vec::with_capacity(usize::from(total));
    let available = capacity(options, version)?;
    for (i, &(start, length)) in selection.ranges.iter().enumerate() {
        let chunk = source.chunk(start, length)?;
        let mut fixed = options.clone();
        fixed.version = Some(version);
        fixed.min_version = version;
        fixed.max_version = version;
        fixed.structured_append = Some(Segment::structured_append(
            i as u8 + 1,
            total,
            source.parity,
        )?);
        let result = match chunk.data {
            ChunkData::Text(text) => crate::generate(&text, &fixed)?,
            ChunkData::Bytes(bytes) => crate::generate_bytes(&bytes, &fixed)?,
            ChunkData::Manual(segments) => crate::generate_segments(&segments, &fixed)?,
        };
        let bits = segment::bit_length(result.segments(), version)?;
        let mut symbol = Value::object([
            ("index", (i + 1).into()),
            ("total", total.into()),
            ("parity", source.parity.into()),
            ("sequenceIndex", i.into()),
            ("sequenceTotal", (total - 1).into()),
            (
                "sequenceIndicator",
                ((i << 4) | usize::from(total - 1)).into(),
            ),
            ("version", version.into()),
            ("errorCorrectionLevel", result.ecc().as_str().into()),
            ("dataBitLength", bits.into()),
            ("capacityBits", available.into()),
            ("remainingBits", (available - bits).into()),
            ("maskPattern", result.mask().into()),
        ]);
        for (key, value) in chunk
            .offsets
            .as_object()
            .expect("chunk offsets are objects")
        {
            symbol.insert(key, value.clone())?;
        }
        diagnostics.push(symbol);
        symbols.push(result);
    }
    let mut warnings = Vec::new();
    if total == maximum {
        warnings.push(Value::object([
            ("code", "STRUCTURED_APPEND_MAX_SYMBOLS_NEAR_LIMIT".into()), ("severity", "info".into()),
            ("message", "The generated Structured Append set uses the configured maximum number of symbols.".into()),
            ("details", Value::object([("total", total.into()), ("maxSymbols", maximum.into())]))]));
    }
    if detail.symbol_results == SymbolResults::Diagnostics {
        warnings.push(Value::object([
            ("code", "STRUCTURED_APPEND_DECODER_SUPPORT_VARIES".into()),
            ("severity", "info".into()),
            (
                "message",
                "Decoder APIs vary in how they expose Structured Append set metadata.".into(),
            ),
            ("details", Value::object([("total", total.into())])),
        ]));
    }
    let strategy = if options.version.is_some() {
        "fixed"
    } else {
        "auto-minimum"
    };
    let manual = matches!(source.data, SourceData::Manual(_));
    let input_label = if manual { "manual segments" } else { "payload" };
    let reason = if options.version.is_some() {
        format!("Version {version} was requested explicitly.")
    } else {
        format!(
            "Version {version} is the smallest version in {}..{} that can split the {input_label} into {total} Structured Append symbols at error correction {}.",
            options.min_version, options.max_version, options.ecc
        )
    };
    let mut summary = Value::object([
        ("version", version.into()),
        ("errorCorrectionLevel", options.ecc.as_str().into()),
        ("versionSelection", strategy.into()),
        ("versionSelectionReason", reason.into()),
        ("total", total.into()),
        ("parity", source.parity.into()),
        ("byteLength", source.byte_length.into()),
        ("inputLength", source.input_length.into()),
        ("maxSymbols", maximum.into()),
        (
            "splitStrategy",
            (if manual {
                "segment-boundary-byte-chunk"
            } else {
                "greedy-largest-fitting"
            })
            .into(),
        ),
        ("symbols", Value::Array(diagnostics)),
        ("warnings", Value::Array(warnings)),
    ]);
    if manual {
        summary.insert("segmentCount", source.input_length)?;
        summary.insert("splitUnitCount", source.length)?;
        summary.insert(
            "splitUnitsDetail",
            if detail.split_units == SplitUnits::Full {
                "full"
            } else {
                "summary"
            },
        )?;
        if detail.split_units == SplitUnits::Full {
            summary.insert("splitUnits", source.full_detail())?;
        }
    }
    Ok(SaResult {
        symbols,
        total,
        parity: source.parity,
        input_length: source.input_length,
        byte_length: source.byte_length,
        diagnostics: summary,
    })
}

/// Generate a deterministic set with Unicode-scalar text boundaries. Input that
/// fits one SA-header-bearing symbol is rejected; use ordinary generation.
pub fn generate(
    text: &str,
    options: &Options,
    max_symbols: u8,
    detail: &DiagnosticOptions,
) -> Result<SaResult> {
    validate_options(options, max_symbols, false)?;
    generate_source(
        Source::text(text, options, max_symbols)?,
        options,
        max_symbols,
        detail,
    )
}
/// Generate a deterministic raw-byte Structured Append set.
pub fn generate_bytes(
    bytes: &[u8],
    options: &Options,
    max_symbols: u8,
    detail: &DiagnosticOptions,
) -> Result<SaResult> {
    validate_options(options, max_symbols, false)?;
    generate_source(
        Source::bytes(bytes, options, max_symbols)?,
        options,
        max_symbols,
        detail,
    )
}
/// Preserve manual modes and boundaries, splitting only byte segments. Textual
/// byte segments split on Unicode scalars; binary byte segments split by byte.
pub fn generate_segments(
    segments: &[Segment],
    options: &Options,
    max_symbols: u8,
    detail: &DiagnosticOptions,
) -> Result<SaResult> {
    validate_options(options, max_symbols, true)?;
    generate_source(
        Source::manual(segments, options, max_symbols)?,
        options,
        max_symbols,
        detail,
    )
}

/// Reassemble a complete same-type set, validating metadata, ordering, missing
/// and duplicate indexes, cumulative resources, and canonical byte parity.
pub fn merge(parts: &[Part]) -> Result<MergeResult> {
    if parts.is_empty() || parts.len() > 16 {
        return Err(invalid("Structured Append requires 2..16 parts"));
    }
    let first = &parts[0];
    let total = first.total;
    let parity = first.parity;
    let kind = first.data.data_type();
    let mut ordered = [None; 16];
    let (mut units, mut bytes, mut actual) = (0, 0, 0);
    for part in parts {
        validate_metadata(part.index, part.total)?;
        if part.total != total {
            return Err(invalid("Structured Append total mismatch"));
        }
        if part.parity != parity {
            return Err(invalid("Structured Append parity mismatch"));
        }
        if part.data.data_type() != kind {
            return Err(invalid(
                "Structured Append parts must not mix string and binary data",
            ));
        }
        if ordered[usize::from(part.index - 1)].is_some() {
            return Err(invalid(format!(
                "Structured Append duplicate index {}",
                part.index
            )));
        }
        units += part.data.units()?;
        if units > MAX_PAYLOAD_UNITS {
            return Err(too_long(
                "Merged payload exceeds the 1000000-unit resource limit",
            ));
        }
        bytes += part.data.logical_bytes().len();
        actual ^= xor(part.data.logical_bytes());
        ordered[usize::from(part.index - 1)] = Some(part);
    }
    let missing = (0..total)
        .filter(|&i| ordered[usize::from(i)].is_none())
        .map(|i| (i + 1).to_string())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(invalid(format!(
            "Structured Append parts are missing indexes: {}",
            missing.join(", ")
        )));
    }
    if parts.len() != usize::from(total) {
        return Err(invalid("Structured Append part count does not match total"));
    }
    if actual != parity {
        return Err(invalid(format!(
            "Structured Append parity check failed: expected {parity}, got {actual}"
        )));
    }
    let mut output = Vec::with_capacity(bytes);
    let mut descriptions = Vec::with_capacity(usize::from(total));
    for part in ordered[..usize::from(total)].iter().flatten() {
        output.extend_from_slice(part.data.logical_bytes());
        descriptions.push(PartInfo {
            index: part.index,
            total,
            parity,
            data_type: kind,
            byte_length: part.data.logical_bytes().len(),
        });
    }
    let data = if kind == "string" {
        PartData::Text(String::from_utf8(output).map_err(|_| invalid("Merged data is not UTF-8"))?)
    } else {
        PartData::Bytes(output)
    };
    let diagnostics = Value::object([
        ("partCount", total.into()),
        ("total", total.into()),
        ("parity", parity.into()),
        ("dataType", kind.into()),
        ("byteLength", bytes.into()),
        ("missing", Value::Array(Vec::new())),
        ("duplicate", Value::Array(Vec::new())),
        (
            "parityCheck",
            Value::object([
                ("expected", parity.into()),
                ("actual", actual.into()),
                ("matches", true.into()),
            ]),
        ),
    ]);
    Ok(MergeResult {
        data,
        total,
        parity,
        parts: descriptions,
        diagnostics,
    })
}
