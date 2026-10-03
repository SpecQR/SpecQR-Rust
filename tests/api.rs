use specqr::{
    self, Ecc, ErrorCode, Mode, Options, Segment,
    json::{self, Value},
};
#[test]
fn default_encode_plan_and_typed_results() {
    let o = Options::default();
    let p = specqr::estimate("HELLO 123", &o).unwrap();
    let q = specqr::generate("HELLO 123", &o).unwrap();
    assert!(p.ok());
    assert_eq!(p.version(), Some(q.version()));
    assert_eq!(
        p.required_bits(),
        q.diagnostics()
            .get("dataBitLength")
            .unwrap()
            .as_u64()
            .unwrap() as usize
    );
    assert_eq!(q.size(), 21);
    assert_eq!(q.ecc(), Ecc::M);
    assert_eq!(q.data_codewords().len(), 16);
    assert_eq!(q.codewords().len(), 26);
    assert_eq!(p.diagnostics().get("maskEvaluated"), Some(&false.into()));
    assert_eq!(q.diagnostics().get("maskEvaluated"), Some(&true.into()));
    assert!(q.module(usize::MAX, 0).is_err());
    assert!(q.module(0, usize::MAX).is_err());
    assert!(q.module(0, 0).unwrap());
    fn send_sync<T: Send + Sync>() {}
    send_sync::<specqr::QrCode>();
    send_sync::<specqr::Plan>();
    send_sync::<Segment>();
    send_sync::<Options>();
}
#[test]
fn all_capacity_combinations_and_bounds() {
    for version in 1..=40 {
        for ecc in [Ecc::L, Ecc::M, Ecc::Q, Ecc::H] {
            for mode in [Mode::Numeric, Mode::Alphanumeric, Mode::Byte, Mode::Kanji] {
                let c = specqr::get_capacity(version, ecc, Some(mode), 0).unwrap();
                assert!(c.maximum().unwrap() > 0);
                assert_eq!(c.capacity_bits(), c.data_codewords() * 8);
                assert!(c.total_codewords() > c.data_codewords());
                assert_eq!(c.size(), usize::from(version) * 4 + 17);
            }
        }
    }
    assert_eq!(
        specqr::get_capacity(1, Ecc::L, Some(Mode::Numeric), 0)
            .unwrap()
            .maximum(),
        Some(41)
    );
    assert_eq!(
        specqr::get_capacity(40, Ecc::L, Some(Mode::Byte), 0)
            .unwrap()
            .maximum(),
        Some(2953)
    );
    for bad in [0, 41, 255] {
        assert!(specqr::get_capacity(bad, Ecc::M, None, 0).is_err());
    }
    assert!(specqr::get_capacity(1, Ecc::M, Some(Mode::Eci), 0).is_err());
    assert!(specqr::get_capacity(1, Ecc::M, None, u64::MAX).is_err());
    assert_eq!(
        specqr::get_capacity(1, Ecc::M, Some(Mode::Byte), (1u64 << 53) - 1)
            .unwrap()
            .maximum(),
        Some(0)
    );
}
#[test]
fn planning_overflow_boost_and_ranges() {
    let o = Options {
        version: Some(1),
        ecc: Ecc::H,
        ..Default::default()
    };
    let p = specqr::estimate(&"x".repeat(100), &o).unwrap();
    assert!(!p.ok());
    assert!(p.remaining_bits() < 0);
    assert!(p.overflow_bits() > 0);
    assert_eq!(p.version(), Some(1));
    assert_eq!(
        specqr::generate(&"x".repeat(100), &o).unwrap_err().kind(),
        ErrorCode::DataTooLong
    );
    let p = specqr::estimate(
        &"x".repeat(100),
        &Options {
            max_version: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!p.ok());
    assert_eq!(p.version(), None);
    let q = specqr::generate(
        "A",
        &Options {
            ecc: Ecc::L,
            boost_ecc: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(q.ecc(), Ecc::H);
    let q = specqr::generate(
        "A",
        &Options {
            min_version: 10,
            max_version: 11,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(q.version(), 10);
    for options in [
        Options {
            min_version: 0,
            ..Default::default()
        },
        Options {
            max_version: 41,
            ..Default::default()
        },
        Options {
            min_version: 5,
            max_version: 2,
            ..Default::default()
        },
        Options {
            mask: Some(8),
            ..Default::default()
        },
        Options {
            eci: Some(1_000_000),
            ..Default::default()
        },
        Options {
            mode: Some(Mode::Fnc1),
            ..Default::default()
        },
        Options {
            gs1: true,
            eci: Some(26),
            ..Default::default()
        },
        Options {
            print_dpi: Some(f64::NAN),
            ..Default::default()
        },
        Options {
            print_dpi: Some(f64::INFINITY),
            ..Default::default()
        },
        Options {
            print_dpi: Some(f64::MIN_POSITIVE / 1e10),
            ..Default::default()
        },
    ] {
        assert!(options.validate().is_err());
    }
}
#[test]
fn binary_and_percent_safety() {
    let bytes = [0, 255, 128, 13, 10];
    let q = specqr::generate_bytes(&bytes, &Options::default()).unwrap();
    assert_eq!(q.segments()[0].logical_bytes(), &bytes);
    assert!(
        specqr::generate_bytes(
            &bytes,
            &Options {
                mode: Some(Mode::Numeric),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        specqr::generate_bytes(
            &bytes,
            &Options {
                gs1: true,
                ..Default::default()
            }
        )
        .is_err()
    );
    let o = Options {
        gs1: true,
        ..Default::default()
    };
    let q = specqr::generate("10A%B", &o).unwrap();
    assert_eq!(q.segments()[0].mode(), Mode::Fnc1);
    assert_eq!(q.segments()[1].mode(), Mode::Byte);
    assert_eq!(q.segments()[1].text(), Some("10A%B"));
    assert!(
        specqr::generate(
            "10A%B",
            &Options {
                mode: Some(Mode::Alphanumeric),
                ..o
            }
        )
        .is_err()
    );
    let q = specqr::generate(
        "A%B",
        &Options {
            fnc1_second: Some("A".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(q.segments()[1].mode(), Mode::Byte);
    let q = specqr::generate_segments(
        &[Segment::fnc1(), Segment::alphanumeric("10A%%B").unwrap()],
        &Options::default(),
    )
    .unwrap();
    assert_eq!(q.segments()[1].mode(), Mode::Alphanumeric);
}
#[test]
fn concurrency_values_and_diagnostics() {
    let options = Options {
        render: specqr::render::RenderOptions {
            margin: 1,
            foreground: "#888".into(),
            background: "#999".into(),
            ..Default::default()
        },
        print_dpi: Some(9600.0),
        ..Default::default()
    };
    let expected = specqr::generate("THREAD 日本語 12345678901234567890", &options).unwrap();
    let source = expected.matrix().to_vec();
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let o = options.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    let q = specqr::generate("THREAD 日本語 12345678901234567890", &o).unwrap();
                    assert!(
                        q.diagnostics()
                            .get("warnings")
                            .unwrap()
                            .as_array()
                            .unwrap()
                            .len()
                            >= 4
                    );
                }
                specqr::generate("THREAD 日本語 12345678901234567890", &o)
                    .unwrap()
                    .matrix()
                    .to_vec()
            })
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), source);
    }
    assert_eq!(expected.options(), &options);
    let s = json::stringify(expected.diagnostics()).unwrap();
    assert_eq!(json::parse(&s).unwrap(), expected.diagnostics().clone());
}
#[test]
fn malformed_no_panic_public_configuration() {
    for n in 0..256u16 {
        let n = n as u8;
        for option in [
            Options {
                version: Some(n),
                ..Default::default()
            },
            Options {
                mask: Some(n),
                ..Default::default()
            },
            Options {
                min_version: n,
                max_version: n,
                ..Default::default()
            },
        ] {
            let result = std::panic::catch_unwind(|| specqr::estimate("A", &option));
            assert!(result.is_ok());
        }
    }
    let result =
        std::panic::catch_unwind(|| specqr::estimate(&"😀".repeat(1_000_001), &Options::default()));
    assert!(result.unwrap().is_err());
}
#[test]
fn json_strict_and_bounded() {
    for bad in [
        "",
        "[",
        "{",
        "01",
        "1.",
        "1e",
        "+1",
        "NaN",
        "1e999",
        "null true",
        "[1,]",
        "{\"a\":1,}",
        "{\"a\":1,\"a\":2}",
        "\"\\ud800\"",
        "\"\\udc00\"",
        "\"\\ud800\\u0041\"",
        "\"\n\"",
        "\"\\x00\"",
    ] {
        assert!(json::parse(bad).is_err(), "{bad}");
    }
    let text = "{\"a\":[null,true,false,-1.25e2,\"日本語😀\\ud83d\\ude00\\n\\t\\u0000\"]}";
    let value = json::parse(text).unwrap();
    let again = json::stringify(&value).unwrap();
    assert_eq!(json::parse(&again).unwrap(), value);
    assert!(json::parse(&format!("{}0{}", "[".repeat(66), "]".repeat(66))).is_err());
    assert!(json::parse(&" ".repeat(4 * 1024 * 1024 + 1)).is_err());
    assert!(json::stringify(&Value::Number(f64::NAN)).is_err());
    assert_eq!(Value::Number(-1.0).as_u64(), None);
    assert_eq!(Value::Number(1.2).as_i64(), None);
    assert_eq!(Value::Number(f64::NAN).as_i64(), None);
    let mut seed = 0x1937u32;
    for _ in 0..5000 {
        let n = (seed % 100) as usize;
        let mut s = String::new();
        for _ in 0..n {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            s.push(char::from((seed % 128) as u8));
        }
        assert!(std::panic::catch_unwind(|| json::parse(&s)).is_ok());
    }
}
