use specqr::{self, Ecc, ErrorCode, Mode, Options, Segment};
use std::str::FromStr;

// Preserve Rust's established safe byte fallback / forced-alpha refusal rather
// than making segment selection match the repaired TypeScript implementation.
#[test]
fn fnc1_literal_percent_corpus() {
    let payloads = [
        "10ABC%DEF",
        "10ABC%%DEF",
        "10%ABC",
        "10ABC%",
        "10ABC%\u{1d}21DEF%",
        "10AAAAAAAAAAAAAAAAAAA%",
        "ABC%DEF%%",
        "漢字A%AAAAAA",
    ];
    for second in [None, Some("37"), Some("A")] {
        for text in payloads {
            if second.is_none() && !text.starts_with("10") {
                continue;
            }
            for optimize in [false, true] {
                for mode in [None, Some(Mode::Byte), Some(Mode::Alphanumeric)] {
                    let o = Options {
                        gs1: second.is_none(),
                        fnc1_second: second.map(str::to_owned),
                        mode,
                        optimize_segments: optimize,
                        ..Default::default()
                    };
                    let q = specqr::generate(text, &o);
                    let p = specqr::estimate(text, &o);
                    if mode == Some(Mode::Alphanumeric) {
                        assert_eq!(q.unwrap_err().kind(), ErrorCode::InvalidMode);
                        assert_eq!(p.unwrap_err().kind(), ErrorCode::InvalidMode);
                    } else {
                        let q = q.unwrap();
                        let p = p.unwrap();
                        assert!(p.ok());
                        assert_eq!(q.segments()[1].mode(), Mode::Byte);
                        assert_eq!(q.segments()[1].logical_bytes(), text.as_bytes());
                        assert_eq!(
                            q.diagnostics().get("dataBitLength"),
                            p.diagnostics().get("dataBitLength")
                        );
                    }
                }
            }
        }
    }
    for text in ["ABC%DEF", "ABC%%DEF"] {
        let q = specqr::generate_segments(
            &[Segment::fnc1(), Segment::alphanumeric(text).unwrap()],
            &Options::default(),
        )
        .unwrap();
        assert_eq!(q.segments()[1].text(), Some(text));
    }
    let o = Options {
        gs1: true,
        version: Some(1),
        ecc: Ecc::L,
        ..Default::default()
    };
    let text = "10AAAAAAAAAAAAAAAAAAA%";
    assert!(!specqr::estimate(text, &o).unwrap().ok());
    assert_eq!(
        specqr::generate(text, &o).unwrap_err().kind(),
        ErrorCode::DataTooLong
    );
    assert!(specqr::generate(&"%".repeat(1_000_001), &o).is_err());
}

#[test]
fn digital_link_dot_data_stays_intact() {
    use specqr::gs1::*;
    let root = "https://example.com/01/04912345678904";
    for (raw, value) in [
        (".", "."),
        ("..", ".."),
        ("%2e", "."),
        ("%2E%2e", ".."),
        (".%2E", ".."),
        ("%2e.", ".."),
    ] {
        let uri = format!("{root}/10/{raw}");
        assert_eq!(
            parse_digital_link(&uri).unwrap().elements()[1].value(),
            value
        );
        assert!(validate_digital_link(&uri).ok());
        assert_eq!(
            normalize_digital_link(&uri).unwrap(),
            format!("{root}?10={value}")
        );
        let elements = [
            Element::new("01", "04912345678904"),
            Element::new("10", value),
        ];
        assert_eq!(
            create_digital_link_with_options(
                &elements,
                &DigitalLinkOptions::for_base_url("https://example.com")
                    .with_path_ais(vec!["10".into()])
            )
            .unwrap(),
            format!("{root}?10={value}")
        );
    }
    let erased = "https://example.com/01/./../01/04912345678904";
    assert!(parse_digital_link(erased).is_err());
    assert!(!validate_digital_link(erased).ok());
    assert!(normalize_digital_link(erased).is_err());
    let query = format!("{root}?10=..&21=.&utm=a&utm=b");
    assert_eq!(normalize_digital_link(&query).unwrap(), query);
    assert_eq!(
        create_digital_link(
            &[
                Element::new("01", "04912345678904"),
                Element::new("10", "%2e")
            ],
            "https://example.com"
        )
        .unwrap(),
        format!("{root}/10/%252e")
    );
    assert_eq!(
        create_digital_link(
            &[Element::new("01", "04912345678904")],
            "https://example.com/a/../b"
        )
        .unwrap(),
        "https://example.com/b/01/04912345678904"
    );
}

#[test]
fn print_geometry_and_typed_ecc_remain_safe() {
    // Rust options validate against the maximum version extent. This may
    // conservatively reject a finite small symbol; do not silently loosen it.
    for dpi in [f64::from_bits(1), 1e-305, 1e-304] {
        for version in [1, 40] {
            let o = Options {
                print_dpi: Some(dpi),
                version: Some(version),
                ..Default::default()
            };
            assert_eq!(o.validate().unwrap_err().kind(), ErrorCode::InvalidInput);
            assert!(specqr::generate("A", &o).is_err());
        }
    }
    for dpi in [1e-300, 300.0, f64::MAX] {
        let q = specqr::generate(
            "A",
            &Options {
                print_dpi: Some(dpi),
                ..Default::default()
            },
        )
        .unwrap();
        let print = q.diagnostics().get("print").unwrap();
        assert!(
            print
                .get("moduleSizeMm")
                .unwrap()
                .as_f64()
                .unwrap()
                .is_finite()
        );
        assert!(
            print
                .get("symbolSizeMm")
                .unwrap()
                .as_f64()
                .unwrap()
                .is_finite()
        );
    }
    for invalid in [
        "invalid",
        "constructor",
        "toString",
        "valueOf",
        "__proto__",
        "hasOwnProperty",
    ] {
        assert_eq!(
            Ecc::from_str(invalid).unwrap_err().kind(),
            ErrorCode::InvalidEcc
        );
    }
    for ecc in [Ecc::L, Ecc::M, Ecc::Q, Ecc::H] {
        let o = Options {
            ecc,
            ..Default::default()
        };
        assert_eq!(specqr::generate("A", &o).unwrap().ecc(), ecc);
    }
    let a = specqr::generate("A", &Options::default()).unwrap();
    let b = specqr::generate(
        "A",
        &Options {
            print_dpi: Some(300.0),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(a.matrix(), b.matrix());
}
