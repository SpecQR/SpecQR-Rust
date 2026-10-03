use specqr::{Ecc, Mode, Segment, core::*, segment, tables::*};

#[test]
fn tables_cover_all_versions_and_strengths() {
    assert_eq!((size(1).unwrap(), size(40).unwrap()), (21, 177));
    assert_eq!(
        (
            raw_codeword_count(1).unwrap(),
            raw_codeword_count(40).unwrap()
        ),
        (26, 3706)
    );
    assert_eq!(
        Ecc::ALL.map(|level| data_codeword_count(1, level).unwrap()),
        [19, 16, 13, 9]
    );
    assert_eq!(data_codeword_count(40, Ecc::L).unwrap(), 2956);
    for (version, expected) in [
        (1, vec![]),
        (2, vec![6, 18]),
        (7, vec![6, 22, 38]),
        (32, vec![6, 34, 60, 86, 112, 138]),
        (40, vec![6, 30, 58, 86, 114, 142, 170]),
    ] {
        assert_eq!(alignment_positions(version).unwrap(), expected);
    }
    for version in 1..=40 {
        let raw = raw_codeword_count(version).unwrap();
        let mut previous = raw;
        for level in Ecc::ALL {
            let info = block_info(version, level).unwrap();
            assert_eq!(
                info.data_codewords() + info.blocks() * info.ecc_per_block(),
                raw
            );
            assert!(info.data_codewords() < previous);
            assert!((7..=30).contains(&info.ecc_per_block()));
            previous = info.data_codewords();
        }
    }
}

#[test]
fn count_fields_change_at_versions_10_and_27() {
    for (mode, widths) in [
        (Mode::Numeric, [10, 12, 12, 14]),
        (Mode::Alphanumeric, [9, 11, 11, 13]),
        (Mode::Byte, [8, 16, 16, 16]),
        (Mode::Kanji, [8, 10, 10, 12]),
    ] {
        assert_eq!(
            [9, 10, 26, 27].map(|version| character_count_bits(version, mode).unwrap()),
            widths
        );
    }
}

#[test]
fn padding_handles_empty_partial_and_full_bits() {
    assert_eq!(
        pad_data_bits(&[], 1, Ecc::H).unwrap(),
        [0, 236, 17, 236, 17, 236, 17, 236, 17]
    );
    assert_eq!(
        &pad_data_bits(&[1, 0, 1], 1, Ecc::H).unwrap()[..3],
        [160, 236, 17]
    );
    assert_eq!(pad_data_bits(&[1; 72], 1, Ecc::H).unwrap(), [255; 9]);
    for (length, last) in [(71, 254), (68, 240), (64, 0)] {
        let result = pad_data_bits(&vec![1; length], 1, Ecc::H).unwrap();
        assert_eq!(&result[..8], [255; 8]);
        assert_eq!(result[8], last);
    }
}

#[test]
fn field_products_match_independent_polynomial_long_division_exhaustively() {
    for left in 0u16..=255 {
        for right in 0u16..=255 {
            let mut polynomial = 0u16;
            for bit in 0..8 {
                if right & (1 << bit) != 0 {
                    polynomial ^= left << bit;
                }
            }
            for bit in (8..=14).rev() {
                if polynomial & (1 << bit) != 0 {
                    polynomial ^= 0x11d << (bit - 8);
                }
            }
            assert_eq!(gf_multiply(left as u8, right as u8), polynomial as u8);
        }
    }
    assert_eq!(gf_multiply(0x53, 0xca), 0x8f);
}

#[test]
fn all_generator_degrees_produce_zero_syndrome_codewords() {
    assert_eq!(
        reed_solomon_divisor(7).unwrap(),
        [1, 127, 122, 154, 164, 11, 68, 117]
    );
    let data: Vec<u8> = (0..40).collect();
    for degree in 1..=255 {
        let divisor = reed_solomon_divisor(degree).unwrap();
        assert_eq!(divisor.len(), usize::from(degree) + 1);
        assert_eq!(divisor[0], 1);
        let parity = reed_solomon_remainder(&data, degree).unwrap();
        let mut codeword = data.clone();
        codeword.extend(&parity);
        assert_eq!(
            reed_solomon_remainder(&codeword, degree).unwrap(),
            vec![0; usize::from(degree)]
        );
        assert_eq!(
            reed_solomon_remainder(&[], degree).unwrap(),
            vec![0; usize::from(degree)]
        );
    }
}

#[test]
fn all_160_block_layouts_reassemble_data_and_valid_parity() {
    for version in 1..=40 {
        for level in Ecc::ALL {
            let info = block_info(version, level).unwrap();
            let data: Vec<u8> = (0..info.data_codewords())
                .map(|index| (index * 197 + 83) as u8)
                .collect();
            let result = interleave_codewords(&data, version, level).unwrap();
            assert_eq!(result.blocks().len(), info.blocks());
            assert_eq!(result.total_codewords(), info.raw_codewords());
            assert_eq!(result.data_codewords(), data.len());
            assert_eq!(
                result.error_correction_codewords(),
                info.blocks() * info.ecc_per_block()
            );
            let reconstructed: Vec<u8> = result
                .blocks()
                .iter()
                .flat_map(|block| block.data().iter().copied())
                .collect();
            assert_eq!(reconstructed, data);
            for block in result.blocks() {
                let mut full = block.data().to_vec();
                full.extend(block.ecc());
                assert_eq!(
                    reed_solomon_remainder(&full, info.ecc_per_block() as u8).unwrap(),
                    vec![0; info.ecc_per_block()]
                );
            }
            let matrix = build_matrix(result.codewords(), version, level, Some(0)).unwrap();
            assert_eq!(matrix.matrix().len(), size(version).unwrap());
            assert_eq!(matrix.mask_penalties().len(), 1);
            assert_eq!(matrix.penalty(), penalty_score(matrix.matrix()).unwrap());
            assert!(matrix.matrix()[size(version).unwrap() - 8][8]);
        }
    }
}

#[test]
fn hello_world_matches_pinned_javascript_codewords_matrix_and_all_penalties() {
    // Generated directly from user-owned SpecQR JS SHA 15ad15e5c770ea0e39072f8f88b2733018f02ffd.
    let expected_data = [32, 91, 11, 120, 209, 114, 220, 77, 67, 64, 236, 17, 236];
    let expected_words = [
        32, 91, 11, 120, 209, 114, 220, 77, 67, 64, 236, 17, 236, 168, 72, 22, 82, 217, 54, 156, 0,
        46, 15, 180, 122, 16,
    ];
    let expected_rows = [
        "111111100001001111111",
        "100000101100101000001",
        "101110100101101011101",
        "101110101111101011101",
        "101110101101001011101",
        "100000100100101000001",
        "111111101010101111111",
        "000000001101100000000",
        "010111101100111011010",
        "101111010000111101110",
        "001010110001001100000",
        "101101000101100011000",
        "110111111110111011111",
        "000000001000100101000",
        "111111100110011001111",
        "100000101010010010111",
        "101110101101001000111",
        "101110101011100010100",
        "101110100100001000011",
        "100000101110011100110",
        "111111100101000000010",
    ];
    let bits = segment::bits(&[Segment::alphanumeric("HELLO WORLD").unwrap()], 1).unwrap();
    let data = pad_data_bits(&bits, 1, Ecc::Q).unwrap();
    assert_eq!(data, expected_data);
    let words = interleave_codewords(&data, 1, Ecc::Q).unwrap();
    assert_eq!(words.codewords(), expected_words);
    let matrix = build_matrix(words.codewords(), 1, Ecc::Q, None).unwrap();
    assert_eq!((matrix.mask_pattern(), matrix.penalty()), (6, 314));
    assert_eq!(
        matrix
            .mask_penalties()
            .iter()
            .map(|value| value.penalty())
            .collect::<Vec<_>>(),
        [347, 470, 506, 441, 539, 516, 314, 558]
    );
    let rows: Vec<String> = matrix
        .matrix()
        .iter()
        .map(|row| {
            row.iter()
                .map(|&dark| if dark { '1' } else { '0' })
                .collect()
        })
        .collect();
    assert_eq!(rows, expected_rows);
    for mask in 0..8 {
        let forced = build_matrix(words.codewords(), 1, Ecc::Q, Some(mask)).unwrap();
        assert_eq!(forced.mask_pattern(), mask);
        assert_eq!(
            forced.penalty(),
            matrix.mask_penalties()[usize::from(mask)].penalty()
        );
    }
}

#[test]
fn penalty_rules_include_runs_blocks_and_balance() {
    assert_eq!(penalty_score(&[vec![false]]).unwrap(), 100);
    assert_eq!(
        penalty_score(&[vec![false, false], vec![false, false]]).unwrap(),
        103
    );
    assert_eq!(
        penalty_score(&[vec![true, false], vec![false, true]]).unwrap(),
        0
    );
    assert_eq!(penalty_score(&vec![vec![false; 5]; 5]).unwrap(), 178);
}

#[test]
fn malformed_public_core_inputs_return_typed_errors_without_panicking() {
    for version in [0, 41, 255] {
        assert_eq!(size(version).unwrap_err().code(), "INVALID_VERSION");
        assert!(raw_codeword_count(version).is_err());
        assert!(alignment_positions(version).is_err());
        assert!(interleave_codewords(&[], version, Ecc::L).is_err());
        assert!(build_matrix(&[], version, Ecc::L, None).is_err());
    }
    assert!("l".parse::<Ecc>().is_err());
    assert!(character_count_bits(1, Mode::Eci).is_err());
    assert!(pad_data_bits(&[2], 1, Ecc::L).is_err());
    assert_eq!(
        pad_data_bits(&[0; 73], 1, Ecc::H).unwrap_err().code(),
        "DATA_TOO_LONG"
    );
    assert!(reed_solomon_divisor(0).is_err());
    assert!(reed_solomon_remainder(&[], 0).is_err());
    assert!(reed_solomon_remainder(&[0; 3707], 7).is_err());
    assert!(interleave_codewords(&[], 1, Ecc::L).is_err());
    assert!(build_matrix(&[0; 26], 1, Ecc::L, Some(8)).is_err());
    assert!(build_matrix(&[0; 25], 1, Ecc::L, None).is_err());
    assert!(mask_condition(8, 0, 0).is_err());
    assert!(mask_condition(0, usize::MAX, usize::MAX).is_err());
    assert!(penalty_score(&[]).is_err());
    assert!(penalty_score(&[vec![]]).is_err());
    assert!(penalty_score(&[vec![false; 2]]).is_err());
    assert!(penalty_score(&vec![vec![false; 178]; 178]).is_err());
}
