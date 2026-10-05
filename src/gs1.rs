//! Bounded GS1 element strings and local Digital Link helpers.
//!
//! Catalog and semantic rules follow SpecQR JavaScript revision
//! `15ad15e5c770ea0e39072f8f88b2733018f02ffd`, `src/gs1/*`.
//! The safe, dependency-free URL adapter is independently implemented here;
//! it does not resolve hosts, fetch resources, or claim full WHATWG/UTS #46
//! conformance. GS1 dot-only path values are preserved and rendered in query
//! parameters. Unicode hosts use a bounded mapping plus RFC 3492 Punycode:
//! pinned Unicode 15.0.0 lowercase, fullwidth ASCII, alternate dots, and
//! soft-hyphen removal. Unsupported combining/format/unassigned scalars,
//! compatibility-unstable scalars and Hangul Jamo are rejected instead of
//! emitting noncanonical A-labels. RTL labels use a conservative letters/digits
//! and interior-hyphen subset without mixed digit conventions. No full Unicode
//! normalization or complete IDNA contextual/bidi implementation is provided.
//! Prefer ASCII hostnames when exact cross-runtime URI equivalence matters.

#[path = "idna_table.rs"]
mod idna_table;

use crate::{Error, ErrorCode, Result};
use std::collections::HashSet;
use std::net::Ipv6Addr;
use std::sync::LazyLock;

pub const FNC1_SEPARATOR: &str = "\u{1d}";
pub const GS1_FNC1_SEPARATOR: &str = FNC1_SEPARATOR;
pub const MAX_INPUT_CHARACTERS: usize = 1_000_000;
pub const MAX_ELEMENTS: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    ai: String,
    value: String,
}
impl Element {
    /// Creates an immutable element; helper methods perform validation.
    pub fn new(ai: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            ai: ai.into(),
            value: value.into(),
        }
    }
    pub fn ai(&self) -> &str {
        &self.ai
    }
    pub fn value(&self) -> &str {
        &self.value
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiLength {
    kind: &'static str,
    exact: Option<usize>,
    min: Option<usize>,
    max: Option<usize>,
}
impl AiLength {
    pub fn kind(&self) -> &str {
        self.kind
    }
    pub fn r#type(&self) -> &str {
        self.kind
    }
    pub fn exact(&self) -> Option<usize> {
        self.exact
    }
    pub fn min(&self) -> Option<usize> {
        self.min
    }
    pub fn max(&self) -> Option<usize> {
        self.max
    }
    pub fn is_variable(&self) -> bool {
        self.kind == "variable"
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiInfo {
    ai: String,
    label: &'static str,
    length: AiLength,
    value_kind: &'static str,
    check_digit_rule: &'static str,
    digital_link_role: &'static str,
    separator: &'static str,
    digital_link_path_for_primary: Option<&'static [&'static str]>,
}
impl AiInfo {
    pub fn ai(&self) -> &str {
        &self.ai
    }
    pub fn label(&self) -> &str {
        self.label
    }
    pub fn length(&self) -> &AiLength {
        &self.length
    }
    pub fn value_kind(&self) -> &str {
        self.value_kind
    }
    pub fn check_digit_rule(&self) -> &str {
        self.check_digit_rule
    }
    pub fn digital_link_role(&self) -> &str {
        self.digital_link_role
    }
    pub fn separator(&self) -> &str {
        self.separator
    }
    pub fn digital_link_path_for_primary(&self) -> Option<&[&str]> {
        self.digital_link_path_for_primary
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementStringParseResult {
    elements: Vec<Element>,
    has_separators: bool,
}
impl ElementStringParseResult {
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }
    pub fn has_separators(&self) -> bool {
        self.has_separators
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expected {
    Text(String),
    Boolean(bool),
}
impl Expected {
    pub fn as_str(&self) -> Option<&str> {
        if let Self::Text(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Self::Boolean(v) = self {
            Some(*v)
        } else {
            None
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    code: &'static str,
    message: String,
    reason: &'static str,
    ai: Option<String>,
    value: Option<String>,
    key: Option<String>,
    offset: Option<usize>,
    element_index: Option<usize>,
    expected: Option<Expected>,
    count: Option<usize>,
}
impl ValidationIssue {
    pub fn code(&self) -> &str {
        self.code
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn reason(&self) -> &str {
        self.reason
    }
    pub fn ai(&self) -> Option<&str> {
        self.ai.as_deref()
    }
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }
    /// Offset in UTF-16 code units, matching the JavaScript diagnostics.
    pub fn offset(&self) -> Option<usize> {
        self.offset
    }
    pub fn element_index(&self) -> Option<usize> {
        self.element_index
    }
    pub fn expected(&self) -> Option<&Expected> {
        self.expected.as_ref()
    }
    pub fn count(&self) -> Option<usize> {
        self.count
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationOptions {
    context: String,
    collect_all_errors: bool,
    allow_unsupported_ai: bool,
}
impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            context: "element-string".into(),
            collect_all_errors: true,
            allow_unsupported_ai: false,
        }
    }
}
impl ValidationOptions {
    pub fn new(
        context: impl Into<String>,
        collect_all_errors: bool,
        allow_unsupported_ai: bool,
    ) -> Self {
        Self {
            context: context.into(),
            collect_all_errors,
            allow_unsupported_ai,
        }
    }
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = context.into();
        self
    }
    pub fn with_collect_all_errors(mut self, value: bool) -> Self {
        self.collect_all_errors = value;
        self
    }
    pub fn with_allow_unsupported_ai(mut self, value: bool) -> Self {
        self.allow_unsupported_ai = value;
        self
    }
    pub fn context(&self) -> &str {
        &self.context
    }
    pub fn collect_all_errors(&self) -> bool {
        self.collect_all_errors
    }
    pub fn allow_unsupported_ai(&self) -> bool {
        self.allow_unsupported_ai
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationResult {
    ok: bool,
    elements: Option<Vec<Element>>,
    has_separators: Option<bool>,
    errors: Vec<ValidationIssue>,
    warnings: Vec<ValidationIssue>,
}
impl ValidationResult {
    pub fn ok(&self) -> bool {
        self.ok
    }
    pub fn elements(&self) -> Option<&[Element]> {
        self.elements.as_deref()
    }
    pub fn has_separators(&self) -> Option<bool> {
        self.has_separators
    }
    pub fn errors(&self) -> &[ValidationIssue] {
        &self.errors
    }
    pub fn warnings(&self) -> &[ValidationIssue] {
        &self.warnings
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownQuery {
    key: String,
    value: String,
}
impl UnknownQuery {
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn value(&self) -> &str {
        &self.value
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigitalLinkParseResult {
    elements: Vec<Element>,
    primary: Element,
    path_elements: Vec<Element>,
    query_elements: Vec<Element>,
    unknown_query: Vec<UnknownQuery>,
}
impl DigitalLinkParseResult {
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }
    pub fn primary(&self) -> &Element {
        &self.primary
    }
    pub fn path_elements(&self) -> &[Element] {
        &self.path_elements
    }
    pub fn query_elements(&self) -> &[Element] {
        &self.query_elements
    }
    pub fn unknown_query(&self) -> &[UnknownQuery] {
        &self.unknown_query
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigitalLinkValidationResult {
    ok: bool,
    result: Option<DigitalLinkParseResult>,
    errors: Vec<ValidationIssue>,
    warnings: Vec<ValidationIssue>,
}
impl DigitalLinkValidationResult {
    pub fn ok(&self) -> bool {
        self.ok
    }
    pub fn result(&self) -> Option<&DigitalLinkParseResult> {
        self.result.as_ref()
    }
    pub fn errors(&self) -> &[ValidationIssue] {
        &self.errors
    }
    pub fn warnings(&self) -> &[ValidationIssue] {
        &self.warnings
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DigitalLinkOptions {
    base_url: Option<String>,
    primary_ai: Option<String>,
    path_ais: Option<Vec<String>>,
    unknown_query: String,
    normalize: bool,
    mode: String,
}
impl Default for DigitalLinkOptions {
    fn default() -> Self {
        Self {
            base_url: None,
            primary_ai: None,
            path_ais: None,
            unknown_query: "preserve".into(),
            normalize: false,
            mode: "specqr-deterministic".into(),
        }
    }
}
impl DigitalLinkOptions {
    pub fn for_base_url(url: impl Into<String>) -> Self {
        Self::default().with_base_url(url)
    }
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }
    pub fn with_primary_ai(mut self, ai: impl Into<String>) -> Self {
        self.primary_ai = Some(ai.into());
        self
    }
    pub fn with_path_ais(mut self, ais: Vec<String>) -> Self {
        self.path_ais = Some(ais);
        self
    }
    pub fn with_unknown_query(mut self, policy: impl Into<String>) -> Self {
        self.unknown_query = policy.into();
        self
    }
    pub fn with_normalize(mut self, normalize: bool) -> Self {
        self.normalize = normalize;
        self
    }
    pub fn with_mode(mut self, mode: impl Into<String>) -> Self {
        self.mode = mode.into();
        self
    }
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
    pub fn primary_ai(&self) -> Option<&str> {
        self.primary_ai.as_deref()
    }
    pub fn path_ais(&self) -> Option<&[String]> {
        self.path_ais.as_deref()
    }
    pub fn unknown_query(&self) -> &str {
        &self.unknown_query
    }
    pub fn normalize(&self) -> bool {
        self.normalize
    }
    pub fn mode(&self) -> &str {
        &self.mode
    }
}

fn failure(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidGs1, message)
}
fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}
fn text<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    // A byte precheck bounds the cost of computing UTF-16 length.
    if value.len() > MAX_INPUT_CHARACTERS * 3 || utf16_len(value) > MAX_INPUT_CHARACTERS {
        return Err(failure(format!(
            "{label} must contain at most {MAX_INPUT_CHARACTERS} characters"
        )));
    }
    Ok(value)
}
fn digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit())
}
fn is_ai(value: &str) -> bool {
    (2..=4).contains(&value.len()) && digits(value)
}
fn is_primary(ai: &str) -> bool {
    matches!(ai, "00" | "01" | "414")
}
fn bounded(values: &[Element]) -> Result<()> {
    if values.len() > MAX_ELEMENTS {
        return Err(failure(format!(
            "GS1 elements must contain at most {MAX_ELEMENTS} elements"
        )));
    }
    let mut work = 0;
    for e in values {
        text(&e.ai, "GS1 AI")?;
        text(&e.value, "GS1 value")?;
        work += utf16_len(&e.ai) + utf16_len(&e.value);
        if work > MAX_INPUT_CHARACTERS {
            return Err(failure(format!(
                "GS1 elements aggregate text exceeds the character work budget ({MAX_INPUT_CHARACTERS})"
            )));
        }
    }
    Ok(())
}
static CATALOG: LazyLock<Vec<AiInfo>> = LazyLock::new(|| {
    let mut result = Vec::new();
    let mut add = |ai: String, label, size, variable, kind, check, role| {
        result.push(AiInfo {
            ai,
            label,
            length: if variable {
                AiLength {
                    kind: "variable",
                    exact: None,
                    min: Some(1),
                    max: Some(size),
                }
            } else {
                AiLength {
                    kind: "fixed",
                    exact: Some(size),
                    min: None,
                    max: None,
                }
            },
            value_kind: kind,
            check_digit_rule: check,
            digital_link_role: role,
            separator: if variable {
                "required-when-followed"
            } else {
                "none"
            },
            digital_link_path_for_primary: if role == "key-qualifier" {
                Some(&["01"][..])
            } else {
                None
            },
        })
    };
    add(
        "00".into(),
        "Serial shipping container code",
        18,
        false,
        "numeric",
        "sscc",
        "primary-key",
    );
    add(
        "01".into(),
        "Global trade item number",
        14,
        false,
        "numeric",
        "gtin",
        "primary-key",
    );
    add(
        "02".into(),
        "Contained trade item GTIN",
        14,
        false,
        "numeric",
        "gtin",
        "data-attribute",
    );
    add(
        "10".into(),
        "Batch or lot number",
        20,
        true,
        "text",
        "none",
        "key-qualifier",
    );
    for (ai, label) in [
        ("11", "Production date"),
        ("12", "Due date"),
        ("13", "Packaging date"),
        ("15", "Best before date"),
        ("16", "Sell by date"),
        ("17", "Expiration date"),
    ] {
        add(
            ai.into(),
            label,
            6,
            false,
            "numeric",
            "none",
            "data-attribute",
        );
    }
    add(
        "20".into(),
        "Internal product variant",
        2,
        false,
        "numeric",
        "none",
        "data-attribute",
    );
    for (ai, label) in [("21", "Serial number"), ("22", "Consumer product variant")] {
        add(ai.into(), label, 20, true, "text", "none", "key-qualifier");
    }
    for (ai, label) in [
        ("30", "Variable count"),
        ("37", "Count of contained trade items"),
    ] {
        add(
            ai.into(),
            label,
            8,
            true,
            "numeric",
            "none",
            "data-attribute",
        );
    }
    for (ai, label) in [
        ("240", "Additional product identification"),
        ("241", "Customer part number"),
        ("400", "Customer purchase order number"),
    ] {
        add(ai.into(), label, 30, true, "text", "none", "data-attribute");
    }
    for (i, label) in [
        "Ship to global location number",
        "Bill to global location number",
        "Purchased from global location number",
        "Ship for global location number",
        "Identification of a physical location",
        "Global location number of the invoicing party",
    ]
    .iter()
    .enumerate()
    {
        add(
            (410 + i).to_string(),
            label,
            13,
            false,
            "numeric",
            "none",
            if i == 4 {
                "primary-key"
            } else {
                "data-attribute"
            },
        );
    }
    add(
        "420".into(),
        "Ship to postal code",
        20,
        true,
        "text",
        "none",
        "data-attribute",
    );
    for (ai, label) in [
        ("422", "Country of origin"),
        ("424", "Country of processing"),
        ("425", "Country of disassembly"),
        ("426", "Country covering full process chain"),
    ] {
        add(
            ai.into(),
            label,
            3,
            false,
            "numeric",
            "none",
            "data-attribute",
        );
    }
    for start in [3100, 3200] {
        for i in 0..6 {
            add(
                (start + i).to_string(),
                if start == 3100 {
                    "Net weight in kilograms"
                } else {
                    "Net weight in pounds"
                },
                6,
                false,
                "numeric",
                "none",
                "data-attribute",
            );
        }
    }
    for i in 91..100 {
        add(
            i.to_string(),
            "Company internal information",
            90,
            true,
            "text",
            "none",
            "data-attribute",
        );
    }
    result
});
pub fn get_supported_ais() -> &'static [AiInfo] {
    &CATALOG
}
pub fn get_ai_info(ai: &str) -> Option<&'static AiInfo> {
    CATALOG.iter().find(|e| e.ai == ai)
}
pub use get_ai_info as get_gs1_ai_info;
pub use get_supported_ais as get_supported_gs1_ais;
fn numeric<'a>(input: &'a str, label: &str) -> Result<&'a str> {
    text(input, label)?;
    if !digits(input) {
        return Err(failure(format!("{label} must contain digits only")));
    }
    Ok(input)
}
pub fn calculate_check_digit(body: &str) -> Result<String> {
    numeric(body, "GS1 check digit input")?;
    let sum: u32 = body
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, c)| u32::from(c - b'0') * if i % 2 == 0 { 3 } else { 1 })
        .sum();
    Ok(((10 - sum % 10) % 10).to_string())
}
pub fn validate_check_digit(value: &str) -> Result<bool> {
    numeric(value, "GS1 check digit value")?;
    if value.len() < 2 {
        return Err(failure(
            "GS1 check digit value must include body digits and one check digit",
        ));
    }
    Ok(calculate_check_digit(&value[..value.len() - 1])? == value[value.len() - 1..])
}
pub fn calculate_gtin_check_digit(body: &str) -> Result<String> {
    numeric(body, "GTIN body")?;
    if ![7, 11, 12, 13].contains(&body.len()) {
        return Err(failure("GTIN body must be 7, 11, 12, or 13 digits"));
    }
    calculate_check_digit(body)
}
pub fn append_gtin_check_digit(body: &str) -> Result<String> {
    Ok(format!("{body}{}", calculate_gtin_check_digit(body)?))
}
pub fn validate_gtin_check_digit(value: &str) -> Result<bool> {
    numeric(value, "GTIN")?;
    if ![8, 12, 13, 14].contains(&value.len()) {
        return Err(failure("GTIN must be 8, 12, 13, or 14 digits"));
    }
    validate_check_digit(value)
}
pub fn calculate_sscc_check_digit(body: &str) -> Result<String> {
    numeric(body, "SSCC body")?;
    if body.len() != 17 {
        return Err(failure("SSCC body must be exactly 17 digits"));
    }
    calculate_check_digit(body)
}
pub fn append_sscc_check_digit(body: &str) -> Result<String> {
    Ok(format!("{body}{}", calculate_sscc_check_digit(body)?))
}
pub fn validate_sscc_check_digit(value: &str) -> Result<bool> {
    numeric(value, "SSCC")?;
    if value.len() != 18 {
        return Err(failure("SSCC must be exactly 18 digits"));
    }
    validate_check_digit(value)
}
pub use calculate_check_digit as calculate_gs1_check_digit;
pub use validate_check_digit as validate_gs1_check_digit;
fn element(e: &Element, index: usize) -> Result<Element> {
    text(&e.ai, "GS1 AI")?;
    text(&e.value, "GS1 value")?;
    if !is_ai(&e.ai) {
        return Err(failure(format!(
            "GS1 element {index} has invalid AI {:?}; expected 2 to 4 digits",
            e.ai.chars().take(100).collect::<String>()
        )));
    }
    let info = get_ai_info(&e.ai).ok_or_else(|| {
        failure(format!(
            "Unsupported GS1 AI {}. Add explicit support before using it.",
            e.ai
        ))
    })?;
    let value = &e.value;
    let prefix = format!("GS1 AI {} value ", e.ai);
    let problem = if value.is_empty() {
        Some("must not be empty")
    } else if value.contains(FNC1_SEPARATOR) {
        Some("must not contain the FNC1 separator")
    } else if value.contains(['(', ')']) {
        Some("must be raw data without human-readable parentheses")
    } else if !value.bytes().all(|b| (32..=126).contains(&b)) {
        Some("must use printable ASCII characters")
    } else if info.value_kind == "numeric" && !digits(value) {
        Some("must contain digits only")
    } else {
        None
    };
    if let Some(problem) = problem {
        return Err(failure(prefix + problem));
    }
    if let Some(max) = info.length.max {
        if value.len() > max {
            return Err(failure(format!("{prefix}must be at most {max} characters")));
        }
    }
    if let Some(exact) = info.length.exact {
        if value.len() != exact {
            return Err(failure(format!(
                "{prefix}must be exactly {exact} characters"
            )));
        }
    }
    if info.check_digit_rule == "gtin" && !validate_gtin_check_digit(value)? {
        return Err(failure(prefix + "has an invalid GTIN check digit"));
    }
    if info.check_digit_rule == "sscc" && !validate_sscc_check_digit(value)? {
        return Err(failure(prefix + "has an invalid SSCC check digit"));
    }
    Ok(e.clone())
}
pub fn from_human_readable(input: &str) -> Result<Vec<Element>> {
    text(input, "GS1 human-readable input")?;
    if input.is_empty() {
        return Err(failure("GS1 human-readable input must not be empty"));
    }
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        if out.len() == MAX_ELEMENTS {
            return Err(failure("GS1 elements exceed element limit"));
        }
        // All preceding validated elements contain ASCII only.
        let offset = pos;
        if input.as_bytes()[pos] != b'(' {
            return Err(failure(format!(
                "GS1 human-readable input must contain an AI in parentheses at offset {offset}"
            )));
        }
        let close = input[pos + 1..]
            .find(')')
            .map(|n| pos + 1 + n)
            .ok_or_else(|| {
                failure(format!(
                    "GS1 AI starting at offset {offset} is missing a closing parenthesis"
                ))
            })?;
        let end = input[close + 1..]
            .find('(')
            .map_or(input.len(), |n| close + 1 + n);
        out.push(element(
            &Element::new(&input[pos + 1..close], &input[close + 1..end]),
            out.len(),
        )?);
        pos = end;
    }
    Ok(out)
}
pub use from_human_readable as parse_human_readable;
pub fn to_element_string(elements: &[Element]) -> Result<String> {
    bounded(elements)?;
    if elements.is_empty() {
        return Err(failure("GS1 elements must not be empty"));
    }
    let mut out = String::new();
    for (i, e) in elements.iter().enumerate() {
        let e = element(e, i)?;
        out.push_str(&e.ai);
        out.push_str(&e.value);
        if i + 1 < elements.len() && get_ai_info(&e.ai).is_some_and(|a| a.length.is_variable()) {
            out.push('\u{1d}');
        }
        if out.len() > MAX_INPUT_CHARACTERS {
            return Err(failure("GS1 element string output exceeds character limit"));
        }
    }
    Ok(out)
}
pub use to_element_string as create_element_string;
pub fn to_human_readable(elements: &[Element]) -> Result<String> {
    bounded(elements)?;
    if elements.is_empty() {
        return Err(failure("GS1 elements must not be empty"));
    }
    let mut out = String::new();
    for (i, e) in elements.iter().enumerate() {
        let e = element(e, i)?;
        out.push('(');
        out.push_str(&e.ai);
        out.push(')');
        out.push_str(&e.value);
        if out.len() > MAX_INPUT_CHARACTERS {
            return Err(failure("GS1 human-readable output exceeds character limit"));
        }
    }
    Ok(out)
}
pub fn element_string_to_human_readable(input: &str) -> Result<String> {
    to_human_readable(parse_element_string(input)?.elements())
}
fn read_ai(input: &str, offset: usize) -> Option<&'static AiInfo> {
    [4, 3, 2]
        .iter()
        .find_map(|n| input.get(offset..offset + n).and_then(get_ai_info))
}
fn utf16_end(input: &str, start: usize, count: usize) -> usize {
    let mut units = 0;
    for (i, c) in input[start..].char_indices() {
        if units + c.len_utf16() > count {
            return start + i + c.len_utf8();
        }
        units += c.len_utf16();
        if units == count {
            return start + i + c.len_utf8();
        }
    }
    input.len()
}
pub fn parse_element_string(input: &str) -> Result<ElementStringParseResult> {
    text(input, "GS1 element string input")?;
    if input.is_empty() {
        return Err(failure("GS1 element string input must not be empty"));
    }
    if input.contains(['(', ')']) {
        return Err(failure(
            "GS1 element string input must be raw data without human-readable parentheses; use from_human_readable() and to_element_string() first",
        ));
    }
    let mut result = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        if result.len() == MAX_ELEMENTS {
            return Err(failure("GS1 elements exceed element limit"));
        }
        // All preceding validated elements contain ASCII only.
        let offset = pos;
        if input.as_bytes()[pos] == 29 {
            return Err(failure(format!(
                "GS1 element string has an unexpected FNC1 separator at offset {offset}"
            )));
        }
        let info = read_ai(input, pos)
            .ok_or_else(|| failure(format!("Unsupported GS1 AI at offset {offset}")))?;
        let start = pos + info.ai.len();
        let end = if info.length.is_variable() {
            input[start..]
                .find('\u{1d}')
                .map_or(input.len(), |n| start + n)
        } else {
            utf16_end(input, start, info.length.exact.unwrap_or(0))
        };
        if info.length.is_variable() && end == input.len() {
            // Every fixed catalog tail is at most 22 UTF-16 units long.
            for (relative, _) in input[start..end].char_indices().skip(1) {
                let scan = start + relative;
                if end - scan > 88 {
                    continue;
                }
                if utf16_len(&input[scan..end]) > 22 {
                    continue;
                }
                if let Some(tail) = read_ai(input, scan) {
                    if !tail.length.is_variable()
                        && utf16_len(&input[scan..end])
                            == tail.ai.len() + tail.length.exact.unwrap_or(0)
                    {
                        return Err(failure(format!(
                            "GS1 variable-length element at offset {} is missing an FNC1 separator before offset {}",
                            start,
                            utf16_len(&input[..scan])
                        )));
                    }
                }
            }
        }
        result.push(element(
            &Element::new(&info.ai, &input[start..end]),
            result.len(),
        )?);
        pos = end;
        if pos < input.len() && input.as_bytes()[pos] == 29 && info.length.is_variable() {
            pos += 1;
            if pos == input.len() {
                return Err(failure(
                    "GS1 element string must not end with an FNC1 separator",
                ));
            }
        }
    }
    Ok(ElementStringParseResult {
        elements: result,
        has_separators: input.contains(FNC1_SEPARATOR),
    })
}
fn simple_issue(
    code: &'static str,
    message: impl Into<String>,
    reason: &'static str,
    expected: Option<Expected>,
) -> ValidationIssue {
    ValidationIssue {
        code,
        message: message.into(),
        reason,
        expected,
        ai: None,
        value: None,
        key: None,
        offset: None,
        element_index: None,
        count: None,
    }
}
fn expected(value: impl Into<String>) -> Option<Expected> {
    Some(Expected::Text(value.into()))
}
fn options_issue(opts: &ValidationOptions) -> Option<ValidationIssue> {
    if !matches!(opts.context.as_str(), "element-string" | "digital-link") {
        return Some(simple_issue(
            "GS1_INVALID_INPUT",
            "GS1 validation context must be \"element-string\" or \"digital-link\"",
            "invalid-options",
            expected("element-string or digital-link"),
        ));
    }
    if opts.allow_unsupported_ai {
        return Some(simple_issue(
            "GS1_INVALID_INPUT",
            "GS1 validation allowUnsupportedAi must be false",
            "invalid-options",
            Some(Expected::Boolean(false)),
        ));
    }
    None
}
fn invalid(errors: Vec<ValidationIssue>) -> ValidationResult {
    ValidationResult {
        ok: false,
        elements: None,
        has_separators: None,
        errors,
        warnings: Vec::new(),
    }
}
pub fn validate_elements(elements: &[Element]) -> ValidationResult {
    validate_elements_with_options(elements, &ValidationOptions::default())
}
pub fn validate_elements_with_options(
    elements: &[Element],
    options: &ValidationOptions,
) -> ValidationResult {
    if let Some(issue) = options_issue(options) {
        return invalid(vec![issue]);
    }
    if let Err(e) = bounded(elements) {
        return invalid(vec![issue(e, None, None, None, false)]);
    }
    if elements.is_empty() {
        return invalid(vec![issue(
            failure("GS1 elements must not be empty"),
            None,
            None,
            None,
            false,
        )]);
    }
    let mut normalized = Vec::new();
    let mut errors = Vec::new();
    for (i, raw) in elements.iter().enumerate() {
        match element(raw, i) {
            Ok(e) => normalized.push(e),
            Err(e) => {
                errors.push(issue(e, Some(raw), Some(i), None, false));
                if !options.collect_all_errors {
                    break;
                }
            }
        }
    }
    if !errors.is_empty() {
        return invalid(errors);
    }
    if options.context == "digital-link" && !normalized.iter().any(|e| is_primary(&e.ai)) {
        return invalid(vec![simple_issue(
            "GS1_INVALID_DIGITAL_LINK_PLACEMENT",
            "GS1 Digital Link elements must include a primary AI 00, 01, or 414",
            "invalid-digital-link-placement",
            expected("primary AI 00, 01, or 414"),
        )]);
    }
    ValidationResult {
        ok: true,
        elements: Some(normalized),
        has_separators: None,
        errors,
        warnings: Vec::new(),
    }
}
pub fn validate_element_string(input: &str) -> ValidationResult {
    validate_element_string_with_options(input, &ValidationOptions::default())
}
pub fn validate_element_string_with_options(
    input: &str,
    options: &ValidationOptions,
) -> ValidationResult {
    if let Some(issue) = options_issue(options) {
        return invalid(vec![issue]);
    }
    match parse_element_string(input) {
        Ok(p) => ValidationResult {
            ok: true,
            elements: Some(p.elements),
            has_separators: Some(p.has_separators),
            errors: Vec::new(),
            warnings: Vec::new(),
        },
        Err(e) => invalid(vec![issue(e, None, None, Some(input), false)]),
    }
}
fn digits_after<'a>(value: &'a str, pattern: &str, max: usize) -> Option<&'a str> {
    let start = value.find(pattern)? + pattern.len();
    let len = value[start..]
        .bytes()
        .take(max)
        .take_while(u8::is_ascii_digit)
        .count();
    if len == 0 {
        None
    } else {
        Some(&value[start..start + len])
    }
}
fn byte_at_utf16(value: &str, offset: usize) -> Option<usize> {
    let mut n = 0;
    for (i, c) in value.char_indices() {
        if n == offset {
            return Some(i);
        }
        n += c.len_utf16();
    }
    if n == offset { Some(value.len()) } else { None }
}
fn issue(
    error: Error,
    raw: Option<&Element>,
    index: Option<usize>,
    input: Option<&str>,
    digital: bool,
) -> ValidationIssue {
    let message = error.message();
    let (mut code, mut reason, mut exp) = ("GS1_INVALID_INPUT", "invalid-input", None);
    if digital
        && (message.contains("absolute http or https URL")
            || message.contains("must use http or https"))
    {
        code = "GS1_DIGITAL_LINK_INVALID_URI";
        reason = "invalid-uri";
        exp = expected("absolute http or https URL");
    } else if digital && message.contains("must not include a fragment") {
        code = "GS1_DIGITAL_LINK_FRAGMENT_NOT_ALLOWED";
        reason = "fragment-not-allowed";
        exp = expected("URI without fragment");
    } else if digital && message.contains("valid percent-encoding") {
        code = "GS1_INVALID_PERCENT_ENCODING";
        reason = "invalid-percent-encoding";
        exp = expected("percent escapes must use two hexadecimal digits");
    } else if digital && message.contains("query parameter") && message.contains("is not a GS1 AI")
    {
        code = "GS1_DIGITAL_LINK_UNKNOWN_QUERY";
        reason = "unknown-query";
        exp = expected("GS1 AI query parameter or unknownQuery: \"preserve\"");
    } else if digital
        && (message.contains("primaryAi must be one") || message.contains("unknownQuery must be"))
    {
        reason = "invalid-options";
        exp = expected(if message.contains("primaryAi") {
            "00, 01, or 414"
        } else {
            "preserve or reject"
        });
    } else if digital && (message.contains("path must") || message.contains("path segment")) {
        reason = "malformed-path";
        exp = expected("Digital Link path containing primary AI and AI/value pairs");
    } else if message.contains("Unsupported GS1 AI") {
        code = "GS1_UNSUPPORTED_AI";
        reason = "unsupported-ai";
        exp = expected("supported GS1 AI");
    } else if let Some((begin, end)) = ["exactly ", "at most "].iter().find_map(|p| {
        let n = digits_after(message, p, usize::MAX)?;
        let begin = message.find(p)?;
        let end = begin + p.len() + n.len();
        message[end..]
            .starts_with(" characters")
            .then_some((begin, end + 11))
    }) {
        code = "GS1_INVALID_LENGTH";
        reason = "invalid-length";
        exp = expected(&message[begin..end]);
    } else if message.contains("digits only") || message.contains("printable ASCII") {
        code = "GS1_INVALID_CHARSET";
        reason = "invalid-charset";
        exp = expected(if message.contains("digits only") {
            "digits only"
        } else {
            "printable ASCII"
        });
    } else if message.contains("missing an FNC1 separator") {
        code = "GS1_MISSING_SEPARATOR";
        reason = "missing-separator";
        exp = expected("FNC1 separator before the next GS1 element");
    } else if message.contains("unexpected FNC1 separator")
        || message.contains("must not end with an FNC1 separator")
        || message.contains("must not contain the FNC1 separator")
    {
        code = "GS1_UNEXPECTED_SEPARATOR";
        reason = "unexpected-separator";
        exp = expected("separator only after a non-final variable-length GS1 element");
    } else if message.contains("invalid GTIN check digit")
        || message.contains("invalid SSCC check digit")
    {
        code = "GS1_INVALID_CHECK_DIGIT";
        reason = "invalid-check-digit";
        exp = expected(if message.contains("SSCC") {
            "valid SSCC check digit"
        } else {
            "valid GTIN check digit"
        });
    } else if message.contains("cannot be placed in the Digital Link path") {
        code = "GS1_INVALID_DIGITAL_LINK_PLACEMENT";
        reason = "invalid-digital-link-placement";
    } else if message.contains("duplicate AI") {
        code = "GS1_DUPLICATE_AI";
        reason = "duplicate-ai";
        exp = expected("unique GS1 AI within the Digital Link URI");
    }
    let mut out = simple_issue(code, message, reason, exp);
    out.ai = digits_after(message, "GS1 AI ", 4)
        .or_else(|| digits_after(message, "duplicate AI ", 4))
        .filter(|v| v.len() >= 2)
        .map(str::to_owned);
    out.offset = digits_after(message, "offset ", 20).and_then(|s| s.parse().ok());
    if out.ai.is_none() {
        if let (Some(input), Some(offset)) = (input, out.offset) {
            if let Some(byte) = byte_at_utf16(input, offset) {
                let n = input[byte..]
                    .bytes()
                    .take(4)
                    .take_while(u8::is_ascii_digit)
                    .count();
                if n >= 2 {
                    out.ai = Some(input[byte..byte + n].into());
                } else {
                    for size in 2..=4 {
                        if let Some(start) = offset
                            .checked_sub(size)
                            .and_then(|p| byte_at_utf16(input, p))
                        {
                            if get_ai_info(&input[start..byte]).is_some() {
                                out.ai = Some(input[start..byte].into());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    out.element_index = digits_after(message, "GS1 element ", 20)
        .and_then(|s| s.parse().ok())
        .or(index);
    out.value = raw.map(|e| e.value.clone());
    if code == "GS1_DIGITAL_LINK_UNKNOWN_QUERY" {
        out.key = message
            .strip_prefix("GS1 Digital Link query parameter \"")
            .and_then(|s| s.strip_suffix("\" is not a GS1 AI"))
            .map(str::to_owned);
    }
    out
}

#[derive(Clone, Debug)]
struct Url {
    scheme: String,
    authority: String,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}
impl Url {
    fn serialize(&self) -> Result<String> {
        let mut out = format!("{}://{}{}", self.scheme, self.authority, self.path);
        if let Some(q) = &self.query {
            out.push('?');
            out.push_str(q);
        }
        if let Some(f) = &self.fragment {
            out.push('#');
            out.push_str(f);
        }
        text(&out, "GS1 Digital Link output")?;
        Ok(out)
    }
}
fn invalid_uri() -> Error {
    failure("GS1 Digital Link URI must be an absolute http or https URL")
}
fn escape(out: &mut String, value: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push('%');
    out.push(HEX[(value >> 4) as usize] as char);
    out.push(HEX[(value & 15) as usize] as char);
}
// 0 = encodeURIComponent, 1 = form encoding, 2 = path, 3 = special query.
fn encode(value: &str, mode: u8) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        let keep = match mode {
            0 => b.is_ascii_alphanumeric() || b"~!*'()-._".contains(&b),
            1 => b.is_ascii_alphanumeric() || b"*-._".contains(&b),
            2 => b > 32 && b < 127 && !b"\"#<>?`{}^".contains(&b),
            _ => b > 32 && b < 127 && !b"\"#'<>".contains(&b),
        };
        if keep {
            out.push(b as char);
        } else if mode == 1 && b == 32 {
            out.push('+');
        } else {
            escape(&mut out, b);
        }
    }
    out
}
fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
fn invalid_percent(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.iter().enumerate().any(|(i, b)| {
        *b == b'%'
            && (bytes.get(i + 1).and_then(|b| hex(*b)).is_none()
                || bytes.get(i + 2).and_then(|b| hex(*b)).is_none())
    })
}
fn percent_bytes(value: &str, form: bool) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(a * 16 + b);
                i += 3;
                continue;
            }
        }
        out.push(if form && bytes[i] == b'+' {
            b' '
        } else {
            bytes[i]
        });
        i += 1;
    }
    out
}
fn strict_decode(value: &str, label: &str) -> Result<String> {
    let err = || {
        failure(format!(
            "GS1 Digital Link path {label} must be valid percent-encoding"
        ))
    };
    if invalid_percent(value) {
        return Err(err());
    }
    String::from_utf8(percent_bytes(value, false)).map_err(|_| err())
}
fn form_decode(value: &str) -> String {
    String::from_utf8_lossy(&percent_bytes(value, true)).into_owned()
}
fn credentials(input: &str) -> String {
    let mut out = String::new();
    let mut colon = false;
    for b in input.bytes() {
        if b == b':' && !colon {
            out.push(':');
            colon = true;
        } else if b <= 32 || b > 126 || b"\"#<>?`{}/:;=@[\\]^|".contains(&b) {
            escape(&mut out, b);
        } else {
            out.push(b as char);
        }
    }
    if out.ends_with(':') {
        out.pop();
    }
    out
}

// RFC 3492 algorithms with checked arithmetic and bounded label size.
fn adapt(mut delta: u64, points: u64, first: bool) -> u64 {
    delta = if first { delta / 700 } else { delta / 2 };
    delta += delta / points;
    let mut k = 0;
    while delta > 455 {
        delta /= 35;
        k += 36;
    }
    k + 36 * delta / (delta + 38)
}
fn puny_digit(d: u64) -> char {
    if d < 26 {
        (b'a' + d as u8) as char
    } else {
        (b'0' + (d - 26) as u8) as char
    }
}
fn puny_value(c: u8) -> Option<u64> {
    match c {
        b'a'..=b'z' => Some((c - b'a') as u64),
        b'A'..=b'Z' => Some((c - b'A') as u64),
        b'0'..=b'9' => Some((c - b'0' + 26) as u64),
        _ => None,
    }
}
fn punycode_encode(input: &str) -> Result<String> {
    let points: Vec<u32> = input.chars().map(u32::from).collect();
    if points.len() > 1024 {
        return Err(invalid_uri());
    }
    let mut out: String = input.chars().filter(char::is_ascii).collect();
    let basic = out.len() as u64;
    let mut h = basic;
    if basic > 0 {
        out.push('-');
    }
    let (mut n, mut delta, mut bias) = (128u64, 0u64, 72u64);
    while h < points.len() as u64 {
        let m = points
            .iter()
            .map(|p| *p as u64)
            .filter(|p| *p >= n)
            .min()
            .ok_or_else(invalid_uri)?;
        delta = delta
            .checked_add((m - n).checked_mul(h + 1).ok_or_else(invalid_uri)?)
            .ok_or_else(invalid_uri)?;
        n = m;
        for cp in &points {
            let cp = *cp as u64;
            match cp.cmp(&n) {
                std::cmp::Ordering::Less => {
                    delta = delta.checked_add(1).ok_or_else(invalid_uri)?;
                }
                std::cmp::Ordering::Equal => {
                    let mut q = delta;
                    let mut k = 36;
                    loop {
                        let t = if k <= bias {
                            1
                        } else if k >= bias + 26 {
                            26
                        } else {
                            k - bias
                        };
                        if q < t {
                            break;
                        }
                        out.push(puny_digit(t + (q - t) % (36 - t)));
                        q = (q - t) / (36 - t);
                        k += 36;
                    }
                    out.push(puny_digit(q));
                    bias = adapt(delta, h + 1, h == basic);
                    delta = 0;
                    h += 1;
                }
                std::cmp::Ordering::Greater => {}
            }
        }
        delta = delta.checked_add(1).ok_or_else(invalid_uri)?;
        n = n.checked_add(1).ok_or_else(invalid_uri)?;
    }
    Ok(out)
}
fn punycode_decode(input: &str) -> Result<String> {
    if input.len() > 4096 || !input.is_ascii() {
        return Err(invalid_uri());
    }
    let (mut out, mut position) = if let Some(d) = input.rfind('-') {
        (input[..d].bytes().map(u32::from).collect::<Vec<_>>(), d + 1)
    } else {
        (Vec::new(), 0)
    };
    let (mut n, mut i, mut bias) = (128u64, 0u64, 72u64);
    while position < input.len() {
        let old = i;
        let (mut weight, mut k) = (1u64, 36u64);
        loop {
            let digit = input
                .as_bytes()
                .get(position)
                .and_then(|c| puny_value(*c))
                .ok_or_else(invalid_uri)?;
            position += 1;
            i = i
                .checked_add(digit.checked_mul(weight).ok_or_else(invalid_uri)?)
                .ok_or_else(invalid_uri)?;
            let t = if k <= bias {
                1
            } else if k >= bias + 26 {
                26
            } else {
                k - bias
            };
            if digit < t {
                break;
            }
            weight = weight.checked_mul(36 - t).ok_or_else(invalid_uri)?;
            k += 36;
        }
        let count = out.len() as u64 + 1;
        bias = adapt(i - old, count, old == 0);
        n = n.checked_add(i / count).ok_or_else(invalid_uri)?;
        if n > 0x10ffff || (0xd800..=0xdfff).contains(&n) {
            return Err(invalid_uri());
        }
        i %= count;
        out.insert(i as usize, n as u32);
        if out.len() > 1024 {
            return Err(invalid_uri());
        }
        i += 1;
    }
    out.into_iter()
        .map(|p| char::from_u32(p).ok_or_else(invalid_uri))
        .collect()
}
fn in_ranges(cp: u32, ranges: &[(u32, u32)]) -> bool {
    let index = ranges.partition_point(|(_, end)| *end < cp);
    ranges.get(index).is_some_and(|(start, _)| *start <= cp)
}
fn forbidden_unicode(c: char) -> bool {
    let cp = c as u32;
    cp <= 32
        || cp == 127
        || cp == 0xfffd
        || "#/:<>?@[\\]^|%".contains(c)
        || in_ranges(cp, idna_table::REJECTED)
}
fn supported_label(label: &str) -> bool {
    if label.chars().any(forbidden_unicode) {
        return false;
    }
    let rtl = |c: char| in_ranges(c as u32, idna_table::RTL_LETTERS);
    let arabic_digit = |c: char| in_ranges(c as u32, idna_table::ARABIC_DIGITS);
    if !label.chars().any(|c| rtl(c) || arabic_digit(c)) {
        return true;
    }
    // A deliberately conservative subset of RTL labels: RTL start, non-hyphen
    // end, RTL letters/digits and interior hyphens only, one digit convention.
    if !label.chars().next().is_some_and(rtl)
        || label.ends_with('-')
        || !label
            .chars()
            .all(|c| rtl(c) || arabic_digit(c) || c.is_ascii_digit() || c == '-')
    {
        return false;
    }
    !(label.chars().any(arabic_digit) && label.bytes().any(|b| b.is_ascii_digit()))
}
fn map_host(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        let c = match c {
            '\u{3002}' | '\u{ff0e}' | '\u{ff61}' => '.',
            '\u{ad}' => continue,
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            _ => c,
        };
        if let Ok(index) = idna_table::LOWERCASE.binary_search_by_key(&(c as u32), |(cp, _)| *cp) {
            out.push_str(idna_table::LOWERCASE[index].1);
        } else {
            out.push(c);
        }
    }
    out
}
fn host(raw: &str) -> Result<String> {
    if raw.is_empty() {
        return Err(invalid_uri());
    }
    if raw.starts_with('[') {
        return ipv6(raw);
    }
    let decoded = strict_decode(raw, "host").map_err(|_| invalid_uri())?;
    let mapped = map_host(&decoded);
    let mut labels = Vec::new();
    for label in mapped.split('.') {
        if !supported_label(label) {
            return Err(invalid_uri());
        }
        if let Some(encoded) = label.strip_prefix("xn--") {
            let decoded = punycode_decode(encoded)?;
            if decoded.is_ascii()
                || !supported_label(&decoded)
                || map_host(&decoded) != decoded
                || format!("xn--{}", punycode_encode(&decoded)?) != label
            {
                return Err(invalid_uri());
            }
            labels.push(label.to_owned());
        } else if label.is_ascii() {
            labels.push(label.to_owned());
        } else {
            labels.push(format!("xn--{}", punycode_encode(label)?));
        }
    }
    let value = labels.join(".");
    if value.is_empty()
        || value
            .bytes()
            .any(|b| b <= 32 || b == 127 || b"#/:<>?@[\\]^|%".contains(&b))
    {
        return Err(invalid_uri());
    }
    let pieces: Vec<&str> = value
        .strip_suffix('.')
        .unwrap_or(&value)
        .split('.')
        .collect();
    let last = pieces.last().copied().unwrap_or("");
    let numeric_end = digits(last)
        || last
            .strip_prefix("0x")
            .is_some_and(|s| s.bytes().all(|b| hex(b).is_some()));
    if !numeric_end {
        return Ok(value);
    }
    if pieces.len() > 4 {
        return Err(invalid_uri());
    }
    let mut address = 0u64;
    for (i, piece) in pieces.iter().enumerate() {
        if piece.is_empty() {
            return Err(invalid_uri());
        }
        let (radix, body) = if let Some(s) = piece.strip_prefix("0x") {
            (16u64, s)
        } else if piece.len() > 1 && piece.starts_with('0') {
            (8u64, &piece[1..])
        } else {
            (10u64, *piece)
        };
        let mut number = 0u64;
        for b in body.bytes() {
            let d = hex(b)
                .filter(|d| (*d as u64) < radix)
                .ok_or_else(invalid_uri)? as u64;
            number = number
                .checked_mul(radix)
                .and_then(|n| n.checked_add(d))
                .filter(|n| *n <= 0xffffffff)
                .ok_or_else(invalid_uri)?;
        }
        if i + 1 < pieces.len() {
            if number > 255 {
                return Err(invalid_uri());
            }
            address += number << (8 * (3 - i));
        } else {
            if number >= (1u64 << (8 * (5 - pieces.len()))) {
                return Err(invalid_uri());
            }
            address += number;
        }
    }
    Ok([24, 16, 8, 0]
        .iter()
        .map(|s| ((address >> s) & 255).to_string())
        .collect::<Vec<_>>()
        .join("."))
}
fn ipv6(raw: &str) -> Result<String> {
    let inner = raw
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(invalid_uri)?;
    let address: Ipv6Addr = inner.parse().map_err(|_| invalid_uri())?;
    let groups = address.segments();
    let (mut best_start, mut best_len) = (0, 1);
    let mut i = 0;
    while i < 8 {
        if groups[i] != 0 {
            i += 1;
            continue;
        }
        let start = i;
        while i < 8 && groups[i] == 0 {
            i += 1;
        }
        if i - start > best_len {
            best_start = start;
            best_len = i - start;
        }
    }
    let mut out = String::from("[");
    i = 0;
    while i < 8 {
        if best_len > 1 && i == best_start {
            out.push_str("::");
            i += best_len;
        } else {
            if i > 0 && !(best_len > 1 && i == best_start + best_len) {
                out.push(':');
            }
            out.push_str(&format!("{:x}", groups[i]));
            i += 1;
        }
    }
    out.push(']');
    Ok(out)
}
fn normalize_path(path: &str, base: bool) -> Result<String> {
    if path.bytes().filter(|b| *b == b'/').count() > MAX_ELEMENTS {
        return Err(failure(
            "GS1 Digital Link path component count exceeds limit",
        ));
    }
    let parts: Vec<&str> = path.split('/').collect();
    let mut out = Vec::new();
    let mut primary_seen = false;
    for (i, part) in parts.iter().enumerate() {
        if !base && is_primary(part) {
            primary_seen = true;
        }
        let dot = part.replace("%2e", ".").replace("%2E", ".");
        if !primary_seen && (dot == "." || dot == "..") {
            if dot == ".." && out.len() > 1 {
                out.pop();
            }
            if i + 1 == parts.len() {
                out.push("");
            }
        } else {
            out.push(*part);
        }
    }
    let result = out.join("/");
    Ok(encode(
        &if result.starts_with('/') {
            result
        } else {
            format!("/{result}")
        },
        2,
    ))
}
fn url(input: &str, base: bool) -> Result<Url> {
    text(input, "GS1 Digital Link URI")?;
    let clean: String = input
        .trim_matches(|c| c <= '\u{20}')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    let colon = clean.find(':').ok_or_else(invalid_uri)?;
    let scheme = &clean[..colon];
    if scheme.is_empty()
        || !scheme.as_bytes()[0].is_ascii_alphabetic()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+.-".contains(&b))
    {
        return Err(invalid_uri());
    }
    let scheme = scheme.to_ascii_lowercase();
    let mut rest = &clean[colon + 1..];
    let mut fragment = None;
    let mut query = None;
    if let Some(hash) = rest.find('#') {
        fragment = Some(rest[hash + 1..].into());
        rest = &rest[..hash];
    }
    if let Some(question) = rest.find('?') {
        query = Some(encode(&rest[question + 1..], 3));
        rest = &rest[..question];
    }
    if !matches!(scheme.as_str(), "http" | "https" | "ftp" | "ws" | "wss") {
        return Ok(Url {
            scheme,
            authority: String::new(),
            path: rest.into(),
            query,
            fragment,
        });
    }
    let rest = rest.replace('\\', "/");
    let rest = rest.trim_start_matches('/');
    let (authority, path) = rest
        .split_once('/')
        .map_or((rest, "/".to_owned()), |(a, p)| (a, format!("/{p}")));
    if authority.is_empty() {
        return Err(invalid_uri());
    }
    let (user, authority) = if let Some((u, a)) = authority.rsplit_once('@') {
        let u = credentials(u);
        (if u.is_empty() { u } else { u + "@" }, a)
    } else {
        (String::new(), authority)
    };
    let (hostname, port) = if authority.starts_with('[') {
        let end = authority.find(']').ok_or_else(invalid_uri)?;
        let suffix = &authority[end + 1..];
        let port = if suffix.is_empty() {
            ""
        } else {
            suffix.strip_prefix(':').ok_or_else(invalid_uri)?
        };
        (&authority[..end + 1], port)
    } else {
        authority.split_once(':').unwrap_or((authority, ""))
    };
    let port = if port.is_empty() {
        String::new()
    } else {
        if !digits(port) {
            return Err(invalid_uri());
        }
        let mut n = 0u32;
        for b in port.bytes() {
            n = n
                .checked_mul(10)
                .and_then(|n| n.checked_add((b - b'0') as u32))
                .filter(|n| *n <= 65535)
                .ok_or_else(invalid_uri)?;
        }
        if ((scheme == "http" || scheme == "ws") && n == 80)
            || ((scheme == "https" || scheme == "wss") && n == 443)
            || (scheme == "ftp" && n == 21)
        {
            String::new()
        } else {
            format!(":{n}")
        }
    };
    Ok(Url {
        scheme,
        authority: user + &host(hostname)? + &port,
        path: normalize_path(&path, base)?,
        query,
        fragment,
    })
}
fn check_uri(url: &Url, base: bool) -> Result<()> {
    if url.scheme != "http" && url.scheme != "https" {
        return Err(failure("GS1 Digital Link URI must use http or https"));
    }
    if base {
        if url.query.as_ref().is_some_and(|s| !s.is_empty())
            || url.fragment.as_ref().is_some_and(|s| !s.is_empty())
        {
            return Err(failure(
                "GS1 Digital Link baseUrl must not include query or fragment components",
            ));
        }
    } else if url.fragment.as_ref().is_some_and(|s| !s.is_empty()) {
        return Err(failure("GS1 Digital Link URI must not include a fragment"));
    }
    Ok(())
}
fn primary(ai: &str) -> Result<&str> {
    if !is_primary(ai) {
        return Err(failure(
            "GS1 Digital Link primaryAi must be one of 00, 01, or 414",
        ));
    }
    Ok(ai)
}
fn policy(value: &str) -> Result<&str> {
    if !matches!(value, "preserve" | "reject") {
        return Err(failure(
            "GS1 Digital Link unknownQuery must be \"preserve\" or \"reject\"",
        ));
    }
    Ok(value)
}
fn eligible(ai: &str, primary: &str) -> bool {
    primary == "01" && matches!(ai, "10" | "21" | "22")
}
fn placement(ai: &str, primary: &str) -> Result<()> {
    if get_ai_info(ai).is_none() {
        return Err(failure(format!(
            "Unsupported GS1 AI {ai}. Add explicit support before using it."
        )));
    }
    if !eligible(ai, primary) {
        return Err(failure(format!(
            "GS1 AI {ai} cannot be placed in the Digital Link path after primary AI {primary}"
        )));
    }
    Ok(())
}
fn unique(ai: &str, seen: &mut HashSet<String>) -> Result<()> {
    if !seen.insert(ai.into()) {
        return Err(failure(format!(
            "GS1 Digital Link input must not contain duplicate AI {ai}"
        )));
    }
    Ok(())
}
/// Creates a link with default primary AI `01` and the provided absolute base.
pub fn create_digital_link(elements: &[Element], base_url: &str) -> Result<String> {
    create_digital_link_with_options(elements, &DigitalLinkOptions::for_base_url(base_url))
}
pub fn create_digital_link_with_options(
    elements: &[Element],
    options: &DigitalLinkOptions,
) -> Result<String> {
    bounded(elements)?;
    let base = options
        .base_url
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| failure("GS1 Digital Link baseUrl is required"))?;
    let url = url(base, true)?;
    check_uri(&url, true)?;
    let primary_ai = primary(options.primary_ai.as_deref().unwrap_or("01"))?;
    let paths = if let Some(paths) = &options.path_ais {
        if paths.len() > MAX_ELEMENTS {
            return Err(failure("GS1 Digital Link pathAis exceeds element limit"));
        }
        let mut selected = HashSet::new();
        for ai in paths {
            if !is_ai(ai) {
                return Err(failure(
                    "GS1 Digital Link pathAis entries must be 2 to 4 digit AI strings",
                ));
            }
            if ai != primary_ai {
                placement(ai, primary_ai)?;
                selected.insert(ai.as_str());
            }
        }
        Some(selected)
    } else {
        None
    };
    if elements.is_empty() {
        return Err(failure("GS1 Digital Link input elements must not be empty"));
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for (i, e) in elements.iter().enumerate() {
        let e = element(e, i)?;
        unique(&e.ai, &mut seen)?;
        normalized.push(e);
    }
    let selected = normalized
        .iter()
        .find(|e| e.ai == primary_ai)
        .ok_or_else(|| {
            failure(format!(
                "GS1 Digital Link input must include primary AI {primary_ai}"
            ))
        })?;
    let mut path = vec![selected];
    let mut query = Vec::new();
    for e in &normalized {
        if e.ai == primary_ai {
            continue;
        }
        let in_path = paths.as_ref().map_or_else(
            || eligible(&e.ai, primary_ai),
            |p| p.contains(e.ai.as_str()),
        );
        if in_path && !matches!(e.value.as_str(), "." | "..") {
            path.push(e);
        } else {
            query.push(e);
        }
    }
    let mut pathname = url.path.trim_end_matches('/').to_owned();
    for e in path {
        pathname.push('/');
        pathname.push_str(&encode(&e.ai, 0));
        pathname.push('/');
        pathname.push_str(&encode(&e.value, 0));
    }
    query.sort_by(|a, b| a.ai.cmp(&b.ai).then_with(|| a.value.cmp(&b.value)));
    let search = query
        .iter()
        .map(|e| encode(&e.ai, 1) + "=" + &encode(&e.value, 1))
        .collect::<Vec<_>>()
        .join("&");
    Url {
        scheme: url.scheme,
        authority: url.authority,
        path: pathname,
        query: if search.is_empty() {
            None
        } else {
            Some(search)
        },
        fragment: url.fragment,
    }
    .serialize()
}
fn path_parts(path: &str) -> Result<Vec<&str>> {
    let value = path.trim_matches('/');
    if value.is_empty() {
        return Err(failure(
            "GS1 Digital Link path must include primary AI 00, 01, or 414",
        ));
    }
    let parts: Vec<&str> = value.split('/').collect();
    if parts.contains(&"") {
        return Err(failure(
            "GS1 Digital Link path must not contain empty segments",
        ));
    }
    Ok(parts)
}
fn first_ai(parts: &[&str], selected: Option<&str>) -> Result<usize> {
    parts
        .iter()
        .position(|part| selected.map_or_else(|| is_primary(part), |s| s == *part))
        .ok_or_else(|| failure("GS1 Digital Link path must include primary AI 00, 01, or 414"))
}
fn parse_link(url: &Url, options: &DigitalLinkOptions) -> Result<DigitalLinkParseResult> {
    check_uri(url, false)?;
    if let Some(ai) = &options.primary_ai {
        primary(ai)?;
    }
    let unknown_policy = policy(&options.unknown_query)?;
    let parts = path_parts(&url.path)?;
    let start = first_ai(&parts, options.primary_ai.as_deref())?;
    if (parts.len() - start) % 2 != 0 {
        return Err(failure("GS1 Digital Link path must contain AI/value pairs"));
    }
    let mut path: Vec<Element> = Vec::new();
    let mut query = Vec::new();
    let mut unknown = Vec::new();
    let mut seen = HashSet::new();
    for i in (start..parts.len()).step_by(2) {
        let ai = parts[i];
        if !is_ai(ai) {
            return Err(failure(format!(
                "GS1 Digital Link path segment {} must be a GS1 AI",
                i + 1
            )));
        }
        let e = element(
            &Element::new(
                ai,
                strict_decode(parts[i + 1], &format!("value for AI {ai}"))?,
            ),
            path.len(),
        )?;
        if let Some(primary) = path.first() {
            placement(ai, &primary.ai)?;
        }
        unique(ai, &mut seen)?;
        path.push(e);
    }
    if let Some(raw) = &url.query {
        if raw.bytes().filter(|b| *b == b'&').count() + 1 > MAX_ELEMENTS {
            return Err(failure(
                "GS1 Digital Link query component count exceeds limit",
            ));
        }
        for pair in raw.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            let key = form_decode(k);
            let value = form_decode(v);
            if is_ai(&key) {
                let e = element(&Element::new(&key, value), path.len() + query.len())?;
                unique(&key, &mut seen)?;
                query.push(e);
            } else if unknown_policy == "preserve" {
                unknown.push(UnknownQuery { key, value });
            } else {
                return Err(failure(format!(
                    "GS1 Digital Link query parameter \"{key}\" is not a GS1 AI"
                )));
            }
        }
    }
    let primary = path[0].clone();
    let elements = path.iter().chain(query.iter()).cloned().collect();
    Ok(DigitalLinkParseResult {
        elements,
        primary,
        path_elements: path,
        query_elements: query,
        unknown_query: unknown,
    })
}
pub fn parse_digital_link(uri: &str) -> Result<DigitalLinkParseResult> {
    parse_digital_link_with_options(uri, &DigitalLinkOptions::default())
}
pub fn parse_digital_link_with_options(
    uri: &str,
    options: &DigitalLinkOptions,
) -> Result<DigitalLinkParseResult> {
    parse_link(&url(uri, false)?, options)
}
pub fn validate_digital_link(uri: &str) -> DigitalLinkValidationResult {
    validate_digital_link_with_options(uri, &DigitalLinkOptions::default())
}
pub fn validate_digital_link_with_options(
    uri: &str,
    options: &DigitalLinkOptions,
) -> DigitalLinkValidationResult {
    let fail = |e| DigitalLinkValidationResult {
        ok: false,
        result: None,
        errors: vec![e],
        warnings: Vec::new(),
    };
    if options.normalize {
        return fail(simple_issue(
            "GS1_INVALID_INPUT",
            "GS1 Digital Link validation normalize is not implemented yet",
            "unsupported-option",
            Some(Expected::Boolean(false)),
        ));
    }
    let url = match url(uri, false) {
        Ok(url) => url,
        Err(e) => return fail(issue(e, None, None, None, true)),
    };
    if invalid_percent(&url.path) || url.query.as_ref().is_some_and(|s| invalid_percent(s)) {
        return fail(issue(
            failure("GS1 Digital Link URI must use valid percent-encoding"),
            None,
            None,
            None,
            true,
        ));
    }
    match parse_link(&url, options) {
        Ok(result) => {
            let mut warnings = Vec::new();
            if url.scheme == "http" {
                warnings.push(simple_issue("GS1_DIGITAL_LINK_HTTP","GS1 Digital Link URI uses http. Use https when transport security is required.","http-uri",None));
            }
            if !result.unknown_query.is_empty() {
                let mut warning = simple_issue(
                    "GS1_DIGITAL_LINK_UNKNOWN_QUERY_PRESERVED",
                    "GS1 Digital Link URI contains non-GS1 query parameters preserved in unknownQuery.",
                    "unknown-query-preserved",
                    None,
                );
                warning.count = Some(result.unknown_query.len());
                warnings.push(warning);
            }
            DigitalLinkValidationResult {
                ok: true,
                result: Some(result),
                errors: Vec::new(),
                warnings,
            }
        }
        Err(e) => fail(issue(e, None, None, None, true)),
    }
}
pub fn normalize_digital_link(uri: &str) -> Result<String> {
    normalize_digital_link_with_options(uri, &DigitalLinkOptions::default())
}
pub fn normalize_digital_link_with_options(
    uri: &str,
    options: &DigitalLinkOptions,
) -> Result<String> {
    if options.mode != "specqr-deterministic" {
        return Err(failure(
            "GS1 Digital Link normalization mode must be \"specqr-deterministic\"",
        ));
    }
    let url = url(uri, false)?;
    check_uri(&url, false)?;
    if invalid_percent(&url.path) || url.query.as_ref().is_some_and(|s| invalid_percent(s)) {
        return Err(failure(
            "GS1 Digital Link URI must use valid percent-encoding",
        ));
    }
    let parsed = parse_link(&url, options)?;
    let parts = path_parts(&url.path)?;
    let start = first_ai(&parts, options.primary_ai.as_deref())?;
    let stem = Url {
        scheme: url.scheme,
        authority: url.authority,
        path: format!("/{}", parts[..start].join("/")),
        query: None,
        fragment: None,
    }
    .serialize()?;
    let mut out = create_digital_link_with_options(
        &parsed.elements,
        &DigitalLinkOptions::for_base_url(stem).with_primary_ai(&parsed.primary.ai),
    )?;
    let mut query = out.contains('?');
    for q in parsed.unknown_query {
        out.push(if query { '&' } else { '?' });
        query = true;
        out.push_str(&encode(&q.key, 1));
        out.push('=');
        out.push_str(&encode(&q.value, 1));
    }
    text(&out, "GS1 Digital Link output")?;
    Ok(out)
}
