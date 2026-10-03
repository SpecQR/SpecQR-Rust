use crate::{
    Ecc, Error, ErrorCode, Mode, Result, Segment, core, gs1, json::Value, render, segment, tables,
};
use render::RenderOptions;

/// Encoding and rendering configuration. Validation occurs at every public entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub ecc: Ecc,
    /// `None` selects automatic mixed-mode segmentation.
    pub mode: Option<Mode>,
    pub version: Option<u8>,
    pub min_version: u8,
    pub max_version: u8,
    pub mask: Option<u8>,
    pub optimize_segments: bool,
    pub boost_ecc: bool,
    pub eci: Option<u32>,
    pub gs1: bool,
    pub fnc1_second: Option<String>,
    pub structured_append: Option<Segment>,
    pub render: RenderOptions,
    pub print_dpi: Option<f64>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            ecc: Ecc::M,
            mode: None,
            version: None,
            min_version: 1,
            max_version: 40,
            mask: None,
            optimize_segments: true,
            boost_ecc: false,
            eci: None,
            gs1: false,
            fnc1_second: None,
            structured_append: None,
            render: RenderOptions::default(),
            print_dpi: None,
        }
    }
}
impl Options {
    /// Check all control, version and geometry constraints before doing work.
    pub fn validate(&self) -> Result<()> {
        tables::validate_version(self.min_version)?;
        tables::validate_version(self.max_version)?;
        if self.min_version > self.max_version {
            return Err(invalid(
                ErrorCode::InvalidVersion,
                "min_version must not exceed max_version",
            ));
        }
        if let Some(v) = self.version {
            tables::validate_version(v)?;
        }
        if self.mask.is_some_and(|m| m > 7) {
            return Err(invalid(ErrorCode::InvalidInput, "mask must be 0..7"));
        }
        if self.mode.is_some_and(|m| {
            !matches!(
                m,
                Mode::Numeric | Mode::Alphanumeric | Mode::Byte | Mode::Kanji
            )
        }) {
            return Err(invalid(
                ErrorCode::InvalidMode,
                "Encoding mode must be a data mode",
            ));
        }
        if let Some(e) = self.eci {
            Segment::eci(e)?;
        }
        if let Some(f) = &self.fnc1_second {
            Segment::fnc1_second(f)?;
        }
        if self
            .structured_append
            .as_ref()
            .is_some_and(|s| s.mode() != Mode::StructuredAppend)
        {
            return Err(invalid(
                ErrorCode::InvalidMode,
                "structured_append requires an append header",
            ));
        }
        if usize::from(self.eci.is_some())
            + usize::from(self.gs1)
            + usize::from(self.fnc1_second.is_some())
            + usize::from(self.structured_append.is_some())
            > 1
        {
            return Err(invalid(
                ErrorCode::InvalidMode,
                "ECI, GS1, FNC1 second and Structured Append cannot be combined",
            ));
        }
        if self.render.scale == 0 {
            return Err(invalid(ErrorCode::InvalidInput, "scale must be positive"));
        }
        if self.render.foreground.len() > render::SVG_CHARACTER_BUDGET / 12
            || self.render.background.len() > render::SVG_CHARACTER_BUDGET / 12
        {
            return Err(invalid(
                ErrorCode::InvalidColor,
                "Colors exceed resource budget",
            ));
        }
        if let Some(dpi) = self.print_dpi {
            let extent =
                (177.0 + 2.0 * f64::from(self.render.margin)) * f64::from(self.render.scale) / dpi
                    * 25.4;
            if !dpi.is_finite() || dpi <= 0.0 || !extent.is_finite() {
                return Err(invalid(
                    ErrorCode::InvalidInput,
                    "print_dpi must be finite and positive with finite print geometry",
                ));
            }
        }
        Ok(())
    }
}
fn invalid(code: ErrorCode, message: &str) -> Error {
    Error::new(code, message)
}
/// Immutable planning result. Planning builds neither ECC nor a matrix.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    ok: bool,
    version: Option<u8>,
    capacity_version: u8,
    ecc: Ecc,
    requested_ecc: Ecc,
    required_bits: usize,
    capacity_bits: usize,
    segments: Vec<Segment>,
    diagnostics: Value,
}
impl Plan {
    pub fn ok(&self) -> bool {
        self.ok
    }
    pub fn version(&self) -> Option<u8> {
        self.version
    }
    pub fn capacity_version(&self) -> u8 {
        self.capacity_version
    }
    pub fn ecc(&self) -> Ecc {
        self.ecc
    }
    pub fn requested_ecc(&self) -> Ecc {
        self.requested_ecc
    }
    pub fn boosted_ecc(&self) -> bool {
        self.ecc != self.requested_ecc
    }
    pub fn required_bits(&self) -> usize {
        self.required_bits
    }
    pub fn data_bit_length(&self) -> usize {
        self.required_bits
    }
    pub fn capacity_bits(&self) -> usize {
        self.capacity_bits
    }
    pub fn remaining_bits(&self) -> i64 {
        self.capacity_bits as i64 - self.required_bits as i64
    }
    pub fn overflow_bits(&self) -> usize {
        self.required_bits.saturating_sub(self.capacity_bits)
    }
    pub fn capacity_utilization(&self) -> f64 {
        self.required_bits as f64 / self.capacity_bits as f64
    }
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
    pub fn diagnostics(&self) -> &Value {
        &self.diagnostics
    }
}
/// Immutable QR symbol. Borrowed slices prevent accidental mutation of metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct QrCode {
    matrix: Vec<Vec<bool>>,
    version: u8,
    mask: u8,
    ecc: Ecc,
    data: Vec<u8>,
    codewords: Vec<u8>,
    segments: Vec<Segment>,
    diagnostics: Value,
    options: Options,
}
impl QrCode {
    pub fn matrix(&self) -> &[Vec<bool>] {
        &self.matrix
    }
    pub fn version(&self) -> u8 {
        self.version
    }
    pub fn size(&self) -> usize {
        self.matrix.len()
    }
    pub fn mask(&self) -> u8 {
        self.mask
    }
    pub fn ecc(&self) -> Ecc {
        self.ecc
    }
    pub fn data_codewords(&self) -> &[u8] {
        &self.data
    }
    pub fn codewords(&self) -> &[u8] {
        &self.codewords
    }
    pub fn error_correction_codewords(&self) -> &[u8] {
        &self.codewords[self.data.len()..]
    }
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
    pub fn diagnostics(&self) -> &Value {
        &self.diagnostics
    }
    pub fn options(&self) -> &Options {
        &self.options
    }
    pub fn module(&self, x: usize, y: usize) -> Result<bool> {
        self.matrix
            .get(y)
            .and_then(|row| row.get(x))
            .copied()
            .ok_or_else(|| invalid(ErrorCode::InvalidInput, "Module coordinates out of bounds"))
    }
    pub fn to_svg(&self) -> Result<String> {
        render::to_svg(&self.matrix, &self.options.render)
    }
    pub fn to_png(&self) -> Result<Vec<u8>> {
        render::to_png(&self.matrix, &self.options.render)
    }
    pub fn to_pixels(&self) -> Result<render::Pixels> {
        render::to_pixels(&self.matrix, &self.options.render)
    }
    pub fn to_svg_data_url(&self) -> Result<String> {
        render::to_svg_data_url(&self.matrix, &self.options.render)
    }
    pub fn to_png_data_url(&self) -> Result<String> {
        render::to_png_data_url(&self.matrix, &self.options.render)
    }
}
/// Encode UTF-8 text, choosing the smallest fitting version.
/// ```
/// let qr = specqr::generate("Hello, 日本語", &specqr::Options::default())?;
/// assert!(qr.size() >= 21);
/// assert!(qr.to_svg()?.starts_with("<svg"));
/// # Ok::<(), specqr::Error>(())
/// ```
pub fn generate(text: &str, options: &Options) -> Result<QrCode> {
    build(estimate(text, options)?, options)
}
/// Encode uninterpreted bytes; only automatic and byte modes accept binary input.
pub fn generate_bytes(data: &[u8], options: &Options) -> Result<QrCode> {
    build(estimate_bytes(data, options)?, options)
}
/// Encode explicit segments, preserving boundaries and low-level control semantics.
pub fn generate_segments(segments: &[Segment], options: &Options) -> Result<QrCode> {
    build(analyze_segments(segments, options)?, options)
}
/// Plan UTF-8 input without constructing a symbol.
pub fn estimate(text: &str, options: &Options) -> Result<Plan> {
    options.validate()?;
    if text.len() > 4 * segment::MAX_PAYLOAD_UNITS {
        return Err(invalid(
            ErrorCode::DataTooLong,
            "Input exceeds resource budget",
        ));
    }
    let scalar_count = text.chars().count();
    if scalar_count > segment::MAX_PAYLOAD_UNITS {
        return Err(invalid(
            ErrorCode::DataTooLong,
            "Input exceeds resource budget",
        ));
    }
    let gs1_validation = if options.gs1 {
        let parsed = gs1::parse_element_string(text)?;
        Some(Value::object([
            ("enabled", true.into()),
            ("elementCount", parsed.elements().len().into()),
            (
                "ais",
                Value::array(parsed.elements().iter().map(|e| e.ai().into())),
            ),
            ("hasSeparators", text.contains('\u{1d}').into()),
        ]))
    } else {
        None
    };
    let mut mode = options.mode;
    if (options.gs1 || options.fnc1_second.is_some()) && text.contains('%') {
        if mode == Some(Mode::Alphanumeric) {
            return Err(invalid(
                ErrorCode::InvalidMode,
                "Literal percent in high-level FNC1 requires byte mode; use escaped manual segments for low-level data",
            ));
        }
        if mode.is_none() {
            mode = Some(Mode::Byte);
        }
    }
    let mut cache: [Option<Vec<Segment>>; 3] = [None, None, None];
    select(
        |version| {
            let group = group(version);
            if let Some(v) = &cache[group] {
                return Ok(v.clone());
            }
            let segments = controls(
                segment::create_segments(
                    text,
                    version,
                    mode,
                    options.optimize_segments && scalar_count <= 7089,
                    options.eci.is_none(),
                )?,
                options,
            )?;
            cache[group] = Some(segments.clone());
            Ok(segments)
        },
        options,
        gs1_validation,
    )
}
/// Plan uninterpreted bytes without ECC or matrix work.
pub fn estimate_bytes(data: &[u8], options: &Options) -> Result<Plan> {
    options.validate()?;
    if options.gs1 {
        return Err(invalid(
            ErrorCode::InvalidGs1,
            "High-level GS1 requires an element string",
        ));
    }
    if options.mode.is_some_and(|m| m != Mode::Byte) {
        return Err(invalid(
            ErrorCode::InvalidMode,
            "Binary input can only use byte mode",
        ));
    }
    let segments = controls(vec![Segment::bytes(data)?], options)?;
    select(|_| Ok(segments.clone()), options, None)
}
/// Plan manual segments, retaining oversized arithmetic length when possible.
pub fn analyze_segments(segments: &[Segment], options: &Options) -> Result<Plan> {
    options.validate()?;
    let segments = controls(segment::normalize_segments(segments)?, options)?;
    select(|_| Ok(segments.clone()), options, None)
}
fn group(version: u8) -> usize {
    if version <= 9 {
        0
    } else if version <= 26 {
        1
    } else {
        2
    }
}
fn controls(mut segments: Vec<Segment>, options: &Options) -> Result<Vec<Segment>> {
    let header = if let Some(e) = options.eci {
        Some(Segment::eci(e)?)
    } else if options.gs1 {
        Some(Segment::fnc1())
    } else if let Some(f) = &options.fnc1_second {
        Some(Segment::fnc1_second(f)?)
    } else {
        options.structured_append.clone()
    };
    if let Some(header) = header {
        segments.insert(0, header);
    }
    segment::normalize_segments(&segments)
}
fn select(
    mut factory: impl FnMut(u8) -> Result<Vec<Segment>>,
    options: &Options,
    gs1_validation: Option<Value>,
) -> Result<Plan> {
    let first = options.version.unwrap_or(options.min_version);
    let last = options.version.unwrap_or(options.max_version);
    let mut version = last;
    let mut segments = Vec::new();
    let mut required_bits = 0;
    let mut ecc = options.ecc;
    let mut ok = false;
    for candidate in first..=last {
        version = candidate;
        segments = factory(candidate)?;
        required_bits = segment::bit_length(&segments, candidate)?;
        ok = required_bits <= tables::data_codeword_count(candidate, ecc)? * 8;
        if ok {
            if options.boost_ecc {
                for stronger in [Ecc::L, Ecc::M, Ecc::Q, Ecc::H] {
                    if stronger > ecc
                        && required_bits <= tables::data_codeword_count(candidate, stronger)? * 8
                    {
                        ecc = stronger;
                    }
                }
            }
            break;
        }
    }
    let capacity_bits = tables::data_codeword_count(version, ecc)? * 8;
    let mut diagnostics = diagnostics(&segments, version, ecc, required_bits, options, true, ok)?;
    if let Some(validation) = gs1_validation {
        diagnostics.insert("gs1Validation", validation)?;
    }
    Ok(Plan {
        ok,
        version: if ok || options.version.is_some() {
            Some(version)
        } else {
            None
        },
        capacity_version: version,
        ecc,
        requested_ecc: options.ecc,
        required_bits,
        capacity_bits,
        segments,
        diagnostics,
    })
}
fn build(plan: Plan, options: &Options) -> Result<QrCode> {
    if !plan.ok {
        return Err(Error::new(
            ErrorCode::DataTooLong,
            format!(
                "Input requires {} bits; version {}-{} holds {}",
                plan.required_bits,
                plan.capacity_version,
                plan.ecc.as_str(),
                plan.capacity_bits
            ),
        ));
    }
    let version = plan.capacity_version;
    let data = core::pad_data_bits(&segment::bits(&plan.segments, version)?, version, plan.ecc)?;
    let interleaved = core::interleave_codewords(&data, version, plan.ecc)?;
    let matrix = core::build_matrix(interleaved.codewords(), version, plan.ecc, options.mask)?;
    let mut diagnostics = diagnostics(
        &plan.segments,
        version,
        plan.ecc,
        plan.required_bits,
        options,
        false,
        true,
    )?;
    if let Some(gs1) = plan.diagnostics.get("gs1Validation") {
        diagnostics.insert("gs1Validation", gs1.clone())?;
    }
    diagnostics.insert("maskPattern", matrix.mask_pattern())?;
    diagnostics.insert("maskPenalty", matrix.penalty())?;
    diagnostics.insert(
        "maskPenalties",
        Value::array(matrix.mask_penalties().iter().map(|p| {
            Value::object([
                ("maskPattern", p.mask_pattern().into()),
                ("penalty", p.penalty().into()),
            ])
        })),
    )?;
    diagnostics.insert(
        "maskSelectionReason",
        if options.mask.is_some() {
            "Explicit mask requested."
        } else {
            "Lowest penalty; first mask wins ties."
        },
    )?;
    diagnostics.insert("dataCodewords", data.len())?;
    diagnostics.insert(
        "errorCorrectionCodewords",
        interleaved.codewords().len() - data.len(),
    )?;
    diagnostics.insert("totalCodewords", interleaved.codewords().len())?;
    Ok(QrCode {
        matrix: matrix.matrix().to_vec(),
        version,
        mask: matrix.mask_pattern(),
        ecc: plan.ecc,
        data,
        codewords: interleaved.codewords().to_vec(),
        segments: plan.segments,
        diagnostics,
        options: options.clone(),
    })
}
/// Exact single-segment capacity. Byte capacity counts encoded bytes, not scalars.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capacity {
    version: u8,
    ecc: Ecc,
    size: usize,
    data_codewords: usize,
    total_codewords: usize,
    mode: Option<Mode>,
    character_count_bits: Option<u8>,
    control_bits: u64,
    payload_bits: Option<u64>,
    maximum: Option<usize>,
}
impl Capacity {
    pub fn version(&self) -> u8 {
        self.version
    }
    pub fn ecc(&self) -> Ecc {
        self.ecc
    }
    pub fn size(&self) -> usize {
        self.size
    }
    pub fn data_codewords(&self) -> usize {
        self.data_codewords
    }
    pub fn total_codewords(&self) -> usize {
        self.total_codewords
    }
    pub fn capacity_bits(&self) -> usize {
        self.data_codewords * 8
    }
    pub fn mode(&self) -> Option<Mode> {
        self.mode
    }
    pub fn character_count_bits(&self) -> Option<u8> {
        self.character_count_bits
    }
    pub fn mode_bits(&self) -> Option<u8> {
        self.mode.map(|_| 4)
    }
    pub fn control_bits(&self) -> u64 {
        self.control_bits
    }
    pub fn payload_bits(&self) -> Option<u64> {
        self.payload_bits
    }
    pub fn maximum(&self) -> Option<usize> {
        self.maximum
    }
    pub fn max_characters(&self) -> Option<usize> {
        if self.mode == Some(Mode::Byte) {
            None
        } else {
            self.maximum
        }
    }
    pub fn max_bytes(&self) -> Option<usize> {
        if self.mode == Some(Mode::Byte) {
            self.maximum
        } else {
            None
        }
    }
}
/// Return QR data capacity, optionally accounting for one data-mode header and controls.
pub fn get_capacity(
    version: u8,
    ecc: Ecc,
    mode: Option<Mode>,
    control_bits: u64,
) -> Result<Capacity> {
    tables::validate_version(version)?;
    if control_bits > (1u64 << 53) - 1 {
        return Err(invalid(
            ErrorCode::InvalidInput,
            "control_bits must be 0..2^53-1",
        ));
    }
    let data = tables::data_codeword_count(version, ecc)?;
    let mut width = None;
    let mut payload = None;
    let mut maximum = None;
    if let Some(mode) = mode {
        if !matches!(
            mode,
            Mode::Numeric | Mode::Alphanumeric | Mode::Byte | Mode::Kanji
        ) {
            return Err(invalid(
                ErrorCode::InvalidMode,
                "Capacity requires a data mode",
            ));
        }
        let count_bits = tables::character_count_bits(version, mode)?;
        let bits = (data as u64 * 8).saturating_sub(control_bits + 4 + u64::from(count_bits));
        let count = match mode {
            Mode::Numeric => {
                bits / 10 * 3
                    + if bits % 10 >= 7 {
                        2
                    } else if bits % 10 >= 4 {
                        1
                    } else {
                        0
                    }
            }
            Mode::Alphanumeric => bits / 11 * 2 + u64::from(bits % 11 >= 6),
            Mode::Byte => bits / 8,
            _ => bits / 13,
        };
        width = Some(count_bits);
        payload = Some(bits);
        maximum = Some(count.min((1u64 << count_bits) - 1) as usize);
    }
    Ok(Capacity {
        version,
        ecc,
        size: tables::size(version)?,
        data_codewords: data,
        total_codewords: tables::raw_codeword_count(version)?,
        mode,
        character_count_bits: width,
        control_bits,
        payload_bits: payload,
        maximum,
    })
}
fn warning(code: &str, severity: &str, message: &str, details: Value) -> Value {
    Value::object([
        ("code", code.into()),
        ("severity", severity.into()),
        ("message", message.into()),
        ("details", details),
    ])
}
fn diagnostics(
    segments: &[Segment],
    version: u8,
    ecc: Ecc,
    bits: usize,
    options: &Options,
    planning: bool,
    ok: bool,
) -> Result<Value> {
    let capacity = tables::data_codeword_count(version, ecc)? * 8;
    let controls: Vec<&Segment> = segments.iter().filter(|s| s.is_control()).collect();
    let mut modes = Vec::new();
    for segment in segments.iter().filter(|s| !s.is_control()) {
        if !modes.contains(&segment.mode()) {
            modes.push(segment.mode());
        }
    }
    let mode = if modes.len() == 1 {
        modes[0].as_str()
    } else if modes.is_empty() {
        "byte"
    } else {
        "mixed"
    };
    let mut warnings = Vec::new();
    if options.render.margin < 4 {
        warnings.push(warning(
            "QUIET_ZONE_TOO_SMALL",
            "warning",
            "QR readers expect at least four quiet-zone modules.",
            Value::object([("margin", options.render.margin.into())]),
        ));
    }
    let foreground = render::try_parse_color(&options.render.foreground);
    let background = render::try_parse_color(&options.render.background);
    let ratio = foreground
        .zip(background)
        .map(|(a, b)| render::contrast_ratio(a, b));
    match ratio {
        None => warnings.push(warning(
            "COLOR_CONTRAST_UNKNOWN",
            "info",
            "These SVG colors cannot be checked for contrast.",
            Value::object([]),
        )),
        Some(r) if r < 4.5 => warnings.push(warning(
            "COLOR_CONTRAST_LOW",
            "warning",
            "Color contrast is below the recommended minimum.",
            Value::object([("ratio", r.into())]),
        )),
        Some(r) if r < 7.0 => warnings.push(warning(
            "COLOR_CONTRAST_MODERATE",
            "info",
            "Stronger contrast is recommended.",
            Value::object([("ratio", r.into())]),
        )),
        _ => {}
    }
    if foreground
        .zip(background)
        .is_some_and(|(a, b)| a[3] < 255 || b[3] < 255)
    {
        warnings.push(warning(
            "COLOR_ALPHA_USED",
            "warning",
            "Transparency can reduce scan reliability.",
            Value::object([]),
        ));
    }
    if capacity >= bits && ((capacity - bits) as f64) < (capacity as f64) * 0.05 {
        warnings.push(warning(
            "CAPACITY_NEAR_LIMIT",
            "info",
            "Selected version is close to full.",
            Value::object([]),
        ));
    }
    let mm = options
        .print_dpi
        .map(|dpi| f64::from(options.render.scale) / dpi * 25.4);
    if mm.is_some_and(|m| m < 0.25) {
        warnings.push(warning(
            "PRINT_MODULE_TOO_SMALL",
            "warning",
            "Print modules are smaller than 0.25 mm.",
            Value::object([("moduleSizeMm", mm.into())]),
        ));
    }
    let blocking: Vec<Value> = warnings
        .iter()
        .filter(|w| w.get("severity").and_then(Value::as_str) == Some("warning"))
        .filter_map(|w| w.get("code").cloned())
        .collect();
    if !blocking.is_empty() {
        warnings.push(warning(
            "SCAN_RISK",
            "warning",
            "One or more settings may reduce scan reliability.",
            Value::object([("blockingWarnings", Value::Array(blocking))]),
        ));
    }
    let append = controls.iter().find(|s| s.mode() == Mode::StructuredAppend);
    let second = controls.iter().find(|s| s.mode() == Mode::Fnc1Second);
    let eci = controls.iter().find(|s| s.mode() == Mode::Eci);
    let gs1 = controls.iter().any(|s| s.mode() == Mode::Fnc1);
    let control_json: Vec<Value> = controls
        .iter()
        .map(|s| {
            Ok(Value::object([
                ("mode", s.mode().as_str().into()),
                ("bitLength", s.total_bits(version)?.into()),
            ]))
        })
        .collect::<Result<_>>()?;
    let segment_json: Vec<Value> = segments
        .iter()
        .map(|s| {
            Ok(Value::object([
                ("mode", s.mode().as_str().into()),
                ("characterCount", s.character_count().into()),
                ("byteCount", s.byte_count().into()),
                ("bitLength", s.total_bits(version)?.into()),
            ]))
        })
        .collect::<Result<_>>()?;
    let fnc1 = if gs1 {
        Some("first-position")
    } else if second.is_some() {
        Some("second-position")
    } else {
        None
    };
    let visible_version = if ok || options.version.is_some() {
        Some(version)
    } else {
        None
    };
    let index = append.and_then(|s| s.index());
    let total = append.and_then(|s| s.total());
    Ok(Value::object([
        (
            "phase",
            if planning { "planning" } else { "generation" }.into(),
        ),
        ("renderPlanned", false.into()),
        ("maskEvaluated", (!planning).into()),
        ("codewordsBuilt", (!planning).into()),
        ("version", visible_version.into()),
        (
            "size",
            visible_version.map(|v| usize::from(v) * 4 + 17).into(),
        ),
        ("errorCorrectionLevel", ecc.as_str().into()),
        ("requestedErrorCorrectionLevel", options.ecc.as_str().into()),
        ("boostedErrorCorrection", (ecc != options.ecc).into()),
        (
            "versionSelection",
            if options.version.is_some() {
                "fixed"
            } else if ok {
                "auto-minimum"
            } else {
                "auto-range"
            }
            .into(),
        ),
        (
            "versionSelectionReason",
            if options.version.is_some() {
                "Explicit version requested."
            } else if ok {
                "Smallest fitting version in the requested range."
            } else {
                "No fitting version in the requested range."
            }
            .into(),
        ),
        ("mode", mode.into()),
        ("controlSegments", Value::Array(control_json)),
        (
            "eciAssignmentNumber",
            eci.and_then(|s| s.assignment()).into(),
        ),
        ("fnc1", fnc1.into()),
        ("gs1", gs1.into()),
        (
            "gs1Validation",
            Value::object([
                ("enabled", gs1.into()),
                (
                    "elementCount",
                    if gs1 { Value::Null } else { 0usize.into() },
                ),
                ("ais", Value::Array(Vec::new())),
                ("hasSeparators", false.into()),
            ]),
        ),
        (
            "fnc1Second",
            Value::object([
                ("enabled", second.is_some().into()),
                (
                    "applicationIndicator",
                    second.and_then(|s| s.application_indicator()).into(),
                ),
                (
                    "applicationIndicatorCodeword",
                    second
                        .and_then(|s| s.application_indicator_codeword())
                        .into(),
                ),
            ]),
        ),
        (
            "structuredAppend",
            Value::object([
                ("enabled", append.is_some().into()),
                ("index", index.into()),
                ("total", total.into()),
                ("parity", append.and_then(|s| s.parity()).into()),
                ("sequenceIndex", index.map(|i| i - 1).into()),
                ("sequenceTotal", total.map(|t| t - 1).into()),
                (
                    "sequenceIndicator",
                    index
                        .zip(total)
                        .map(|(i, t)| ((i - 1) << 4) | (t - 1))
                        .into(),
                ),
            ]),
        ),
        ("segments", Value::Array(segment_json)),
        ("dataBitLength", bits.into()),
        ("capacityBits", capacity.into()),
        ("remainingBits", (capacity as i64 - bits as i64).into()),
        (
            "capacityUtilization",
            (bits as f64 / capacity as f64).into(),
        ),
        (
            "inputBytes",
            segments
                .iter()
                .map(|s| s.logical_bytes().len())
                .sum::<usize>()
                .into(),
        ),
        (
            "quietZone",
            Value::object([
                ("modules", options.render.margin.into()),
                ("recommendedModules", 4usize.into()),
                ("isSufficient", (options.render.margin >= 4).into()),
            ]),
        ),
        (
            "colors",
            Value::object([
                ("ratio", ratio.into()),
                ("isInspectable", ratio.is_some().into()),
                ("foregroundAlpha", foreground.map(|a| a[3]).into()),
                ("backgroundAlpha", background.map(|a| a[3]).into()),
                ("isStrong", ratio.is_some_and(|r| r >= 7.0).into()),
                (
                    "isSufficient",
                    (ratio.is_some_and(|r| r >= 4.5)
                        && foreground
                            .zip(background)
                            .is_some_and(|(a, b)| a[3] == 255 && b[3] == 255))
                    .into(),
                ),
            ]),
        ),
        (
            "print",
            Value::object([
                ("dpi", options.print_dpi.into()),
                ("modulePixels", options.render.scale.into()),
                ("moduleSizeMm", mm.into()),
                (
                    "symbolSizeMm",
                    mm.map(|m| {
                        (f64::from(version) * 4.0 + 17.0 + 2.0 * f64::from(options.render.margin))
                            * m
                    })
                    .into(),
                ),
                ("recommendedMinimumModuleSizeMm", 0.25.into()),
                ("isModuleSizeSufficient", mm.map(|m| m >= 0.25).into()),
            ]),
        ),
        ("warnings", Value::Array(warnings)),
    ]))
}
