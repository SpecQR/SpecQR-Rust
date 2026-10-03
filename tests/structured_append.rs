use specqr::structured_append::{self as sa, DiagnosticOptions, Part, PartData};
use specqr::{Ecc, ErrorCode, Mode, Options, QrCode, Segment, json::Value};

fn v1(mode: Option<Mode>) -> Options {
    Options {
        version: Some(1),
        ecc: Ecc::L,
        mask: Some(0),
        mode,
        ..Options::default()
    }
}
fn run(text: &str, options: &Options) -> sa::SaResult {
    sa::generate(text, options, 16, &DiagnosticOptions::default()).unwrap()
}
fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap()
}
fn integer(value: &Value, key: &str) -> u64 {
    field(value, key).as_u64().unwrap()
}
fn symbol_info(result: &sa::SaResult) -> &[Value] {
    field(result.diagnostics(), "symbols").as_array().unwrap()
}
fn hash(symbol: &QrCode) -> u64 {
    // FNV-1a over the same newline-delimited binary matrix as the cross-edition
    // SHA-256 fixtures. Constants were independently produced with SpecQR-Python.
    let mut value = 0xcbf29ce484222325_u64;
    for (y, row) in symbol.matrix().iter().enumerate() {
        if y > 0 {
            value = (value ^ u64::from(b'\n')).wrapping_mul(0x100000001b3);
        }
        for &module in row {
            value =
                (value ^ u64::from(if module { b'1' } else { b'0' })).wrapping_mul(0x100000001b3);
        }
    }
    value
}
fn hashes(result: &sa::SaResult) -> Vec<u64> {
    result.symbols().iter().map(hash).collect()
}
fn xor(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0, |a, &b| a ^ b)
}

#[test]
fn original_utf8_and_binary_parity() {
    assert_eq!(sa::calculate_parity("").unwrap(), 0);
    assert_eq!(sa::calculate_bytes_parity(&[]).unwrap(), 0);
    for text in ["ABC", "漢字", "é😀", "\0\u{ffff}", "01234567890123456789"] {
        assert_eq!(sa::calculate_parity(text).unwrap(), xor(text.as_bytes()));
    }
    let segments = [
        Segment::numeric("0123").unwrap(),
        Segment::kanji("漢字").unwrap(),
        Segment::bytes(&[0, 1, 255]).unwrap(),
    ];
    assert_eq!(
        sa::calculate_segments_parity(&segments).unwrap(),
        xor("0123漢字".as_bytes()) ^ 254
    );
    assert_eq!(
        sa::calculate_segments_parity(&[]).unwrap_err().kind(),
        ErrorCode::InvalidInput
    );
}

#[test]
fn alphanumeric_golden_and_header_bits() {
    let result = run(&"A".repeat(31), &v1(Some(Mode::Alphanumeric)));
    assert_eq!(
        (
            result.total(),
            result.parity(),
            result.input_length(),
            result.byte_length()
        ),
        (2, 65, 31, 31)
    );
    assert_eq!(hashes(&result), [0xebee3b5a51220593, 0x75322ea00f0b76a9]);
    assert_eq!(integer(&symbol_info(&result)[0], "inputLength"), 21);
    assert_eq!(integer(&symbol_info(&result)[1], "inputStart"), 21);
    assert_eq!(integer(&symbol_info(&result)[0], "dataBitLength"), 149);
    assert_eq!(
        &result.symbols()[0].data_codewords()[..3],
        &[0x30, 0x14, 0x12]
    );
    assert_eq!(
        &result.symbols()[1].data_codewords()[..3],
        &[0x31, 0x14, 0x12]
    );
}

#[test]
fn manual_golden_boundaries_and_diagnostics() {
    let segments = [
        Segment::alphanumeric("ABCDEFGHIJKLMNOPQRSTU").unwrap(),
        Segment::numeric("12345678901234567890").unwrap(),
        Segment::bytes(&[0, 1, 2, 255]).unwrap(),
    ];
    let result =
        sa::generate_segments(&segments, &v1(None), 16, &DiagnosticOptions::full()).unwrap();
    assert_eq!(
        (
            result.total(),
            result.parity(),
            result.input_length(),
            result.byte_length()
        ),
        (2, 189, 3, 45)
    );
    assert_eq!(hashes(&result), [0x481497fcfb836ac9, 0xf5e98665b1e72b37]);
    assert_eq!(integer(&symbol_info(&result)[1], "sourceSegmentStart"), 1);
    assert_eq!(integer(&symbol_info(&result)[1], "sourceSegmentEnd"), 3);
    assert_eq!(integer(&symbol_info(&result)[1], "splitUnitLength"), 5);
    let detail = field(result.diagnostics(), "splitUnits")
        .as_array()
        .unwrap();
    assert_eq!(detail.len(), 6);
    assert_eq!(integer(&detail[0], "unitLength"), 21);
    assert_eq!(integer(&detail[1], "unitLength"), 20);
    assert_eq!(integer(&detail[2], "byteStart"), 41);
    assert_eq!(result.symbols()[0].segments()[1], segments[0]);
    assert_eq!(result.symbols()[1].segments()[1], segments[1]);
    let summary =
        sa::generate_segments(&segments, &v1(None), 16, &DiagnosticOptions::default()).unwrap();
    assert_eq!(hashes(&summary), hashes(&result));
    assert!(summary.diagnostics().get("splitUnits").is_none());
}

#[test]
fn binary_manual_golden() {
    let mut bytes: Vec<u8> = (0..30).collect();
    bytes.push(255);
    let options = Options {
        mask: Some(1),
        ..v1(None)
    };
    let result = sa::generate_segments(
        &[Segment::bytes(&bytes).unwrap()],
        &options,
        16,
        &DiagnosticOptions::default(),
    )
    .unwrap();
    assert_eq!((result.total(), result.parity()), (3, 254));
    assert_eq!(
        hashes(&result),
        [0xcedf0d549f15c57b, 0x86979a60834a3273, 0x85395a3c7d7e382f]
    );
    let merged: Vec<u8> = result
        .symbols()
        .iter()
        .flat_map(|s| s.segments()[1..].iter().flat_map(Segment::logical_bytes))
        .copied()
        .collect();
    assert_eq!(merged, bytes);
}

#[test]
fn kanji_canonical_parity_golden() {
    let segments = [
        Segment::alphanumeric("ABCDEFGHIJKLMNOPQRSTU").unwrap(),
        Segment::kanji("漢字").unwrap(),
        Segment::numeric("12345678901234567890").unwrap(),
    ];
    let result = sa::generate_segments(
        &segments,
        &Options {
            mask: Some(2),
            ..v1(None)
        },
        16,
        &DiagnosticOptions::default(),
    )
    .unwrap();
    assert_eq!((result.parity(), result.byte_length()), (102, 47));
    assert_eq!(hashes(&result), [0x4d3f21e69495f38d, 0xf2f2cbb1cdbb58a9]);
}

#[test]
fn unicode_scalar_splits_and_canonical_offsets() {
    for text in [
        "😀".repeat(17),
        "é漢😀abc".repeat(12),
        "1234567890".repeat(17),
        "漢字".repeat(20),
    ] {
        for optimize in [false, true] {
            let result = run(
                &text,
                &Options {
                    optimize_segments: optimize,
                    ..v1(None)
                },
            );
            let (mut merged, mut scalar_offset, mut byte_offset) = (String::new(), 0, 0);
            for (i, symbol) in result.symbols().iter().enumerate() {
                let header = &symbol.segments()[0];
                assert_eq!(
                    (header.index(), header.total(), header.parity()),
                    (
                        Some(i as u8 + 1),
                        Some(result.total()),
                        Some(result.parity())
                    )
                );
                assert_eq!(
                    integer(&symbol_info(&result)[i], "inputStart"),
                    scalar_offset
                );
                assert_eq!(integer(&symbol_info(&result)[i], "byteStart"), byte_offset);
                for segment in &symbol.segments()[1..] {
                    let text = segment.text().unwrap();
                    merged.push_str(text);
                    scalar_offset += text.chars().count() as u64;
                    byte_offset += text.len() as u64;
                }
                assert!(specqr::segment::bit_length(symbol.segments(), 1).unwrap() <= 152);
            }
            assert_eq!(merged, text);
            assert_eq!(result.input_length(), text.chars().count());
            assert_eq!(result.byte_length(), text.len());
        }
    }
}

#[test]
fn manual_byte_text_only_splits_scalars() {
    let text = "😀éA漢".repeat(10);
    let result = sa::generate_segments(
        &[Segment::utf8(&text).unwrap()],
        &v1(None),
        16,
        &DiagnosticOptions::full(),
    )
    .unwrap();
    let mut merged = String::new();
    for symbol in result.symbols() {
        for segment in &symbol.segments()[1..] {
            assert_eq!(segment.mode(), Mode::Byte);
            merged.push_str(segment.text().unwrap());
        }
    }
    assert_eq!(merged, text);
    let detail = field(result.diagnostics(), "splitUnits")
        .as_array()
        .unwrap();
    assert_eq!(detail.len(), 40);
    assert_eq!(integer(&detail[0], "byteLength"), 4);
    assert_eq!(integer(&detail[1], "byteStart"), 4);
    assert_eq!(integer(&detail[1], "byteLength"), 2);
}

#[test]
fn complete_two_through_sixteen_symbol_sets() {
    for total in 2..=16 {
        let bytes = vec![0xa7; 15 * (total - 1) + 1];
        let result = sa::generate_bytes(
            &bytes,
            &v1(None),
            total as u8,
            &DiagnosticOptions::default(),
        )
        .unwrap();
        assert_eq!(result.total(), total as u8);
        let parts: Vec<_> = result
            .symbols()
            .iter()
            .enumerate()
            .map(|(i, symbol)| {
                Part::new(
                    i as u8 + 1,
                    total as u8,
                    result.parity(),
                    PartData::Bytes(symbol.segments()[1].logical_bytes().to_vec()),
                )
                .unwrap()
            })
            .collect();
        assert_eq!(sa::merge(&parts).unwrap().bytes().unwrap(), bytes);
        assert_eq!(
            field(result.diagnostics(), "warnings")
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    assert!(sa::generate_bytes(&[0; 241], &v1(None), 16, &DiagnosticOptions::default()).is_err());
}

#[test]
fn auto_smallest_version_and_fixed_version() {
    let manual = sa::generate_segments(
        &[Segment::utf8(&"a".repeat(100)).unwrap()],
        &Options {
            ecc: Ecc::L,
            max_version: 5,
            mask: Some(0),
            ..Options::default()
        },
        2,
        &DiagnosticOptions::default(),
    )
    .unwrap();
    let version = manual.symbols()[0].version();
    assert_eq!(
        field(manual.diagnostics(), "versionSelectionReason")
            .as_str()
            .unwrap(),
        format!(
            "Version {version} is the smallest version in 1..5 that can split the manual segments into 2 Structured Append symbols at error correction L."
        )
    );
    let result = sa::generate(
        &"A".repeat(70),
        &Options {
            ecc: Ecc::L,
            max_version: 5,
            mask: Some(0),
            ..Options::default()
        },
        2,
        &DiagnosticOptions::default(),
    )
    .unwrap();
    assert_eq!(result.total(), 2);
    assert!(result.symbols().iter().all(|s| s.version() == 2));
    assert_eq!(
        field(result.diagnostics(), "versionSelection").as_str(),
        Some("auto-minimum")
    );
    assert_eq!(
        field(
            run(&"A".repeat(31), &v1(None)).diagnostics(),
            "versionSelection"
        )
        .as_str(),
        Some("fixed")
    );
    for text in ["", "A"] {
        assert_eq!(
            sa::generate(text, &v1(None), 16, &DiagnosticOptions::default())
                .unwrap_err()
                .kind(),
            ErrorCode::InvalidInput
        );
    }
}

#[test]
fn controls_and_unsupported_options_are_rejected() {
    let text = "A".repeat(31);
    let detail = DiagnosticOptions::default();
    for max in [0, 1, 17, 255] {
        assert_eq!(
            sa::generate(&text, &v1(None), max, &detail)
                .unwrap_err()
                .kind(),
            ErrorCode::InvalidMode
        );
    }
    let mut variants = vec![
        Options {
            eci: Some(0),
            ..v1(None)
        },
        Options {
            fnc1_second: Some("00".into()),
            ..v1(None)
        },
        Options {
            boost_ecc: true,
            ..v1(None)
        },
    ];
    variants.push(Options {
        structured_append: Some(Segment::structured_append(1, 2, 0).unwrap()),
        ..v1(None)
    });
    for options in variants {
        assert_eq!(
            sa::generate(&text, &options, 16, &detail)
                .unwrap_err()
                .kind(),
            ErrorCode::InvalidMode
        );
    }
    assert_eq!(
        sa::generate(
            &text,
            &Options {
                gs1: true,
                ..v1(None)
            },
            16,
            &detail
        )
        .unwrap_err()
        .kind(),
        ErrorCode::InvalidGs1
    );
    assert_eq!(
        sa::generate_bytes(&[0; 40], &v1(Some(Mode::Numeric)), 16, &detail)
            .unwrap_err()
            .kind(),
        ErrorCode::InvalidMode
    );
    for options in [
        v1(Some(Mode::Byte)),
        Options {
            optimize_segments: false,
            ..v1(None)
        },
    ] {
        assert_eq!(
            sa::generate_segments(&[Segment::utf8(&text).unwrap()], &options, 16, &detail)
                .unwrap_err()
                .kind(),
            ErrorCode::InvalidMode
        );
    }
    for control in [
        Segment::eci(26).unwrap(),
        Segment::fnc1_second("00").unwrap(),
        Segment::structured_append(1, 2, 0).unwrap(),
    ] {
        assert_eq!(
            sa::calculate_segments_parity(&[control])
                .unwrap_err()
                .kind(),
            ErrorCode::InvalidMode
        );
    }
    assert_eq!(
        sa::calculate_segments_parity(&[Segment::fnc1()])
            .unwrap_err()
            .kind(),
        ErrorCode::InvalidGs1
    );
    assert_eq!(
        sa::generate_segments(&[Segment::utf8("").unwrap()], &v1(None), 16, &detail)
            .unwrap_err()
            .kind(),
        ErrorCode::InvalidInput
    );
}

#[test]
fn indivisible_modes_and_capacity_failure() {
    let detail = DiagnosticOptions::default();
    for segment in [
        Segment::numeric(&"1".repeat(100)).unwrap(),
        Segment::alphanumeric(&"A".repeat(50)).unwrap(),
        Segment::kanji(&"漢".repeat(15)).unwrap(),
    ] {
        assert_eq!(
            sa::generate_segments(&[segment], &v1(None), 16, &detail)
                .unwrap_err()
                .kind(),
            ErrorCode::DataTooLong
        );
    }
    assert_eq!(
        sa::generate(&"A".repeat(300), &v1(None), 2, &detail)
            .unwrap_err()
            .kind(),
        ErrorCode::DataTooLong
    );
    assert_eq!(
        sa::generate("lowercase", &v1(Some(Mode::Alphanumeric)), 16, &detail)
            .unwrap_err()
            .kind(),
        ErrorCode::InvalidMode
    );
}

#[test]
fn sparse_index_exact_checkpoint_boundaries() {
    for length in [63, 64, 65, 127, 128, 129] {
        let text = "é".repeat(length);
        let result = run(
            &text,
            &Options {
                version: Some(2),
                ..v1(Some(Mode::Byte))
            },
        );
        let merged: String = result
            .symbols()
            .iter()
            .flat_map(|symbol| {
                symbol.segments()[1..]
                    .iter()
                    .map(|segment| segment.text().unwrap())
            })
            .collect();
        assert_eq!(merged, text);
        assert_eq!(
            symbol_info(&result)
                .iter()
                .map(|d| integer(d, "inputLength"))
                .sum::<u64>(),
            length as u64
        );
    }
}

#[test]
fn out_of_order_merge_metadata_and_binary_ownership() {
    let text = "first😀second";
    let parity = sa::calculate_parity(text).unwrap();
    let result = sa::merge(&[
        Part::new(2, 2, parity, "second".into()).unwrap(),
        Part::new(1, 2, parity, "first😀".into()).unwrap(),
    ])
    .unwrap();
    assert_eq!(result.text(), Some(text));
    assert_eq!(result.bytes(), None);
    assert_eq!(result.parts()[0].index(), 1);
    assert_eq!(result.parts()[1].data_type(), "string");
    assert_eq!(
        integer(result.diagnostics(), "byteLength"),
        text.len() as u64
    );
    let mut original = vec![0, 1, 3];
    let part = Part::new(1, 2, 255, original.as_slice().into()).unwrap();
    original[0] = 99;
    let result = sa::merge(&[Part::new(2, 2, 255, vec![2, 255].into()).unwrap(), part]).unwrap();
    assert_eq!(result.bytes(), Some([0, 1, 3, 2, 255].as_slice()));
    assert_eq!(result.text(), None);
    assert_eq!(
        field(field(result.diagnostics(), "parityCheck"), "matches").as_bool(),
        Some(true)
    );
}

#[test]
fn malformed_and_incomplete_merge_sets() {
    let part = |i, t, p, text: &str| Part::new(i, t, p, text.into()).unwrap();
    for (i, t) in [(0, 2), (1, 1), (3, 2), (1, 17), (255, 255)] {
        assert!(Part::new(i, t, 0, "A".into()).is_err());
    }
    for parts in [
        vec![],
        vec![part(1, 2, 0, "A")],
        vec![part(1, 2, 0, "A"), part(1, 2, 0, "B")],
        vec![part(1, 2, 0, "A"), part(2, 3, 0, "A")],
        vec![part(1, 2, 0, "A"), part(2, 2, 1, "A")],
        vec![part(1, 2, 1, "A"), part(2, 2, 1, "A")],
    ] {
        assert_eq!(
            sa::merge(&parts).unwrap_err().kind(),
            ErrorCode::InvalidInput
        );
    }
    assert_eq!(
        sa::merge(&[
            part(1, 2, 0, "A"),
            Part::new(2, 2, 0, vec![65].into()).unwrap()
        ])
        .unwrap_err()
        .kind(),
        ErrorCode::InvalidInput
    );
    // Empty decoder parts are valid metadata when the complete set parity agrees.
    assert_eq!(
        sa::merge(&[part(2, 2, 0, ""), part(1, 2, 0, "")])
            .unwrap()
            .text(),
        Some("")
    );
}

#[test]
fn preallocation_resource_caps() {
    let text = "1".repeat(1_000_001);
    assert_eq!(
        sa::calculate_parity(&text).unwrap_err().kind(),
        ErrorCode::DataTooLong
    );
    assert_eq!(
        sa::calculate_bytes_parity(text.as_bytes())
            .unwrap_err()
            .kind(),
        ErrorCode::DataTooLong
    );
    assert_eq!(
        sa::generate(&text, &v1(None), 16, &DiagnosticOptions::default())
            .unwrap_err()
            .kind(),
        ErrorCode::DataTooLong
    );
    assert_eq!(
        Part::new(1, 2, 0, text.into()).unwrap_err().kind(),
        ErrorCode::DataTooLong
    );
    assert_eq!(
        sa::calculate_segments_parity(&vec![Segment::utf8("A").unwrap(); 16_385])
            .unwrap_err()
            .kind(),
        ErrorCode::DataTooLong
    );
    let parts = [
        Part::new(1, 2, 0, "A".repeat(500_001).into()).unwrap(),
        Part::new(2, 2, 0, "A".repeat(500_001).into()).unwrap(),
    ];
    assert_eq!(
        sa::merge(&parts).unwrap_err().kind(),
        ErrorCode::DataTooLong
    );
}
