use specqr::gs1::*;
const ROOT: &str = "https://example.com/01/04912345678904";
fn sample() -> Vec<Element> {
    from_human_readable("(01)04912345678904(10)100%(17)251231").unwrap()
}

#[test]
fn catalog_is_exact_and_bounded() {
    assert_eq!(get_supported_ais().len(), 50);
    assert_eq!(get_supported_ais()[0].ai(), "00");
    assert_eq!(get_supported_ais()[49].ai(), "99");
    assert_eq!(
        get_ai_info("10").unwrap().digital_link_path_for_primary(),
        Some(&["01"][..])
    );
    assert_eq!(get_ai_info("01").unwrap().length().exact(), Some(14));
    assert_eq!(get_ai_info("91").unwrap().length().max(), Some(90));
    for unsupported in ["90", "3106", "3206", "416", "423", "001", "１"] {
        assert!(get_ai_info(unsupported).is_none());
    }
    for info in get_supported_ais() {
        let value = match info.length().exact() {
            Some(n) => "0".repeat(n),
            None => {
                if info.value_kind() == "numeric" {
                    "1".into()
                } else {
                    "A".into()
                }
            }
        };
        let e = Element::new(info.ai(), &value);
        assert!(
            validate_elements(std::slice::from_ref(&e)).ok(),
            "{}",
            info.ai()
        );
        let hri = to_human_readable(std::slice::from_ref(&e)).unwrap();
        assert_eq!(from_human_readable(&hri).unwrap(), vec![e.clone()]);
        assert_eq!(
            parse_element_string(&to_element_string(std::slice::from_ref(&e)).unwrap())
                .unwrap()
                .elements(),
            &[e]
        );
        for invalid in [
            "".to_string(),
            "é".into(),
            "A\u{1d}B".into(),
            "A(B)".into(),
            "0".repeat(info.length().exact().or(info.length().max()).unwrap() + 1),
        ] {
            assert!(
                !validate_elements(&[Element::new(info.ai(), invalid)]).ok(),
                "{}",
                info.ai()
            );
        }
    }
}
#[test]
fn checksums_and_leading_zeroes() {
    assert_eq!(calculate_gtin_check_digit("0491234567890").unwrap(), "4");
    assert_eq!(
        append_gtin_check_digit("0491234567890").unwrap(),
        "04912345678904"
    );
    assert!(validate_gtin_check_digit("04912345678904").unwrap());
    assert!(!validate_gtin_check_digit("04912345678905").unwrap());
    assert_eq!(
        append_sscc_check_digit("00000000000000000").unwrap(),
        "000000000000000000"
    );
    assert!(validate_sscc_check_digit("000000000000000000").unwrap());
    for v in ["", "x", "１", "12 3", "+123", "1\n"] {
        assert!(calculate_check_digit(v).is_err());
    }
    assert!(validate_check_digit("1").is_err());
    for n in 1..=20 {
        assert_eq!(
            calculate_gtin_check_digit(&"0".repeat(n)).is_ok(),
            [7, 11, 12, 13].contains(&n)
        );
    }
}
#[test]
fn element_roundtrip_and_ambiguity() {
    let elements = sample();
    let raw = to_element_string(&elements).unwrap();
    assert_eq!(raw, "010491234567890410100%\u{1d}17251231");
    assert_eq!(parse_element_string(&raw).unwrap().elements(), elements);
    assert!(parse_element_string(&raw).unwrap().has_separators());
    assert_eq!(
        element_string_to_human_readable(&raw).unwrap(),
        "(01)04912345678904(10)100%(17)251231"
    );
    for value in ["10ABC17251231", "10ABC17XXXXXX"] {
        let r = validate_element_string(value);
        assert_eq!(r.errors()[0].code(), "GS1_MISSING_SEPARATOR");
        assert_eq!(r.errors()[0].offset(), Some(2));
        assert_eq!(r.errors()[0].ai(), Some("10"));
    }
    for value in [
        "\u{1d}10ABC",
        "17251231\u{1d}10ABC",
        "10ABC\u{1d}",
        "10ABC\u{1d}\u{1d}21X",
    ] {
        assert_eq!(
            validate_element_string(value).errors()[0].code(),
            "GS1_UNEXPECTED_SEPARATOR"
        );
    }
    assert!(parse_element_string("(10)ABC").is_err());
    assert!(from_human_readable("10ABC").is_err());
    assert!(from_human_readable("(10ABC").is_err());
    assert!(to_element_string(&[]).is_err());
}
#[test]
fn structured_diagnostics() {
    let elements = [
        Element::new("01", "04912345678905"),
        Element::new("17", "abc123"),
        Element::new("91", ""),
    ];
    let r = validate_elements(&elements);
    assert_eq!(r.errors().len(), 3);
    assert_eq!(r.errors()[0].code(), "GS1_INVALID_CHECK_DIGIT");
    assert_eq!(r.errors()[0].ai(), Some("01"));
    assert_eq!(r.errors()[0].value(), Some("04912345678905"));
    assert_eq!(r.errors()[0].element_index(), Some(0));
    assert_eq!(r.errors()[1].code(), "GS1_INVALID_CHARSET");
    assert_eq!(
        r.errors()[1].expected(),
        Some(&Expected::Text("digits only".into()))
    );
    assert_eq!(
        validate_elements_with_options(
            &elements,
            &ValidationOptions::default().with_collect_all_errors(false)
        )
        .errors()
        .len(),
        1
    );
    let short = validate_elements(&[Element::new("17", "1")]);
    assert_eq!(
        short.errors()[0].expected(),
        Some(&Expected::Text("exactly 6 characters".into()))
    );
    let r = validate_elements_with_options(
        &[Element::new("10", "A")],
        &ValidationOptions::default().with_context("digital-link"),
    );
    assert_eq!(r.errors()[0].code(), "GS1_INVALID_DIGITAL_LINK_PLACEMENT");
    assert!(
        validate_element_string_with_options(
            "10A",
            &ValidationOptions::default().with_context("digital-link")
        )
        .ok()
    );
    for options in [
        ValidationOptions::default().with_context("invalid"),
        ValidationOptions::default().with_allow_unsupported_ai(true),
    ] {
        assert_eq!(
            validate_elements_with_options(&sample(), &options).errors()[0].reason(),
            "invalid-options"
        );
    }
}
#[test]
fn links_preserve_elements_and_sort_query() {
    let elements = sample();
    let link = create_digital_link(&elements, "https://EXAMPLE.com:443/products/").unwrap();
    assert_eq!(
        link,
        "https://example.com/products/01/04912345678904/10/100%25?17=251231"
    );
    let p = parse_digital_link(&link).unwrap();
    assert_eq!(p.elements(), elements);
    assert_eq!(p.primary().ai(), "01");
    assert_eq!(p.path_elements().len(), 2);
    assert_eq!(p.query_elements().len(), 1);
    assert_eq!(normalize_digital_link(&link).unwrap(), link);
    let opts = DigitalLinkOptions::for_base_url("https://example.com").with_path_ais(vec![]);
    assert_eq!(
        create_digital_link_with_options(&elements, &opts).unwrap(),
        format!("{ROOT}?10=100%25&17=251231")
    );
    for primary in ["00", "414"] {
        let val = if primary == "00" {
            "000000000000000000"
        } else {
            "0000000000000"
        };
        let opts = DigitalLinkOptions::for_base_url("https://example.com").with_primary_ai(primary);
        let link = create_digital_link_with_options(
            &[Element::new(primary, val), Element::new("10", "A")],
            &opts,
        )
        .unwrap();
        assert_eq!(link, format!("https://example.com/{primary}/{val}?10=A"));
        assert_eq!(parse_digital_link(&link).unwrap().primary().ai(), primary);
    }
}
#[test]
fn duplicate_placement_and_option_errors() {
    assert_eq!(
        validate_digital_link(&format!("{ROOT}/17/251231")).errors()[0].code(),
        "GS1_INVALID_DIGITAL_LINK_PLACEMENT"
    );
    assert_eq!(
        validate_digital_link(&format!("{ROOT}/10/A?10=B")).errors()[0].code(),
        "GS1_DUPLICATE_AI"
    );
    assert_eq!(
        validate_digital_link(&format!("{ROOT}?90=A")).errors()[0].code(),
        "GS1_UNSUPPORTED_AI"
    );
    assert_eq!(
        validate_digital_link(&format!("{ROOT}#fragment")).errors()[0].code(),
        "GS1_DIGITAL_LINK_FRAGMENT_NOT_ALLOWED"
    );
    for uri in [
        "ftp://example.com/01/04912345678904",
        "https://",
        "/01/04912345678904",
    ] {
        assert_eq!(
            validate_digital_link(uri).errors()[0].code(),
            "GS1_DIGITAL_LINK_INVALID_URI"
        );
    }
    for tail in ["/10", "/foo/A", "/10//A"] {
        assert_eq!(
            validate_digital_link(&format!("{ROOT}{tail}")).errors()[0].reason(),
            "malformed-path"
        );
    }
    assert_eq!(
        validate_digital_link_with_options(
            ROOT,
            &DigitalLinkOptions::default().with_primary_ai("10")
        )
        .errors()[0]
            .reason(),
        "invalid-options"
    );
    assert_eq!(
        validate_digital_link_with_options(
            ROOT,
            &DigitalLinkOptions::default().with_normalize(true)
        )
        .errors()[0]
            .expected(),
        Some(&Expected::Boolean(false))
    );
    assert!(
        normalize_digital_link_with_options(
            ROOT,
            &DigitalLinkOptions::default().with_mode("canonical")
        )
        .is_err()
    );
    assert!(create_digital_link(&sample(), "https://example.com?x=1").is_err());
    assert!(
        create_digital_link_with_options(
            &sample(),
            &DigitalLinkOptions::for_base_url("https://example.com")
                .with_path_ais(vec!["17".into()])
        )
        .is_err()
    );
}
#[test]
fn dot_only_gs1_values_are_never_lost() {
    for (raw, value) in [
        (".", "."),
        ("..", ".."),
        ("%2e", "."),
        ("%2E.", ".."),
        (".%2E", ".."),
        ("%2e%2e", ".."),
    ] {
        let link = format!("{ROOT}/10/{raw}");
        assert_eq!(
            parse_digital_link(&link).unwrap().elements()[1].value(),
            value
        );
        assert_eq!(
            normalize_digital_link(&link).unwrap(),
            format!("{ROOT}?10={value}")
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
            format!("{ROOT}?10={value}")
        );
    }
    assert_eq!(
        parse_digital_link(&format!("{ROOT}/10/%252e"))
            .unwrap()
            .elements()[1]
            .value(),
        "%2e"
    );
    assert_eq!(
        normalize_digital_link("https://example.com/a/../b/01/04912345678904").unwrap(),
        "https://example.com/b/01/04912345678904"
    );
}
#[test]
fn path_utf8_and_percent_errors_survive_parsing() {
    for token in [
        "%",
        "%0",
        "%GG",
        "%FF",
        "%ED%A0%80",
        "%E2%82",
        "%C0%AF",
        "%F4%90%80%80",
    ] {
        let link = format!("{ROOT}/10/{token}");
        assert!(parse_digital_link(&link).is_err(), "{token}");
        assert!(normalize_digital_link(&link).is_err(), "{token}");
        assert_eq!(
            validate_digital_link(&link).errors()[0].code(),
            "GS1_INVALID_PERCENT_ENCODING",
            "{token}"
        );
    }
    for token in ["%F0%9F%98%80", "%00", "%1D", "%28"] {
        assert!(parse_digital_link(&format!("{ROOT}/10/{token}")).is_err());
    }
    assert_eq!(
        parse_digital_link(&format!("{ROOT}/10/A%2FB"))
            .unwrap()
            .elements()[1]
            .value(),
        "A/B"
    );
    assert_eq!(
        parse_digital_link(&format!("{ROOT}/10/A+B"))
            .unwrap()
            .elements()[1]
            .value(),
        "A+B"
    );
}
#[test]
fn query_replacement_decoding_and_unknown_order() {
    let link = format!("{ROOT}?x=a+b&x=%ED%A0%80&y=%E2%82&z=%F0%9F%98%80&nul=%00");
    let parsed = parse_digital_link(&link).unwrap();
    let q = parsed.unknown_query();
    assert_eq!(q.len(), 5);
    assert_eq!(q[0].value(), "a b");
    assert_eq!(q[1].value(), "\u{fffd}\u{fffd}\u{fffd}");
    assert_eq!(q[2].value(), "\u{fffd}");
    assert_eq!(q[3].value(), "😀");
    assert_eq!(q[4].value(), "\0");
    let out = normalize_digital_link(&link).unwrap();
    assert_eq!(parse_digital_link(&out).unwrap(), parsed);
    assert_eq!(normalize_digital_link(&out).unwrap(), out);
    let bad = format!("{ROOT}?x=%GG");
    assert_eq!(
        parse_digital_link(&bad).unwrap().unknown_query()[0].value(),
        "%GG"
    );
    assert_eq!(
        validate_digital_link(&bad).errors()[0].code(),
        "GS1_INVALID_PERCENT_ENCODING"
    );
    assert!(normalize_digital_link(&bad).is_err());
    let r = validate_digital_link(&link.replace("https:", "http:"));
    assert_eq!(r.warnings().len(), 2);
    assert_eq!(r.warnings()[1].count(), Some(5));
    let r = validate_digital_link_with_options(
        &format!("{ROOT}?quote%22back%5Cline%0A=1"),
        &DigitalLinkOptions::default().with_unknown_query("reject"),
    );
    assert_eq!(r.errors()[0].key(), Some("quote\"back\\line\n"));
}
#[test]
fn ascii_url_authority_profile() {
    for (raw, host) in [
        ("EXAMPLE.COM.", "example.com."),
        ("%65xample.com", "example.com"),
        ("0x7f000001", "127.0.0.1"),
        ("0177.1", "127.0.0.1"),
        ("127.1", "127.0.0.1"),
        ("4294967295", "255.255.255.255"),
        ("0X7F.1", "127.0.0.1"),
        ("[0:0:0:1:0:0:0:1]", "[::1:0:0:0:1]"),
        ("[::ffff:192.0.2.128]", "[::ffff:c000:280]"),
    ] {
        assert_eq!(
            normalize_digital_link(&format!("https://{raw}/01/04912345678904")).unwrap(),
            format!("https://{host}/01/04912345678904")
        );
    }
    for host in [
        "4294967296",
        "1.2.3.256",
        "1.2.3.4.5",
        "09",
        "08",
        "a%2fb",
        "%ff",
        "[:::]",
        "[1:2]",
        "[::%25eth0]",
        "[::ffff:192.00.2.128]",
    ] {
        assert!(
            parse_digital_link(&format!("https://{host}/01/04912345678904")).is_err(),
            "{host}"
        );
    }
    assert_eq!(
        normalize_digital_link(" \0HTTPS:\\EXAMPLE.com:0443/01/04912345678904\r\n").unwrap(),
        ROOT
    );
    assert_eq!(
        normalize_digital_link("https://user:pa:ss@EXAMPLE.com:443/01/04912345678904").unwrap(),
        "https://user:pa%3Ass@example.com/01/04912345678904"
    );
}
#[test]
fn bounded_unicode_host_profile() {
    for (raw, ascii) in [
        ("例え.テスト", "xn--r8jz45g.xn--zckzah"),
        ("faß.de", "xn--fa-hia.de"),
        ("😀.test", "xn--e28h.test"),
        ("é.test", "xn--9ca.test"),
        ("ｅｘａｍｐｌｅ.com", "example.com"),
        ("a。b", "a.b"),
        ("ẞ.de", "xn--zca.de"),
    ] {
        assert_eq!(
            normalize_digital_link(&format!("https://{raw}/01/04912345678904")).unwrap(),
            format!("https://{ascii}/01/04912345678904")
        );
    }
    for host in [
        "xn--a",
        "xn--",
        "xn--abc",
        "xn--0.pt",
        "a\u{200c}b.test",
        "a\u{200d}b.test",
        "\u{0600}.test",
    ] {
        assert!(
            parse_digital_link(&format!("https://{host}/01/04912345678904")).is_err(),
            "{host}"
        );
    }
}
#[test]
fn hostile_work_is_bounded_without_panics() {
    assert!(parse_element_string(&"a".repeat(MAX_INPUT_CHARACTERS + 1)).is_err());
    assert!(parse_element_string(&format!("10{}", "A".repeat(MAX_INPUT_CHARACTERS - 2))).is_err());
    assert!(to_element_string(&vec![Element::new("10", "A"); MAX_ELEMENTS + 1]).is_err());
    assert!(parse_digital_link(&format!("{ROOT}?{}", "&".repeat(MAX_ELEMENTS))).is_err());
    assert!(
        parse_digital_link(&format!(
            "https://example.com/{}/01/04912345678904",
            "a/".repeat(MAX_ELEMENTS)
        ))
        .is_err()
    );
    assert!(
        parse_digital_link(&format!(
            "https://example.com:{}/01/04912345678904",
            "9".repeat(1000)
        ))
        .is_err()
    );
    for cp in [
        0, 1, 29, 31, 127, 128, 0x300, 0xd7ff, 0xe000, 0x10000, 0x10ffff,
    ] {
        let c = char::from_u32(cp).unwrap();
        for raw in [
            format!("10{c}"),
            format!("17{c}12345"),
            format!("01{c}1234567890123"),
        ] {
            let _ = parse_element_string(&raw);
        }
    }
}

#[test]
fn unsupported_host_forms_are_rejected_without_emitting_invalid_alabels() {
    for host in [
        "e\u{301}.com",
        "\u{34f}.example",
        "xn--a-ecp.example",
        "xn--e-xbb.com",
        "ﬁ.example",
        "①.example",
        "塚.example",
        "\u{fe0f}.example",
        "xn--jm6c.example",
        "xn--orh.example",
        "xn--nf6c.example",
        "xn--v86c.example",
        "אa.example",
        "aא.example",
        "xn--a-zhc.example",
        "א-.example",
        "ا1١.example",
        "가.example",
        "Ꭰ.example",
        "ᏸ.example",
        "\u{1c80}.example",
        "\u{1f80}.example",
        "\u{2ff0}.example",
        "\u{fffc}.example",
        "\u{fffd}.example",
    ] {
        let uri = format!("https://{host}/01/04912345678904");
        assert!(parse_digital_link(&uri).is_err(), "parse accepted {host}");
        assert!(
            normalize_digital_link(&uri).is_err(),
            "normalize accepted {host}"
        );
        assert_eq!(
            validate_digital_link(&uri).errors()[0].code(),
            "GS1_DIGITAL_LINK_INVALID_URI",
            "{host}"
        );
        assert!(
            create_digital_link(&sample(), &format!("https://{host}")).is_err(),
            "create accepted {host}"
        );
    }
    for (raw, ascii) in [
        ("bücher.example", "xn--bcher-kva.example"),
        ("βόλος.example", "xn--nxasmm1c.example"),
        ("пример.example", "xn--e1afmkfd.example"),
        ("אב.example", "xn--4dbc.example"),
        ("ا١.example", "xn--mgb0j.example"),
        ("한글.example", "xn--bj0bj06e.example"),
        ("☃.example", "xn--n3h.example"),
    ] {
        assert_eq!(
            normalize_digital_link(&format!("https://{raw}/01/04912345678904")).unwrap(),
            format!("https://{ascii}/01/04912345678904")
        );
        assert!(parse_digital_link(&format!("https://{ascii}/01/04912345678904")).is_ok());
    }
}

#[test]
fn large_many_element_parsers_keep_linear_prefix_work() {
    // 10,000 separately validated elements and nearly one million characters.
    // Each successful prefix is ASCII, so diagnostics offsets equal byte offsets;
    // no parser iteration rescans an accumulated prefix to count UTF-16 units.
    let value = "A".repeat(90);
    let hri = format!("(91){value}").repeat(10_000);
    let elements = from_human_readable(&hri).unwrap();
    assert_eq!(elements.len(), 10_000);
    let raw = to_element_string(&elements).unwrap();
    assert_eq!(raw.len(), 929_999);
    let parsed = parse_element_string(&raw).unwrap();
    assert_eq!(parsed.elements(), elements);
    assert_eq!(to_human_readable(parsed.elements()).unwrap(), hri);
}
