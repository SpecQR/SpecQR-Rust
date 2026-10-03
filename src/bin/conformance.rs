//! Development-only JSON-lines adapter to the actual Rust implementation.
//! No reference encoder, expected output, network client or external dependency.
#![forbid(unsafe_code)]
use specqr::json::{self, Value};
use specqr::{self, Ecc, Error, ErrorCode, Mode, Result, Segment};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    str::FromStr,
};

fn bad(message: &str) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}
fn get<'a>(r: &'a Value, key: &str) -> Result<&'a Value> {
    r.get(key)
        .ok_or_else(|| bad(&format!("Missing field {key}")))
}
fn string(v: &Value) -> Result<&str> {
    v.as_str().ok_or_else(|| bad("Expected string"))
}
fn integer(v: &Value) -> Result<i64> {
    v.as_i64().ok_or_else(|| bad("Expected integer"))
}
fn number(v: &Value) -> Result<usize> {
    usize::try_from(integer(v)?).map_err(|_| bad("Expected non-negative integer"))
}
fn byte(v: &Value) -> Result<u8> {
    u8::try_from(integer(v)?).map_err(|_| bad("Expected integer in 0..255"))
}
fn boolean(v: &Value) -> Result<bool> {
    v.as_bool().ok_or_else(|| bad("Expected boolean"))
}
fn bytes(v: &Value) -> Result<Vec<u8>> {
    v.as_array()
        .ok_or_else(|| bad("Expected byte array"))?
        .iter()
        .map(byte)
        .collect()
}
fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::object(pairs)
}
fn hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(data.len() * 2);
    for &value in data {
        out.push(char::from(HEX[usize::from(value >> 4)]));
        out.push(char::from(HEX[usize::from(value & 15)]));
    }
    out
}

fn rows(matrix: &[Vec<bool>]) -> Vec<String> {
    matrix
        .iter()
        .map(|row| row.iter().map(|v| if *v { '1' } else { '0' }).collect())
        .collect()
}
fn matrix_hash(matrix: &[Vec<bool>]) -> String {
    sha256(rows(matrix).concat().as_bytes())
}
fn typed_error(e: &Error) -> Value {
    obj([
        ("error", "SpecQrError".into()),
        ("message", e.message().into()),
        ("isSpecQRError", true.into()),
        ("code", e.code().into()),
    ])
}
fn outcome(result: Result<String>) -> Value {
    match result {
        Ok(value) => obj([("ok", true.into()), ("value", value.into())]),
        Err(e) => obj([("ok", false.into()), ("code", e.code().into())]),
    }
}

fn segments(v: &Value) -> Result<Vec<Segment>> {
    let array = v.as_array().ok_or_else(|| bad("Segments must be array"))?;
    if array.len() > 16_384 {
        return Err(bad("Too many segments"));
    }
    let mut out = Vec::with_capacity(array.len());
    for r in array {
        let mode = string(get(r, "mode")?)?;
        let fields: &[&str] = match mode {
            "numeric" | "alphanumeric" | "byte" | "kanji" => &["mode", "text", "bytes"],
            "eci" => &["mode", "assignmentNumber"],
            "fnc1" => &["mode"],
            "fnc1-second" => &["mode", "applicationIndicator"],
            "structured-append" => &["mode", "index", "total", "parity"],
            _ => return Err(bad("Unknown segment mode")),
        };
        if r.as_object()
            .ok_or_else(|| bad("Segment must be object"))?
            .keys()
            .any(|key| !fields.contains(&key.as_str()))
        {
            return Err(bad("Unknown segment field"));
        }
        if ["numeric", "alphanumeric", "byte", "kanji"].contains(&mode)
            && r.get("text").is_some() == r.get("bytes").is_some()
        {
            return Err(bad("Data segment needs exactly one text or bytes field"));
        }
        out.push(match mode {
            "numeric" => Segment::numeric(string(get(r, "text")?)?)?,
            "alphanumeric" => Segment::alphanumeric(string(get(r, "text")?)?)?,
            "kanji" => Segment::kanji(string(get(r, "text")?)?)?,
            "byte" => {
                if let Some(text) = r.get("text") {
                    Segment::utf8(string(text)?)?
                } else {
                    Segment::bytes(&bytes(get(r, "bytes")?)?)?
                }
            }
            "eci" => Segment::eci(
                u32::try_from(integer(get(r, "assignmentNumber")?)?)
                    .map_err(|_| bad("Invalid ECI"))?,
            )?,
            "fnc1" => Segment::fnc1(),
            "fnc1-second" => Segment::fnc1_second(string(get(r, "applicationIndicator")?)?)?,
            "structured-append" => Segment::structured_append(
                byte(get(r, "index")?)?,
                byte(get(r, "total")?)?,
                byte(get(r, "parity")?)?,
            )?,
            _ => return Err(bad("Unknown segment mode")),
        });
    }
    specqr::segment::normalize_segments(&out)
}

fn raw(r: &Value) -> Result<Value> {
    let version = byte(get(r, "version")?)?;
    let seed = number(get(r, "seed")?)?;
    let mask = integer(get(r, "mask")?)?;
    let ecc = Ecc::from_str(string(get(r, "ecc")?)?)?;
    let ordinal = "LMQH".find(ecc.as_str()).unwrap();
    let length = specqr::tables::data_codeword_count(version, ecc)?;
    let data: Vec<u8> = (0..length)
        .map(|i| {
            if seed == 0 {
                0
            } else if seed == 1 {
                255
            } else {
                (((i * 149 + usize::from(version) * 43 + ordinal * 89 + seed * 67)
                    ^ (i >> (seed + 1)))
                    & 255) as u8
            }
        })
        .collect();
    let encoded = specqr::core::interleave_codewords(&data, version, ecc)?;
    let matrix = specqr::core::build_matrix(
        encoded.codewords(),
        version,
        ecc,
        if mask < 0 {
            None
        } else {
            Some(u8::try_from(mask).map_err(|_| bad("Invalid mask"))?)
        },
    )?;
    Ok(obj([
        ("data", hex(&data).into()),
        ("codewords", hex(encoded.codewords()).into()),
        ("matrixHash", matrix_hash(matrix.matrix()).into()),
        ("mask", matrix.mask_pattern().into()),
        ("penalty", matrix.penalty().into()),
        (
            "penalties",
            Value::array(matrix.mask_penalties().iter().map(|p| p.penalty().into())),
        ),
    ]))
}

fn identity(r: &Value) -> Result<Value> {
    let path = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|_| bad("Cannot locate executable"))?;
    let digest = sha256(&std::fs::read(&path).map_err(|_| bad("Cannot fingerprint executable"))?);
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or_else(|| bad("Non-UTF8 executable filename"))?;
    Ok(obj([
        ("language", "Rust".into()),
        ("packageVersion", specqr::VERSION.into()),
        ("nonce", get(r, "nonce")?.clone()),
        ("pid", std::process::id().into()),
        ("executable", path.to_string_lossy().as_ref().into()),
        ("binarySha256", digest.clone().into()),
        (
            "packageFilesSha256",
            Value::Object(BTreeMap::from([(name.to_string(), digest.into())])),
        ),
        ("runtimeDependencies", Value::Array(vec![])),
    ]))
}

fn gs1_elements(v: &Value) -> Result<Vec<specqr::gs1::Element>> {
    v.as_array()
        .ok_or_else(|| bad("Elements must be array"))?
        .iter()
        .map(|r| {
            Ok(specqr::gs1::Element::new(
                string(get(r, "ai")?)?,
                string(get(r, "value")?)?,
            ))
        })
        .collect()
}
fn element_pairs(elements: &[specqr::gs1::Element]) -> Value {
    Value::array(
        elements
            .iter()
            .map(|v| Value::array([v.ai().into(), v.value().into()])),
    )
}
fn diagnostic(v: &specqr::gs1::ValidationIssue) -> Value {
    let expected = match v.expected() {
        Some(specqr::gs1::Expected::Text(s)) => s.clone().into(),
        Some(specqr::gs1::Expected::Boolean(b)) => (*b).into(),
        None => Value::Null,
    };
    obj([
        ("code", v.code().into()),
        ("reason", v.reason().into()),
        ("ai", v.ai().into()),
        ("value", v.value().into()),
        ("key", v.key().into()),
        ("offset", v.offset().into()),
        ("elementIndex", v.element_index().into()),
        ("expected", expected),
        ("count", v.count().into()),
    ])
}
fn gs1_dispatch(r: &Value, command: &str) -> Result<Value> {
    use specqr::gs1;
    match command {
        "catalog" => {
            let mut values = Vec::new();
            for info in gs1::get_supported_ais() {
                let length = if info.length().is_variable() {
                    obj([
                        ("type", "variable".into()),
                        ("min", info.length().min().into()),
                        ("max", info.length().max().into()),
                    ])
                } else {
                    obj([
                        ("type", "fixed".into()),
                        ("exact", info.length().exact().into()),
                    ])
                };
                let mut entry = obj([
                    ("ai", info.ai().into()),
                    ("label", info.label().into()),
                    ("length", length),
                    ("valueKind", info.value_kind().into()),
                    ("checkDigitRule", info.check_digit_rule().into()),
                    ("digitalLinkRole", info.digital_link_role().into()),
                    ("separator", info.separator().into()),
                ]);
                if let Some(v) = info.digital_link_path_for_primary() {
                    entry.insert(
                        "digitalLinkPathForPrimary",
                        Value::array(v.iter().map(|s| (*s).into())),
                    )?;
                }
                values.push(entry);
            }
            Ok(obj([(
                "catalog",
                obj([("ok", true.into()), ("value", Value::Array(values))]),
            )]))
        }
        "url" => {
            let input = string(get(r, "input")?)?;
            let mut options = gs1::DigitalLinkOptions::default();
            if let Some(opts) = r.get("options") {
                if let Some(v) = opts.get("primaryAi") {
                    options = options.with_primary_ai(string(v)?);
                }
                if let Some(v) = opts.get("unknownQuery") {
                    options = options.with_unknown_query(string(v)?);
                }
                if let Some(v) = opts.get("normalize") {
                    options = options.with_normalize(boolean(v)?);
                }
            }
            let parse = match gs1::parse_digital_link_with_options(input, &options) {
                Ok(v) => obj([
                    ("ok", true.into()),
                    ("elements", element_pairs(v.elements())),
                    ("path", element_pairs(v.path_elements())),
                    ("query", element_pairs(v.query_elements())),
                    (
                        "unknown",
                        Value::array(
                            v.unknown_query()
                                .iter()
                                .map(|q| Value::array([q.key().into(), q.value().into()])),
                        ),
                    ),
                ]),
                Err(e) => obj([("ok", false.into()), ("code", e.code().into())]),
            };
            let normalize = outcome(gs1::normalize_digital_link_with_options(input, &options));
            let v = gs1::validate_digital_link_with_options(input, &options);
            Ok(obj([
                ("parse", parse),
                ("normalize", normalize),
                (
                    "validate",
                    obj([
                        ("ok", v.ok().into()),
                        ("errors", Value::array(v.errors().iter().map(diagnostic))),
                        (
                            "warnings",
                            Value::array(v.warnings().iter().map(diagnostic)),
                        ),
                    ]),
                ),
            ]))
        }
        "create" => Ok(obj([(
            "create",
            outcome(gs1::create_digital_link_with_options(
                &gs1_elements(get(r, "elements")?)?,
                &gs1::DigitalLinkOptions::for_base_url(string(get(r, "baseUrl")?)?),
            )),
        )])),
        "elements" => {
            let values = gs1_elements(get(r, "elements")?)?;
            let v = gs1::validate_elements(&values);
            Ok(obj([
                (
                    "validate",
                    obj([
                        ("ok", v.ok().into()),
                        ("errors", Value::array(v.errors().iter().map(diagnostic))),
                    ]),
                ),
                ("string", outcome(gs1::to_element_string(&values))),
            ]))
        }
        _ => Err(bad("Unknown GS1 command")),
    }
}

/// SHA-256 is test identity plumbing, not an encoder dependency.
fn sha256(input: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut data = input.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&((input.len() as u64) * 8).to_be_bytes());
    for block in data.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let a = w[i - 15];
            let b = w[i - 2];
            w[i] = w[i - 16]
                .wrapping_add(a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3))
                .wrapping_add(w[i - 7])
                .wrapping_add(b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10));
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for i in 0..64 {
            let t1 = h
                .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                .wrapping_add((e & f) ^ (!e & g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
                .wrapping_add((a & b) ^ (a & c) ^ (b & c));
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *s = s.wrapping_add(v);
        }
    }
    hex(&state
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect::<Vec<_>>())
}

// Test responses may contain a full v40 scale-8 PNG (over 17 MB as hex).
// The production JSON codec's smaller resource limit is retained unchanged.
fn write_json<W: Write>(v: &Value, out: &mut W) -> io::Result<()> {
    match v {
        Value::Null => out.write_all(b"null"),
        Value::Bool(v) => out.write_all(if *v { b"true" } else { b"false" }),
        Value::Number(v) => write!(out, "{v}"),
        Value::String(s) => {
            out.write_all(b"\"")?;
            for c in s.chars() {
                match c {
                    '"' => out.write_all(b"\\\"")?,
                    '\\' => out.write_all(b"\\\\")?,
                    '\n' => out.write_all(b"\\n")?,
                    '\r' => out.write_all(b"\\r")?,
                    '\t' => out.write_all(b"\\t")?,
                    c if c < ' ' => write!(out, "\\u{:04x}", c as u32)?,
                    _ => write!(out, "{c}")?,
                }
            }
            out.write_all(b"\"")
        }
        Value::Array(a) => {
            out.write_all(b"[")?;
            for (i, v) in a.iter().enumerate() {
                if i > 0 {
                    out.write_all(b",")?;
                }
                write_json(v, out)?;
            }
            out.write_all(b"]")
        }
        Value::Object(a) => {
            out.write_all(b"{")?;
            for (i, (k, v)) in a.iter().enumerate() {
                if i > 0 {
                    out.write_all(b",")?;
                }
                write_json(&Value::String(k.clone()), out)?;
                out.write_all(b":")?;
                write_json(v, out)?;
            }
            out.write_all(b"}")
        }
    }
}

fn options(r: Option<&Value>) -> Result<specqr::Options> {
    let mut opts = specqr::Options::default();
    if let Some(r) = r {
        for (key, v) in r.as_object().ok_or_else(|| bad("Options must be object"))? {
            match key.as_str() {
                "version" => {
                    opts.version = if v.as_str() == Some("auto") {
                        None
                    } else {
                        Some(byte(v)?)
                    }
                }
                "minVersion" => opts.min_version = byte(v)?,
                "maxVersion" => opts.max_version = byte(v)?,
                "maskPattern" => {
                    opts.mask = if v.as_str() == Some("auto") {
                        None
                    } else {
                        Some(byte(v)?)
                    }
                }
                "errorCorrectionLevel" => opts.ecc = Ecc::from_str(string(v)?)?,
                "mode" => {
                    opts.mode = if v.as_str() == Some("auto") {
                        None
                    } else {
                        Some(Mode::from_str(string(v)?)?)
                    }
                }
                "optimizeSegments" => opts.optimize_segments = boolean(v)?,
                "boostErrorCorrection" => opts.boost_ecc = boolean(v)?,
                "eci" => {
                    opts.eci = if v.as_bool() == Some(false) {
                        None
                    } else {
                        Some(u32::try_from(integer(v)?).map_err(|_| bad("Invalid ECI"))?)
                    }
                }
                "gs1" => opts.gs1 = boolean(v)?,
                "fnc1Second" => {
                    opts.fnc1_second = if v.as_bool() == Some(false) {
                        None
                    } else {
                        Some(string(v)?.to_string())
                    }
                }
                "structuredAppend" => {
                    opts.structured_append = Some(Segment::structured_append(
                        byte(get(v, "index")?)?,
                        byte(get(v, "total")?)?,
                        byte(get(v, "parity")?)?,
                    )?)
                }
                "scale" => {
                    opts.render.scale =
                        u32::try_from(number(v)?).map_err(|_| bad("Invalid scale"))?
                }
                "margin" => {
                    opts.render.margin =
                        u32::try_from(number(v)?).map_err(|_| bad("Invalid margin"))?
                }
                "foreground" => opts.render.foreground = string(v)?.to_string(),
                "background" => opts.render.background = string(v)?.to_string(),
                "printDpi" => {
                    opts.print_dpi = Some(v.as_f64().ok_or_else(|| bad("DPI must be number"))?)
                }
                "maxSymbols" | "diagnostics" => {}
                "output" => {
                    if string(v)? != "matrix" {
                        return Err(bad("Conformance output must be matrix"));
                    }
                }
                _ => return Err(bad(&format!("Unknown option: {key}"))),
            }
        }
    }
    opts.validate()?;
    Ok(opts)
}
fn symbol(qr: &specqr::QrCode, r: &Value) -> Result<Value> {
    let mut out = obj([
        ("version", qr.version().into()),
        ("ecc", qr.ecc().as_str().into()),
        ("mask", qr.mask().into()),
        ("matrixHash", matrix_hash(qr.matrix()).into()),
        ("data", hex(qr.data_codewords()).into()),
        ("codewords", hex(qr.codewords()).into()),
    ]);
    if r.get("includeMatrix").and_then(Value::as_bool) == Some(true) || r.get("pngScale").is_some()
    {
        out.insert(
            "matrix",
            Value::array(rows(qr.matrix()).into_iter().map(Value::from)),
        )?;
    }
    if let Some(scale) = r.get("pngScale") {
        let render = specqr::render::RenderOptions {
            scale: u32::try_from(number(scale)?).map_err(|_| bad("Invalid scale"))?,
            margin: 4,
            ..Default::default()
        };
        out.insert("png", hex(&specqr::render::to_png(qr.matrix(), &render)?))?;
    }
    if r.get("includeDiagnostics").and_then(Value::as_bool) == Some(true) {
        out.insert("diagnostics", qr.diagnostics().clone())?;
    }
    Ok(out)
}
enum Payload {
    Text(String),
    Bytes(Vec<u8>),
}
fn payload(r: &Value) -> Result<Payload> {
    if let Some(v) = r.get("rawText") {
        return Ok(Payload::Text(
            String::from_utf8(bytes(v)?).map_err(|_| bad("Text must be valid UTF-8"))?,
        ));
    }
    if let Some(v) = r.get("bytes") {
        return Ok(Payload::Bytes(bytes(v)?));
    }
    Ok(Payload::Text(
        r.get("text")
            .map(string)
            .transpose()?
            .unwrap_or("")
            .to_string(),
    ))
}
fn append(r: &Value, opts: &specqr::Options, input: &Payload) -> Result<Value> {
    use specqr::structured_append as sa;
    let max = r
        .get("options")
        .and_then(|v| v.get("maxSymbols"))
        .map(byte)
        .transpose()?
        .unwrap_or(16);
    let full = r.get("includeDiagnostics").and_then(Value::as_bool) == Some(true);
    let diag = if full {
        sa::DiagnosticOptions::full()
    } else {
        sa::DiagnosticOptions::default()
    };
    let result = if let Some(v) = r.get("segments") {
        sa::generate_segments(&segments(v)?, opts, max, &diag)?
    } else {
        match input {
            Payload::Text(text) => sa::generate(text, opts, max, &diag)?,
            Payload::Bytes(data) => sa::generate_bytes(data, opts, max, &diag)?,
        }
    };
    let symbols = result
        .symbols()
        .iter()
        .map(|qr| symbol(qr, r))
        .collect::<Result<Vec<_>>>()?;
    let mut out = obj([
        ("total", result.total().into()),
        ("parity", result.parity().into()),
        ("inputLength", result.input_length().into()),
        ("byteLength", result.byte_length().into()),
        (
            "matrixHashes",
            Value::array(symbols.iter().map(|s| s.get("matrixHash").unwrap().clone())),
        ),
        (
            "versions",
            Value::array(symbols.iter().map(|s| s.get("version").unwrap().clone())),
        ),
        (
            "masks",
            Value::array(symbols.iter().map(|s| s.get("mask").unwrap().clone())),
        ),
        ("symbols", Value::Array(symbols)),
    ]);
    if full {
        out.insert("diagnostics", result.diagnostics().clone())?;
    }
    Ok(out)
}
fn merge_parts(r: &Value) -> Result<Value> {
    use specqr::structured_append as sa;
    let mut parts = Vec::new();
    for part in get(r, "parts")?
        .as_array()
        .ok_or_else(|| bad("Parts must be array"))?
    {
        if part.get("text").is_some() == part.get("bytes").is_some() {
            return Err(bad("Part needs exactly one text or bytes payload"));
        }
        let data = if let Some(v) = part.get("text") {
            sa::PartData::Text(string(v)?.to_string())
        } else {
            sa::PartData::Bytes(bytes(get(part, "bytes")?)?)
        };
        parts.push(sa::Part::new(
            byte(get(part, "index")?)?,
            byte(get(part, "total")?)?,
            byte(get(part, "parity")?)?,
            data,
        )?);
    }
    let merged = sa::merge(&parts)?;
    let data = match merged.data() {
        sa::PartData::Text(s) => s.clone().into(),
        sa::PartData::Bytes(b) => Value::array(b.iter().map(|v| (*v).into())),
    };
    Ok(obj([
        ("data", data),
        ("total", merged.total().into()),
        ("parity", merged.parity().into()),
        (
            "parts",
            Value::array(merged.parts().iter().map(|p| {
                obj([
                    ("index", p.index().into()),
                    ("total", p.total().into()),
                    ("parity", p.parity().into()),
                    ("dataType", p.data_type().into()),
                    ("byteLength", p.byte_length().into()),
                ])
            })),
        ),
        ("diagnostics", merged.diagnostics().clone()),
    ]))
}

fn dispatch(r: &Value) -> Result<Value> {
    let command = r
        .get("command")
        .map(string)
        .transpose()?
        .unwrap_or("generate");
    match command {
        "identity" => return identity(r),
        "merge" => return merge_parts(r),
        "raw" => return raw(r),
        "catalog" | "url" | "create" | "elements" => return gs1_dispatch(r, command),
        "gf" => {
            let data: Vec<u8> = (0..=255)
                .flat_map(|a| (0..=255).map(move |b| specqr::core::gf_multiply(a, b)))
                .collect();
            return Ok(obj([("bytes", hex(&data).into())]));
        }
        "rs" => {
            let degree = byte(get(r, "degree")?)?;
            let data: Vec<u8> = (0..300)
                .map(|i| (i * 61 + usize::from(degree)) as u8)
                .collect();
            return Ok(obj([
                (
                    "generator",
                    hex(&specqr::core::reed_solomon_divisor(degree)?).into(),
                ),
                (
                    "remainder",
                    hex(&specqr::core::reed_solomon_remainder(&data, degree)?).into(),
                ),
            ]));
        }
        "concurrency" => {
            let tasks = get(r, "requests")?
                .as_array()
                .ok_or_else(|| bad("Expected requests array"))?;
            if tasks.len() > 512
                || tasks
                    .iter()
                    .any(|r| r.get("command").and_then(Value::as_str) == Some("concurrency"))
            {
                return Err(bad("Unbounded or nested concurrency"));
            }
            let results = std::thread::scope(|scope| {
                let chunk = (tasks.len() / 8).max(1);
                let handles: Vec<_> = tasks
                    .chunks(chunk)
                    .map(|group| {
                        scope.spawn(move || {
                            group
                                .iter()
                                .map(|task| dispatch(task).unwrap_or_else(|e| typed_error(&e)))
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .flat_map(|h| h.join().expect("Conformance worker panicked"))
                    .collect::<Vec<_>>()
            });
            return Ok(obj([("results", Value::Array(results))]));
        }
        _ => {}
    }
    let opts = options(r.get("options"))?;
    if command == "capacity" {
        let c = specqr::get_capacity(
            opts.version.ok_or_else(|| bad("Capacity needs version"))?,
            opts.ecc,
            opts.mode,
            0,
        )?;
        return Ok(obj([
            ("maximum", c.maximum().into()),
            ("dataCodewords", c.data_codewords().into()),
            ("capacityBits", c.capacity_bits().into()),
            ("countBits", c.character_count_bits().into()),
        ]));
    }
    let input = payload(r)?;
    if command == "estimate" {
        let p = if let Some(v) = r.get("segments") {
            specqr::analyze_segments(&segments(v)?, &opts)?
        } else {
            match &input {
                Payload::Text(text) => specqr::estimate(text, &opts)?,
                Payload::Bytes(data) => specqr::estimate_bytes(data, &opts)?,
            }
        };
        let mut out = obj([
            ("fits", p.ok().into()),
            ("version", p.version().into()),
            ("requiredBits", p.required_bits().into()),
            ("capacityBits", p.capacity_bits().into()),
        ]);
        if r.get("includeDiagnostics").and_then(Value::as_bool) == Some(true) {
            out.insert("diagnostics", p.diagnostics().clone())?;
        }
        return Ok(out);
    }
    if command == "structured-append" {
        return append(r, &opts, &input);
    }
    if command != "generate" {
        return Err(bad("Unknown conformance command"));
    }
    let qr = if let Some(v) = r.get("segments") {
        specqr::generate_segments(&segments(v)?, &opts)?
    } else {
        match input {
            Payload::Text(text) => specqr::generate(&text, &opts)?,
            Payload::Bytes(data) => specqr::generate_bytes(&data, &opts)?,
        }
    };
    symbol(&qr, r)
}
fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line?;
        let request = json::parse(&line);
        let is_identity = request
            .as_ref()
            .ok()
            .and_then(|r| r.get("command"))
            .and_then(Value::as_str)
            == Some("identity");
        let mut out = match request {
            Ok(r) => dispatch(&r).unwrap_or_else(|e| typed_error(&e)),
            Err(e) => typed_error(&e),
        };
        let fault = if is_identity {
            String::new()
        } else {
            std::env::var("SPECQR_TEST_FAULT").unwrap_or_default()
        };
        match fault.as_str() {
            "exit" => std::process::exit(73),
            "drop" => continue,
            "error" => {
                out = obj([
                    ("error", "InjectedFailure".into()),
                    ("message", "Test-only negative control".into()),
                ])
            }
            "gs1-catalog" => {
                if out.get("catalog").is_some() {
                    out.insert(
                        "catalog",
                        obj([
                            ("ok", true.into()),
                            ("value", Value::array(["deliberate corruption".into()])),
                        ]),
                    )
                    .unwrap();
                }
            }
            "sa-diagnostics" => {
                if let Some(Value::Object(d)) =
                    out.as_object().and_then(|m| m.get("diagnostics")).cloned()
                {
                    let mut value = Value::Object(d);
                    value.insert("parity", 999usize).unwrap();
                    out.insert("diagnostics", value).unwrap();
                }
            }
            "merge-data" => {
                if out.get("data").is_some() {
                    out.insert("data", "deliberate corruption").unwrap();
                }
            }
            "matrixHash" | "data" | "codewords" => {
                if let Some(v) = out.get(&fault).and_then(Value::as_str) {
                    let mut value = v.as_bytes().to_vec();
                    if !value.is_empty() {
                        let index = if fault == "codewords" {
                            value.len() - 1
                        } else {
                            0
                        };
                        value[index] = if value[index] == b'1' { b'0' } else { b'1' };
                        out.insert(fault.clone(), String::from_utf8(value).unwrap())
                            .unwrap();
                    }
                }
            }
            value if value.starts_with("leak-") => {
                out = obj([
                    ("error", value[5..].into()),
                    ("message", "Test-only untyped error leak".into()),
                    ("isSpecQRError", false.into()),
                    ("code", Value::Null),
                ])
            }
            _ => {}
        }
        write_json(&out, &mut stdout)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_sha_vectors() {
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
