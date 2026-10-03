//! Bounded, deterministic SVG, RGBA, and PNG rendering without dependencies.
//!
//! PNG uses lossless stored DEFLATE blocks. The output intentionally favors
//! portability and byte-for-byte reproducibility over compression ratio.

use std::fmt::Write;

use crate::{Error, ErrorCode, Result};

/// Maximum number of pixels in an RGBA rendering (4 Mi-pixels).
pub const RASTER_PIXEL_BUDGET: usize = 4 * 1024 * 1024;
/// Conservative maximum UTF-8 byte length for SVG output.
pub const SVG_CHARACTER_BUDGET: usize = 8 * 1024 * 1024;
/// Maximum byte length of a rendered data URL.
pub const DATA_URL_CHARACTER_BUDGET: usize = 32 * 1024 * 1024;
/// Largest interoperable exact integer for SVG coordinates.
pub const MAX_GEOMETRY_INTEGER: u64 = (1 << 53) - 1;

/// Geometry and colors for all renderers. Raster colors support hexadecimal
/// RGB/RGBA, `black`, `white`, and `transparent`; SVG also accepts CSS colors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    /// Quiet-zone width in modules.
    pub margin: u32,
    /// Pixels (or SVG coordinate units) per module. Must be positive.
    pub scale: u32,
    /// Dark-module color.
    pub foreground: String,
    /// Light-module and quiet-zone color.
    pub background: String,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            margin: 4,
            scale: 8,
            foreground: "#000000".into(),
            background: "#ffffff".into(),
        }
    }
}

/// An immutable, row-major, unpremultiplied RGBA image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pixels {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Pixels {
    /// Image width in pixels.
    pub fn width(&self) -> usize {
        self.width
    }
    /// Image height in pixels.
    pub fn height(&self) -> usize {
        self.height
    }
    /// Row-major RGBA bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// Consume the image and return its RGBA bytes.
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}

fn invalid(message: &str) -> Error {
    Error::new(ErrorCode::InvalidInput, message)
}
fn limited(message: &str) -> Error {
    Error::new(ErrorCode::ResourceLimit, message)
}

fn validate_matrix(matrix: &[Vec<bool>]) -> Result<()> {
    if !(1..=177).contains(&matrix.len()) {
        return Err(invalid("Matrix dimension must be 1..177"));
    }
    if matrix.iter().any(|row| row.len() != matrix.len()) {
        return Err(invalid("Matrix must be square"));
    }
    Ok(())
}

/// Validate and clone a square matrix with dimension 1..177.
pub fn copy_matrix(matrix: &[Vec<bool>]) -> Result<Vec<Vec<bool>>> {
    validate_matrix(matrix)?;
    Ok(matrix.to_vec())
}

fn dimension(matrix: &[Vec<bool>], options: &RenderOptions, raster: bool) -> Result<u64> {
    validate_matrix(matrix)?;
    if options.scale == 0 {
        return Err(invalid("Render scale must be positive"));
    }
    let modules = matrix.len() as u64 + 2 * u64::from(options.margin);
    let dimension = modules
        .checked_mul(u64::from(options.scale))
        .filter(|&value| value <= MAX_GEOMETRY_INTEGER)
        .ok_or_else(|| limited("Render geometry exceeds the exact-integer resource budget"))?;
    if raster && (dimension > 2048 || dimension * dimension > RASTER_PIXEL_BUDGET as u64) {
        return Err(limited(
            "Render geometry exceeds the 4 Mi-pixel resource budget",
        ));
    }
    Ok(dimension)
}

/// Parse supported raster colors, returning `None` for other CSS colors.
pub fn try_parse_color(value: &str) -> Option<[u8; 4]> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("black") {
        return Some([0, 0, 0, 255]);
    }
    if value.eq_ignore_ascii_case("white") {
        return Some([255, 255, 255, 255]);
    }
    if value.eq_ignore_ascii_case("transparent") {
        return Some([0, 0, 0, 0]);
    }
    let hex = value.strip_prefix('#')?.as_bytes();
    if !matches!(hex.len(), 3 | 4 | 6 | 8) {
        return None;
    }
    fn digit(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let mut result = [0, 0, 0, 255];
    if hex.len() <= 4 {
        for (out, &byte) in result.iter_mut().zip(hex) {
            *out = digit(byte)? * 17;
        }
    } else {
        for (out, pair) in result.iter_mut().zip(hex.chunks_exact(2)) {
            *out = digit(pair[0])? * 16 + digit(pair[1])?;
        }
    }
    Some(result)
}

/// Parse hexadecimal RGB/RGBA or `black`, `white`, and `transparent`.
pub fn parse_color(value: &str) -> Result<[u8; 4]> {
    try_parse_color(value).ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidColor,
            "Raster color must be hex, black, white or transparent",
        )
    })
}

/// WCAG-style RGB contrast ratio. Alpha is intentionally not composited.
pub fn contrast_ratio(foreground: [u8; 4], background: [u8; 4]) -> f64 {
    fn luminance(color: [u8; 4]) -> f64 {
        [0.2126, 0.7152, 0.0722]
            .into_iter()
            .zip(color)
            .map(|(weight, component)| {
                let value = f64::from(component) / 255.0;
                weight
                    * if value <= 0.03928 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    }
            })
            .sum()
    }
    let a = luminance(foreground);
    let b = luminance(background);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Render unpremultiplied RGBA pixels, including the requested quiet zone.
pub fn to_pixels(matrix: &[Vec<bool>], options: &RenderOptions) -> Result<Pixels> {
    let n = dimension(matrix, options, true)? as usize;
    let foreground = parse_color(&options.foreground)?;
    let background = parse_color(&options.background)?;
    let scale = options.scale as usize;
    let margin = options.margin as usize;
    let mut pixels = vec![0; n * n * 4];
    for y in 0..n {
        let my = (y / scale).checked_sub(margin);
        for x in 0..n {
            let mx = (x / scale).checked_sub(margin);
            let dark = match (my, mx) {
                (Some(yy), Some(xx)) if yy < matrix.len() && xx < matrix.len() => matrix[yy][xx],
                _ => false,
            };
            let color = if dark { foreground } else { background };
            let start = (y * n + x) * 4;
            pixels[start..start + 4].copy_from_slice(&color);
        }
    }
    Ok(Pixels {
        width: n,
        height: n,
        pixels,
    })
}

fn escape_xml(value: &str) -> Result<String> {
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#x27;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '\t'
            | '\n'
            | '\r'
            | '\u{20}'..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}' => result.push(ch),
            _ => {
                return Err(Error::new(
                    ErrorCode::InvalidColor,
                    "SVG color contains an invalid XML character",
                ));
            }
        }
    }
    Ok(result)
}

/// Render a standalone SVG with XML-escaped colors and exact integer geometry.
pub fn to_svg(matrix: &[Vec<bool>], options: &RenderOptions) -> Result<String> {
    let n = dimension(matrix, options, false)?;
    let color_bytes = options
        .foreground
        .len()
        .checked_add(options.background.len())
        .filter(|&value| value <= SVG_CHARACTER_BUDGET / 6)
        .ok_or_else(|| limited("SVG colors exceed the output resource budget"))?;
    let digits = n.to_string().len();
    let scale_digits = options.scale.to_string().len();
    let dark = matrix.iter().flatten().filter(|&&cell| cell).count();
    let budget = 512 + 6 * color_bytes + 4 * digits + dark * (7 + 2 * digits + 3 * scale_digits);
    if budget > SVG_CHARACTER_BUDGET {
        return Err(limited("SVG exceeds the output resource budget"));
    }
    let foreground = escape_xml(&options.foreground)?;
    let background = escape_xml(&options.background)?;
    let mut result = String::with_capacity(budget);
    // Writing to a String cannot fail.
    write!(result, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{n}\" height=\"{n}\" viewBox=\"0 0 {n} {n}\" role=\"img\"><rect width=\"100%\" height=\"100%\" fill=\"{background}\"/><path fill=\"{foreground}\" d=\"").unwrap();
    for (y, row) in matrix.iter().enumerate() {
        for (x, &dark) in row.iter().enumerate() {
            if dark {
                let xx = (x as u64 + u64::from(options.margin)) * u64::from(options.scale);
                let yy = (y as u64 + u64::from(options.margin)) * u64::from(options.scale);
                let scale = options.scale;
                write!(result, "M{xx},{yy}h{scale}v{scale}h-{scale}z").unwrap();
            }
        }
    }
    result.push_str("\"/></svg>");
    debug_assert!(result.len() <= budget);
    Ok(result)
}

const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut value = i as u32;
        let mut bit = 0;
        while bit < 8 {
            value = (value >> 1) ^ if value & 1 != 0 { 0xedb8_8320 } else { 0 };
            bit += 1;
        }
        table[i] = value;
        i += 1;
    }
    table
}
const CRC_TABLE: [u32; 256] = crc_table();

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = u32::MAX;
    for &byte in kind.iter().chain(data) {
        crc = CRC_TABLE[((crc ^ u32::from(byte)) & 255) as usize] ^ (crc >> 8);
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1_u32;
    let mut b = 0_u32;
    // NMAX bounds both accumulators below u32::MAX before reduction.
    for block in data.chunks(5552) {
        for &byte in block {
            a += u32::from(byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// Encode deterministic 8-bit RGBA PNG with stored DEFLATE blocks.
pub fn to_png(matrix: &[Vec<bool>], options: &RenderOptions) -> Result<Vec<u8>> {
    let pixels = to_pixels(matrix, options)?;
    let stride = pixels.width * 4;
    let mut raw = Vec::with_capacity((stride + 1) * pixels.height);
    for row in pixels.pixels.chunks_exact(stride) {
        raw.push(0); // PNG filter type None.
        raw.extend_from_slice(row);
    }
    let mut stream = Vec::with_capacity(raw.len() + raw.len().div_ceil(65535) * 5 + 6);
    stream.extend_from_slice(&[0x78, 0x01]);
    let blocks = raw.len().div_ceil(65535);
    for (index, block) in raw.chunks(65535).enumerate() {
        stream.push(u8::from(index + 1 == blocks));
        let length = block.len() as u16;
        stream.extend_from_slice(&length.to_le_bytes());
        stream.extend_from_slice(&(!length).to_le_bytes());
        stream.extend_from_slice(block);
    }
    stream.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&(pixels.width as u32).to_be_bytes());
    header.extend_from_slice(&(pixels.height as u32).to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut result = Vec::with_capacity(stream.len() + 57);
    result.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    png_chunk(&mut result, b"IHDR", &header);
    png_chunk(&mut result, b"IDAT", &stream);
    png_chunk(&mut result, b"IEND", &[]);
    Ok(result)
}

fn base64(data: &[u8], out: &mut String) {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for block in data.chunks(3) {
        let value = (u32::from(block[0]) << 16)
            | (u32::from(*block.get(1).unwrap_or(&0)) << 8)
            | u32::from(*block.get(2).unwrap_or(&0));
        out.push(ALPHABET[((value >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((value >> 12) & 63) as usize] as char);
        out.push(if block.len() > 1 {
            ALPHABET[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if block.len() > 2 {
            ALPHABET[(value & 63) as usize] as char
        } else {
            '='
        });
    }
}

/// Render a base64-encoded PNG data URL.
pub fn to_png_data_url(matrix: &[Vec<bool>], options: &RenderOptions) -> Result<String> {
    let png = to_png(matrix, options)?;
    let length = 22 + png.len().div_ceil(3) * 4;
    if length > DATA_URL_CHARACTER_BUDGET {
        return Err(limited("PNG data URL exceeds the output resource budget"));
    }
    let mut result = String::with_capacity(length);
    result.push_str("data:image/png;base64,");
    base64(&png, &mut result);
    Ok(result)
}

/// Render a percent-encoded UTF-8 SVG data URL.
pub fn to_svg_data_url(matrix: &[Vec<bool>], options: &RenderOptions) -> Result<String> {
    let svg = to_svg(matrix, options)?;
    let length = 31 + svg.len() * 3;
    if length > DATA_URL_CHARACTER_BUDGET {
        return Err(limited("SVG data URL exceeds the output resource budget"));
    }
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut result = String::with_capacity(length);
    result.push_str("data:image/svg+xml;charset=utf-8,");
    for byte in svg.bytes() {
        if byte.is_ascii_alphanumeric() || b"~()*!.'-_".contains(&byte) {
            result.push(byte as char);
        } else {
            result.push('%');
            result.push(HEX[(byte >> 4) as usize] as char);
            result.push(HEX[(byte & 15) as usize] as char);
        }
    }
    Ok(result)
}
