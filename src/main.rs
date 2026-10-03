//! Dependency-free command line. For executable examples use `specqr --help`.
use specqr::{
    Ecc, Error, ErrorCode, Mode, Options, Result, Segment,
    json::{self, Value},
};
use std::{
    env, fs,
    io::{self, Read, Write},
    process::ExitCode,
};
const LIMIT: usize = 4 * 1024 * 1024;
const HELP: &str = "SpecQR Rust: dependency-free QR Code Model 2\n\nUsage: specqr [TEXT | --text TEXT | --hex HEX | --input FILE | --segments FILE]\n              [--format svg|png|json|terminal] [--output FILE]\n              [--ecc L|M|Q|H] [--version 1..40] [--min-version N] [--max-version N]\n              [--mask 0..7] [--mode auto|numeric|alphanumeric|byte|kanji]\n              [--no-optimize] [--boost-ecc] [--eci N | --gs1 | --fnc1-second INDICATOR]\n              [--margin N] [--scale N] [--foreground COLOR] [--background COLOR]\n              [--print-dpi DPI] [--estimate]\n\nWithout an input argument, UTF-8 text is read from standard input. --input reads\nUTF-8 text from a file; --hex preserves arbitrary bytes. --segments accepts a JSON\narray of explicit segment objects. Default output is SVG on standard output.\nPNG writes binary bytes. --estimate writes a JSON planning result without a matrix.\nUse --package-version for package version; --version sets the QR version.\n";
fn bad(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}
fn bounded_read(mut input: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    input
        .by_ref()
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| bad(e.to_string()))?;
    if bytes.len() > LIMIT {
        return Err(Error::new(
            ErrorCode::DataTooLong,
            "CLI input exceeds 4 MiB",
        ));
    }
    Ok(bytes)
}
fn text(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|_| bad("Input text is not valid UTF-8"))
}
fn number<T: std::str::FromStr>(value: &str, name: &str) -> Result<T> {
    value.parse().map_err(|_| bad(format!("Invalid {name}")))
}
fn hex(value: &str) -> Result<Vec<u8>> {
    if value.len() > LIMIT || value.len() % 2 != 0 {
        return Err(bad("Hex input requires a bounded even number of digits"));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|p| {
            let digit = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            };
            Ok(digit(p[0]).ok_or_else(|| bad("Invalid hex"))? * 16
                + digit(p[1]).ok_or_else(|| bad("Invalid hex"))?)
        })
        .collect()
}
fn string_field<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(format!("Segment {key} must be a string")))
}
fn int_field(v: &Value, key: &str) -> Result<u64> {
    v.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| bad(format!("Segment {key} must be an unsigned integer")))
}
fn u8_field(v: &Value, key: &str) -> Result<u8> {
    u8::try_from(int_field(v, key)?).map_err(|_| bad(format!("Segment {key} out of range")))
}
fn segments(value: &Value) -> Result<Vec<Segment>> {
    let input = value
        .as_array()
        .ok_or_else(|| bad("Segments must be a JSON array"))?;
    if input.len() > specqr::segment::MAX_MANUAL_SEGMENTS {
        return Err(Error::new(ErrorCode::DataTooLong, "Too many segments"));
    }
    input
        .iter()
        .map(|v| match string_field(v, "mode")? {
            "numeric" => Segment::numeric(string_field(v, "data")?),
            "alphanumeric" => Segment::alphanumeric(string_field(v, "data")?),
            "kanji" => Segment::kanji(string_field(v, "data")?),
            "byte" => {
                if let Some(h) = v.get("hex") {
                    Segment::bytes(&hex(h
                        .as_str()
                        .ok_or_else(|| bad("Segment hex must be a string"))?)?)
                } else {
                    Segment::utf8(string_field(v, "data")?)
                }
            }
            "eci" => Segment::eci(
                u32::try_from(int_field(v, "assignmentNumber")?)
                    .map_err(|_| bad("ECI assignment out of range"))?,
            ),
            "fnc1" => Ok(Segment::fnc1()),
            "fnc1-second" => Segment::fnc1_second(string_field(v, "applicationIndicator")?),
            "structured-append" => Segment::structured_append(
                u8_field(v, "index")?,
                u8_field(v, "total")?,
                u8_field(v, "parity")?,
            ),
            _ => Err(Error::new(ErrorCode::InvalidMode, "Unknown segment mode")),
        })
        .collect()
}
enum Input {
    Text(String),
    Bytes(Vec<u8>),
    Segments(Vec<Segment>),
}
fn plan_json(plan: &specqr::Plan) -> Value {
    Value::object([
        ("ok", plan.ok().into()),
        ("version", plan.version().into()),
        ("capacityVersion", plan.capacity_version().into()),
        ("errorCorrectionLevel", plan.ecc().as_str().into()),
        ("requiredBits", plan.required_bits().into()),
        ("capacityBits", plan.capacity_bits().into()),
        ("remainingBits", plan.remaining_bits().into()),
        ("diagnostics", plan.diagnostics().clone()),
    ])
}
fn run() -> Result<()> {
    let args: Vec<String> = env::args_os()
        .skip(1)
        .map(|s| {
            s.into_string()
                .map_err(|_| bad("CLI arguments must be valid UTF-8"))
        })
        .collect::<Result<_>>()?;
    if args == ["--package-version"] {
        println!("{}", specqr::VERSION);
        return Ok(());
    }
    let mut options = Options::default();
    let mut input = None;
    let mut format = "svg".to_string();
    let mut output = None;
    let mut estimate = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let argument_index = i;
        let mut next = || -> Result<&str> {
            i += 1;
            args.get(i)
                .map(String::as_str)
                .ok_or_else(|| bad(format!("Missing value after {arg}")))
        };
        let new_input = match arg.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(());
            }
            "--text" => Some(Input::Text(next()?.to_owned())),
            "--hex" => Some(Input::Bytes(hex(next()?)?)),
            "--input" => Some(Input::Text(text(bounded_read(
                fs::File::open(next()?).map_err(|e| bad(e.to_string()))?,
            )?)?)),
            "--segments" => {
                let source = text(bounded_read(
                    fs::File::open(next()?).map_err(|e| bad(e.to_string()))?,
                )?)?;
                Some(Input::Segments(segments(&json::parse(&source)?)?))
            }
            "--format" => {
                format = next()?.into();
                None
            }
            "--output" => {
                output = Some(next()?.to_string());
                None
            }
            "--ecc" => {
                options.ecc = next()?.parse::<Ecc>()?;
                None
            }
            "--version" => {
                options.version = Some(number(next()?, "version")?);
                None
            }
            "--min-version" => {
                options.min_version = number(next()?, "min-version")?;
                None
            }
            "--max-version" => {
                options.max_version = number(next()?, "max-version")?;
                None
            }
            "--mask" => {
                options.mask = Some(number(next()?, "mask")?);
                None
            }
            "--mode" => {
                let mode = next()?;
                options.mode = if mode == "auto" {
                    None
                } else {
                    Some(mode.parse::<Mode>()?)
                };
                None
            }
            "--no-optimize" => {
                options.optimize_segments = false;
                None
            }
            "--boost-ecc" => {
                options.boost_ecc = true;
                None
            }
            "--eci" => {
                options.eci = Some(number(next()?, "eci")?);
                None
            }
            "--gs1" => {
                options.gs1 = true;
                None
            }
            "--fnc1-second" => {
                options.fnc1_second = Some(next()?.into());
                None
            }
            "--margin" => {
                options.render.margin = number(next()?, "margin")?;
                None
            }
            "--scale" => {
                options.render.scale = number(next()?, "scale")?;
                None
            }
            "--foreground" => {
                options.render.foreground = next()?.into();
                None
            }
            "--background" => {
                options.render.background = next()?.into();
                None
            }
            "--print-dpi" => {
                options.print_dpi = Some(number(next()?, "print-dpi")?);
                None
            }
            "--estimate" => {
                estimate = true;
                None
            }
            "--" => {
                if argument_index + 2 != args.len() {
                    return Err(bad("Exactly one text argument must follow --"));
                }
                Some(Input::Text(next()?.into()))
            }
            _ if arg.starts_with('-') => return Err(bad(format!("Unknown option {arg}"))),
            _ => Some(Input::Text(arg.clone())),
        };
        if let Some(new_input) = new_input {
            if input.is_some() {
                return Err(bad("Specify one input source"));
            }
            input = Some(new_input);
        }
        i += 1;
    }
    options.validate()?;
    let input = match input {
        Some(v) => v,
        None => Input::Text(text(bounded_read(io::stdin().lock())?)?),
    };
    let bytes = if estimate {
        let plan = match &input {
            Input::Text(v) => specqr::estimate(v, &options)?,
            Input::Bytes(v) => specqr::estimate_bytes(v, &options)?,
            Input::Segments(v) => specqr::analyze_segments(v, &options)?,
        };
        (json::stringify(&plan_json(&plan))? + "\n").into_bytes()
    } else {
        let qr = match &input {
            Input::Text(v) => specqr::generate(v, &options)?,
            Input::Bytes(v) => specqr::generate_bytes(v, &options)?,
            Input::Segments(v) => specqr::generate_segments(v, &options)?,
        };
        match format.as_str() {
            "svg" => qr.to_svg()?.into_bytes(),
            "png" => qr.to_png()?,
            "json" => {
                let value = Value::object([
                    ("version", qr.version().into()),
                    ("size", qr.size().into()),
                    ("errorCorrectionLevel", qr.ecc().as_str().into()),
                    ("maskPattern", qr.mask().into()),
                    (
                        "matrix",
                        Value::array(qr.matrix().iter().map(|row| {
                            Value::String(row.iter().map(|v| if *v { '1' } else { '0' }).collect())
                        })),
                    ),
                    ("diagnostics", qr.diagnostics().clone()),
                ]);
                (json::stringify(&value)? + "\n").into_bytes()
            }
            "terminal" => {
                let mut value = String::new();
                for y in 0..qr.size() + 8 {
                    for x in 0..qr.size() + 8 {
                        value.push_str(
                            if x >= 4
                                && y >= 4
                                && x < qr.size() + 4
                                && y < qr.size() + 4
                                && qr.module(x - 4, y - 4)?
                            {
                                "██"
                            } else {
                                "  "
                            },
                        );
                    }
                    value.push('\n');
                }
                value.into_bytes()
            }
            _ => return Err(bad("format must be svg, png, json or terminal")),
        }
    };
    if let Some(path) = output {
        fs::write(path, bytes).map_err(|e| bad(e.to_string()))?;
    } else {
        io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|e| bad(e.to_string()))?;
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
