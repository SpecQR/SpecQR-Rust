use specqr::{
    Mode, Segment, kanji,
    segment::{self, OptimizationTracker, create_segments},
};

fn bit_string(segment: &Segment, version: u8) -> String {
    segment
        .bits(version)
        .unwrap()
        .iter()
        .map(|&bit| char::from(b'0' + bit))
        .collect()
}

#[test]
fn numeric_alphanumeric_utf8_and_kanji_match_known_bits() {
    let cases = [
        (
            Segment::numeric("01234567").unwrap(),
            "00010000001000000000110001010110011000011",
        ),
        (
            Segment::alphanumeric("HELLO").unwrap(),
            "00100000001010110000101101111000110011000",
        ),
        (
            Segment::utf8("é😀").unwrap(),
            "010000000110110000111010100111110000100111111001100010000000",
        ),
        (
            Segment::kanji("漢字").unwrap(),
            "10000000001000111001111110101000011010",
        ),
    ];
    for (segment, expected) in cases {
        assert_eq!(bit_string(&segment, 1), expected);
        assert_eq!(segment.total_bits(1).unwrap(), expected.len());
    }
    let unicode = Segment::utf8("é😀").unwrap();
    assert_eq!(
        (
            unicode.count(),
            unicode.character_count(),
            unicode.byte_count()
        ),
        (6, 2, 6)
    );
    assert_eq!(
        unicode.logical_bytes(),
        [0xc3, 0xa9, 0xf0, 0x9f, 0x98, 0x80]
    );
    let kanji = Segment::kanji("漢字").unwrap();
    assert_eq!(
        (kanji.count(), kanji.character_count(), kanji.byte_count()),
        (2, 2, 4)
    );
    assert_eq!(kanji.logical_bytes(), "漢字".as_bytes());
    assert_eq!(kanji::value('漢'), Some(1855));
    assert_eq!(kanji::value('字'), Some(2586));
}

#[test]
fn canonical_kanji_repertoire_has_expected_whatwg_aliases_and_size() {
    for character in "～∥－￠￡￢①㍉漢あ".chars() {
        assert!(kanji::can_encode(character), "{character}");
    }
    for character in "〜‖−¢£¬A😀ｱ\u{e000}".chars() {
        assert!(!kanji::can_encode(character), "{character}");
    }
    let mut count = 0;
    for scalar in 0..=0xffff {
        if let Some(character) = char::from_u32(scalar) {
            if let Some(code) = kanji::code(character) {
                count += 1;
                assert!((0x8140..=0x9ffc).contains(&code) || (0xe040..=0xebbf).contains(&code));
                assert!(kanji::value(character).unwrap() < 8192);
            }
        }
    }
    assert_eq!(count, 6953);
}

#[test]
fn controls_preserve_exact_wire_designators() {
    for (assignment, length, expected) in [
        (0, 8, 0u32),
        (127, 8, 127),
        (128, 16, 0x8080),
        (16383, 16, 0xbfff),
        (16384, 24, 0xc04000),
        (999999, 24, 0xcf423f),
    ] {
        let segment = Segment::eci(assignment).unwrap();
        assert_eq!(segment.assignment(), Some(assignment));
        assert_eq!(segment.data_bit_length(), length);
        let bits = segment.bits(1).unwrap();
        assert_eq!(&bits[..4], [0, 1, 1, 1]);
        assert_eq!(
            bits[4..]
                .iter()
                .fold(0u32, |value, &bit| (value << 1) | u32::from(bit)),
            expected
        );
    }
    assert_eq!(bit_string(&Segment::fnc1(), 1), "0101");
    for (indicator, codeword, expected) in [
        ("12", 12, "100100001100"),
        ("A", 165, "100110100101"),
        ("z", 222, "100111011110"),
    ] {
        let segment = Segment::fnc1_second(indicator).unwrap();
        assert_eq!(segment.application_indicator(), Some(indicator));
        assert_eq!(segment.application_indicator_codeword(), Some(codeword));
        assert_eq!(bit_string(&segment, 1), expected);
    }
    let header = Segment::structured_append(2, 3, 77).unwrap();
    assert_eq!(
        (header.index(), header.total(), header.parity()),
        (Some(2), Some(3), Some(77))
    );
    assert_eq!(bit_string(&header, 1), "00110001001001001101");
}

#[test]
fn empty_data_segments_and_count_field_transitions() {
    for (mode, length) in [
        (Mode::Numeric, 14),
        (Mode::Alphanumeric, 13),
        (Mode::Byte, 12),
        (Mode::Kanji, 12),
    ] {
        let segment = Segment::from_text(mode, "").unwrap();
        assert_eq!(segment.total_bits(1).unwrap(), length);
        assert_eq!(segment.bits(1).unwrap().len(), length);
        assert!(segment.logical_bytes().is_empty());
    }
    for (mode, text, expected) in [
        (Mode::Numeric, "1", [18, 20, 20, 22]),
        (Mode::Alphanumeric, "A", [19, 21, 21, 23]),
        (Mode::Byte, "x", [20, 28, 28, 28]),
        (Mode::Kanji, "漢", [25, 27, 27, 29]),
    ] {
        let segment = Segment::from_text(mode, text).unwrap();
        assert_eq!(
            [9, 10, 26, 27].map(|version| segment.total_bits(version).unwrap()),
            expected
        );
    }
}

#[test]
fn byte_segments_are_owned_snapshots_and_distinguish_binary_from_text() {
    let mut input = vec![0, 255, 1];
    let segment = Segment::bytes(&input).unwrap();
    input[0] = 17;
    assert_eq!(segment.binary(), Some([0, 255, 1].as_slice()));
    assert_eq!(segment.logical_bytes(), [0, 255, 1]);
    assert_eq!(segment.character_count(), 0);
    assert_eq!(segment.payload_units(), 3);
    assert!(segment.text().is_none());
    assert_ne!(
        Segment::bytes(b"abc").unwrap(),
        Segment::utf8("abc").unwrap()
    );
    assert_eq!(
        Segment::bytes(b"abc").unwrap().bits(1).unwrap(),
        Segment::utf8("abc").unwrap().bits(1).unwrap()
    );
}

#[test]
fn manual_boundaries_repeated_eci_and_percent_escaping_are_preserved() {
    let segments = [
        Segment::eci(26).unwrap(),
        Segment::utf8("a").unwrap(),
        Segment::eci(3).unwrap(),
        Segment::bytes(&[0xe9]).unwrap(),
        Segment::eci(26).unwrap(),
        Segment::utf8("é").unwrap(),
    ];
    assert_eq!(segment::normalize_segments(&segments).unwrap(), segments);
    let expected: Vec<u8> = segments
        .iter()
        .flat_map(|part| part.bits(1).unwrap())
        .collect();
    assert_eq!(segment::bits(&segments, 1).unwrap(), expected);
    let fnc1 = [Segment::fnc1(), Segment::alphanumeric("A%%B%C").unwrap()];
    assert_eq!(
        segment::normalize_segments(&fnc1).unwrap()[1].text(),
        Some("A%%B%C")
    );
    assert_eq!(
        segment::bit_length(&fnc1, 1).unwrap(),
        4 + fnc1[1].total_bits(1).unwrap()
    );
    assert_eq!(segment::bits(&[], 1).unwrap(), Vec::<u8>::new());
}

#[test]
fn manual_control_order_and_combination_rules_reject_invalid_sequences() {
    let data = Segment::utf8("data").unwrap();
    let controls = [
        Segment::fnc1(),
        Segment::fnc1_second("A").unwrap(),
        Segment::structured_append(1, 2, 0).unwrap(),
    ];
    for control in &controls {
        assert!(segment::validate_segments(&[data.clone(), control.clone()]).is_err());
        assert!(segment::validate_segments(&[control.clone(), control.clone()]).is_err());
        assert!(
            segment::validate_segments(&[control.clone(), Segment::eci(26).unwrap(), data.clone()])
                .is_err()
        );
        assert!(segment::validate_segments(&[control.clone(), data.clone()]).is_ok());
    }
    assert_eq!(
        segment::validate_segments(&[Segment::fnc1(), Segment::eci(26).unwrap()])
            .unwrap_err()
            .code(),
        "INVALID_GS1"
    );
}

#[test]
fn arithmetic_estimation_survives_oversize_without_materialization() {
    let bytes = Segment::bytes(&vec![0; 256]).unwrap();
    assert_eq!(bytes.total_bits(1).unwrap(), 2060);
    assert_eq!(bytes.bits(1).unwrap_err().code(), "DATA_TOO_LONG");
    assert_eq!(bytes.bits(10).unwrap().len(), 2068);
    let oversized = Segment::bytes(&vec![0; 100000]).unwrap();
    assert_eq!(oversized.total_bits(40).unwrap(), 800020);
    assert_eq!(oversized.bits(40).unwrap_err().code(), "DATA_TOO_LONG");
    assert_eq!(
        segment::bit_length(std::slice::from_ref(&oversized), 40).unwrap(),
        800020
    );
    assert!(segment::bits(&[oversized], 40).is_err());
    let long_digits = Segment::numeric(&"1".repeat(7089)).unwrap();
    assert_eq!(long_digits.bits(40).unwrap().len(), 23648);
}

#[test]
fn resource_caps_apply_before_expensive_optimization_and_bit_expansion() {
    assert_eq!(
        Segment::bytes(&vec![0; 1000001]).unwrap_err().code(),
        "DATA_TOO_LONG"
    );
    assert!(Segment::utf8(&"😀".repeat(1000001)).is_err());
    assert!(segment::normalize_segments(&vec![Segment::fnc1(); 16385]).is_err());
    let piece = Segment::bytes(&vec![0; 600000]).unwrap();
    assert!(segment::normalize_segments(&[piece.clone(), piece]).is_err());
    assert!(create_segments(&"1".repeat(7090), 1, None, true, true).is_err());
    assert_eq!(
        create_segments(&"1".repeat(100000), 1, None, false, true).unwrap()[0].mode(),
        Mode::Numeric
    );
    assert_eq!(
        create_segments(&"1".repeat(100000), 1, Some(Mode::Numeric), true, true).unwrap()[0]
            .count(),
        100000
    );
}

#[test]
fn automatic_selection_and_mixed_optimization() {
    for (text, mode) in [
        ("1234567890", Mode::Numeric),
        ("HELLO WORLD", Mode::Alphanumeric),
        ("こんにちは", Mode::Kanji),
        ("https://example.com", Mode::Byte),
        ("", Mode::Byte),
    ] {
        for optimize in [false, true] {
            let segments = create_segments(text, 1, None, optimize, true).unwrap();
            assert_eq!(segments, [Segment::from_text(mode, text).unwrap()]);
        }
    }
    let text = "abc123456789012345678901234567890def";
    let segments = create_segments(text, 1, None, true, true).unwrap();
    assert_eq!(
        segments,
        [
            Segment::utf8("abc").unwrap(),
            Segment::numeric("123456789012345678901234567890").unwrap(),
            Segment::utf8("def").unwrap()
        ]
    );
    assert!(
        segment::bit_length(&segments, 1).unwrap()
            < segment::bit_length(&create_segments(text, 1, None, false, true).unwrap(), 1)
                .unwrap()
    );
    for optimize in [false, true] {
        assert_eq!(
            create_segments("こんにちは", 1, None, optimize, false).unwrap(),
            [Segment::utf8("こんにちは").unwrap()]
        );
    }
    assert_eq!(
        create_segments("漢字", 1, Some(Mode::Kanji), true, false).unwrap(),
        [Segment::kanji("漢字").unwrap()]
    );
}

#[test]
fn incremental_tracker_matches_every_optimized_prefix() {
    let text = "AB12x漢字1234567890123456789😀HELLO";
    for version in [1, 10, 27] {
        for allow_kanji in [false, true] {
            let mut tracker = OptimizationTracker::new(version, allow_kanji).unwrap();
            let mut prefix = String::new();
            for character in text.chars() {
                prefix.push(character);
                let actual = tracker.append(character).unwrap();
                let segments = create_segments(&prefix, version, None, true, allow_kanji).unwrap();
                assert_eq!(actual, segment::bit_length(&segments, version).unwrap());
            }
        }
    }
}

#[test]
fn optimized_cost_matches_independent_partition_search() {
    let alphabet: Vec<char> = "012ABxy漢😀".chars().collect();
    let mut random = 9172u32;
    for version in [1, 10, 27] {
        for _ in 0..120 {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            let length = 1 + random as usize % 13;
            let mut text = String::new();
            for _ in 0..length {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                text.push(alphabet[random as usize % alphabet.len()]);
            }
            // Independent O(n^2) partition search, without modulo-state recurrence.
            let mut offsets: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
            offsets.push(text.len());
            let mut best = vec![usize::MAX / 2; length + 1];
            best[0] = 0;
            for end in 1..=length {
                for start in 0..end {
                    for mode in [Mode::Numeric, Mode::Alphanumeric, Mode::Kanji, Mode::Byte] {
                        if let Ok(segment) =
                            Segment::from_text(mode, &text[offsets[start]..offsets[end]])
                        {
                            best[end] =
                                best[end].min(best[start] + segment.total_bits(version).unwrap());
                        }
                    }
                }
            }
            let actual = create_segments(&text, version, None, true, true).unwrap();
            assert_eq!(
                segment::bit_length(&actual, version).unwrap(),
                best[length],
                "version={version}, text={text}"
            );
            assert_eq!(
                actual
                    .iter()
                    .filter_map(|segment| segment.text())
                    .collect::<String>(),
                text
            );
            assert!(
                segment::bits(&actual, version)
                    .unwrap()
                    .iter()
                    .all(|&bit| bit <= 1)
            );
        }
    }
}

#[test]
fn invalid_typed_segment_inputs_are_errors() {
    assert!(Segment::numeric("１２").is_err());
    assert!(Segment::numeric("1x").is_err());
    assert!(Segment::alphanumeric("lowercase").is_err());
    assert!(Segment::kanji("😀").is_err());
    assert!(Segment::from_text(Mode::Eci, "").is_err());
    assert_eq!(Segment::eci(1000000).unwrap_err().code(), "INVALID_ECI");
    for indicator in ["", "1", "123", "AA", "é", "-", "１"] {
        assert!(Segment::fnc1_second(indicator).is_err());
    }
    for (index, total) in [(0, 2), (1, 1), (3, 2), (1, 17), (255, 255)] {
        assert!(Segment::structured_append(index, total, 255).is_err());
    }
    for version in [0, 41, 255] {
        assert!(Segment::utf8("").unwrap().bits(version).is_err());
        assert!(segment::bit_length(&[], version).is_err());
        assert!(create_segments("", version, None, true, true).is_err());
        assert!(OptimizationTracker::new(version, true).is_err());
    }
    assert!("auto".parse::<Mode>().is_err());
    assert_eq!(
        "structured-append".parse::<Mode>().unwrap().as_str(),
        "structured-append"
    );
}
