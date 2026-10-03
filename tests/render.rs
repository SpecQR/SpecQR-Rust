use specqr::ErrorCode;
use specqr::render::{self, RenderOptions};

fn options() -> RenderOptions {
    RenderOptions {
        margin: 1,
        scale: 2,
        foreground: "#1234".into(),
        background: "#abcdef80".into(),
    }
}
fn matrix() -> Vec<Vec<bool>> {
    vec![vec![true, false], vec![false, true]]
}
fn u32be(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}
fn crc(bytes: &[u8]) -> u32 {
    let mut result = !0_u32;
    for &byte in bytes {
        result ^= u32::from(byte);
        for _ in 0..8 {
            result = (result >> 1) ^ if result & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !result
}
fn decode_png(png: &[u8]) -> (usize, usize, Vec<u8>, usize) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let (mut at, mut width, mut height) = (8, 0, 0);
    let mut idat = Vec::new();
    let mut kinds = Vec::new();
    while at < png.len() {
        let len = u32be(&png[at..at + 4]) as usize;
        let kind = &png[at + 4..at + 8];
        let data = &png[at + 8..at + 8 + len];
        assert_eq!(
            crc(&png[at + 4..at + 8 + len]),
            u32be(&png[at + 8 + len..at + 12 + len])
        );
        if kind == b"IHDR" {
            width = u32be(&data[..4]) as usize;
            height = u32be(&data[4..8]) as usize;
            assert_eq!(&data[8..], &[8, 6, 0, 0, 0]);
        } else if kind == b"IDAT" {
            idat.extend_from_slice(data);
        } else {
            assert_eq!(kind, b"IEND");
            assert!(data.is_empty());
        }
        kinds.push(kind);
        at += len + 12;
    }
    assert_eq!(kinds, [b"IHDR".as_slice(), b"IDAT", b"IEND"]);
    assert_eq!(&idat[..2], &[0x78, 0x01]);
    let mut raw = Vec::new();
    let mut blocks = 0;
    at = 2;
    loop {
        let final_block = idat[at];
        assert!(final_block <= 1);
        let n = u16::from_le_bytes(idat[at + 1..at + 3].try_into().unwrap());
        let inverse = u16::from_le_bytes(idat[at + 3..at + 5].try_into().unwrap());
        assert_eq!(n, !inverse);
        raw.extend_from_slice(&idat[at + 5..at + 5 + usize::from(n)]);
        at += 5 + usize::from(n);
        blocks += 1;
        if final_block == 1 {
            break;
        }
    }
    assert_eq!(at + 4, idat.len());
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in &raw {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    assert_eq!(u32be(&idat[at..]), (b << 16) | a);
    assert_eq!(raw.len(), (width * 4 + 1) * height);
    let mut rgba = Vec::new();
    for row in raw.chunks_exact(width * 4 + 1) {
        assert_eq!(row[0], 0);
        rgba.extend_from_slice(&row[1..]);
    }
    (width, height, rgba, blocks)
}
fn decode_base64(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut value = 0_u32;
    let mut bits = 0;
    for ch in text.bytes() {
        let digit = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a' + 26,
            b'0'..=b'9' => ch - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => panic!("invalid base64"),
        };
        value = (value << 6) | u32::from(digit);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((value >> bits) as u8);
        }
    }
    out
}
#[test]
fn colors_and_contrast() {
    for (text, expected) in [
        (" #AbC ", [170, 187, 204, 255]),
        ("#1234", [17, 34, 51, 68]),
        ("#ABCDEF80", [171, 205, 239, 128]),
        (" black ", [0, 0, 0, 255]),
        ("WHITE", [255; 4]),
        ("Transparent", [0; 4]),
    ] {
        assert_eq!(render::parse_color(text).unwrap(), expected);
    }
    for text in [
        "",
        "red",
        "#12",
        "#12345",
        "#gggggg",
        "#１２３",
        "rgb(0,0,0)",
        "#123456789",
    ] {
        assert_eq!(
            render::parse_color(text).unwrap_err().kind(),
            ErrorCode::InvalidColor
        );
        assert_eq!(render::try_parse_color(text), None);
    }
    assert_eq!(render::contrast_ratio([0, 0, 0, 255], [255; 4]), 21.0);
    assert_eq!(render::contrast_ratio([5; 4], [5; 4]), 1.0);
}
#[test]
fn pixels_geometry_and_ownership() {
    let m = matrix();
    let p = render::to_pixels(&m, &options()).unwrap();
    assert_eq!((p.width(), p.height(), p.pixels().len()), (8, 8, 256));
    for y in 0..8 {
        for x in 0..8 {
            let dark = (2..4).contains(&x) && (2..4).contains(&y)
                || (4..6).contains(&x) && (4..6).contains(&y);
            assert_eq!(
                &p.pixels()[(y * 8 + x) * 4..(y * 8 + x) * 4 + 4],
                if dark {
                    &[17, 34, 51, 68]
                } else {
                    &[171, 205, 239, 128]
                }
            );
        }
    }
    let mut cloned = render::copy_matrix(&m).unwrap();
    cloned[0][0] = false;
    assert!(m[0][0]);
    let mut rgba = p.clone().into_pixels();
    rgba[0] = 0;
    assert_eq!(p.pixels()[0], 171);
}
#[test]
fn deterministic_png_chunks_checksums_and_pixels() {
    for scale in [1, 2, 40, 64, 129] {
        let o = RenderOptions { scale, ..options() };
        let png = render::to_png(&matrix(), &o).unwrap();
        assert_eq!(png, render::to_png(&matrix(), &o).unwrap());
        let (w, h, rgba, blocks) = decode_png(&png);
        let pixels = render::to_pixels(&matrix(), &o).unwrap();
        assert_eq!((w, h), (pixels.width(), pixels.height()));
        assert_eq!(rgba, pixels.pixels());
        if scale >= 40 {
            assert!(blocks > 1);
        }
        let url = render::to_png_data_url(&matrix(), &o).unwrap();
        assert_eq!(
            decode_base64(url.strip_prefix("data:image/png;base64,").unwrap()),
            png
        );
    }
}
#[test]
fn svg_xml_escaping_and_data_url() {
    let svg = render::to_svg(&matrix(), &options()).unwrap();
    assert_eq!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\" viewBox=\"0 0 8 8\" role=\"img\"><rect width=\"100%\" height=\"100%\" fill=\"#abcdef80\"/><path fill=\"#1234\" d=\"M2,2h2v2h-2zM4,4h2v2h-2z\"/></svg>"
    );
    let o = RenderOptions {
        foreground: "red\"/><script>&'é😀".into(),
        ..options()
    };
    let svg = render::to_svg(&matrix(), &o).unwrap();
    assert!(svg.contains("red&quot;/&gt;&lt;script&gt;&amp;&#x27;é😀"));
    assert!(!svg.contains("<script>"));
    let url = render::to_svg_data_url(&matrix(), &o).unwrap();
    assert!(url.starts_with("data:image/svg+xml;charset=utf-8,%3Csvg"));
    assert!(url.contains("%C3%A9%F0%9F%98%80"));
    for foreground in ["\0", "\u{b}", "\u{fffe}", "\u{ffff}"] {
        assert_eq!(
            render::to_svg(
                &matrix(),
                &RenderOptions {
                    foreground: foreground.into(),
                    ..options()
                }
            )
            .unwrap_err()
            .kind(),
            ErrorCode::InvalidColor
        );
    }
}
#[test]
fn geometry_and_output_caps() {
    for matrix in [
        vec![],
        vec![vec![true], vec![false]],
        vec![vec![true; 178]; 178],
    ] {
        assert_eq!(
            render::to_pixels(&matrix, &options()).unwrap_err().kind(),
            ErrorCode::InvalidInput
        );
    }
    assert_eq!(
        render::to_pixels(
            &matrix(),
            &RenderOptions {
                scale: 0,
                ..options()
            }
        )
        .unwrap_err()
        .kind(),
        ErrorCode::InvalidInput
    );
    let excessive = RenderOptions {
        margin: u32::MAX,
        scale: u32::MAX,
        ..options()
    };
    assert!(render::to_svg(&matrix(), &excessive).is_err());
    assert!(render::to_png(&matrix(), &excessive).is_err());
    let too_big = RenderOptions {
        foreground: "&".repeat(render::SVG_CHARACTER_BUDGET / 6),
        ..options()
    };
    assert!(render::to_svg(&matrix(), &too_big).is_err());
    let max = RenderOptions {
        margin: 0,
        scale: 2048,
        ..options()
    };
    let p = render::to_pixels(&[vec![true]], &max).unwrap();
    assert_eq!(p.pixels().len(), render::RASTER_PIXEL_BUDGET * 4);
    assert!(render::to_pixels(&[vec![true]], &RenderOptions { scale: 2049, ..max }).is_err());
}
