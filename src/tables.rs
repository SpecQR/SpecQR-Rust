//! Model 2 capacities, alignment positions, and error-correction parameters.
//!
//! Normative data matches user-owned SpecQR commit
//! `15ad15e5c770ea0e39072f8f88b2733018f02ffd`, `src/core/tables.js`.
//! Source SHA-256: `3c53e40f238cb2db2fd5eba20dc70dbcb0d327925123f81dddbde7981785fb37`.

use crate::segment::Mode;
use crate::{Error, ErrorCode, Result};
use std::{fmt, str::FromStr};

/// Error-correction strength, in increasing order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Ecc {
    L,
    M,
    Q,
    H,
}

impl Ecc {
    pub const ALL: [Self; 4] = [Self::L, Self::M, Self::Q, Self::H];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::L => "L",
            Self::M => "M",
            Self::Q => "Q",
            Self::H => "H",
        }
    }
    pub const fn format_bits(self) -> u8 {
        match self {
            Self::L => 1,
            Self::M => 0,
            Self::Q => 3,
            Self::H => 2,
        }
    }
    const fn ordinal(self) -> usize {
        match self {
            Self::L => 0,
            Self::M => 1,
            Self::Q => 2,
            Self::H => 3,
        }
    }
}
impl fmt::Display for Ecc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for Ecc {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "L" => Ok(Self::L),
            "M" => Ok(Self::M),
            "Q" => Ok(Self::Q),
            "H" => Ok(Self::H),
            _ => Err(Error::new(
                ErrorCode::InvalidEcc,
                "Error correction level must be L, M, Q, or H",
            )),
        }
    }
}

const ECC_CODEWORDS_PER_BLOCK: [[usize; 41]; 4] = [
    [
        0, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28,
        30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
    [
        0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28,
        28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
    ],
    [
        0, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30,
        30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
    [
        0, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24,
        30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30,
    ],
];

const NUM_ERROR_CORRECTION_BLOCKS: [[usize; 41]; 4] = [
    [
        0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13,
        14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25,
    ],
    [
        0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21,
        23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49,
    ],
    [
        0, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29,
        34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68,
    ],
    [
        0, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32,
        35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81,
    ],
];

/// Immutable Reed–Solomon block layout for a version and correction level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockInfo {
    blocks: usize,
    ecc_per_block: usize,
    raw_codewords: usize,
    data_codewords: usize,
}
impl BlockInfo {
    pub const fn blocks(&self) -> usize {
        self.blocks
    }
    pub const fn ecc_per_block(&self) -> usize {
        self.ecc_per_block
    }
    pub const fn raw_codewords(&self) -> usize {
        self.raw_codewords
    }
    pub const fn data_codewords(&self) -> usize {
        self.data_codewords
    }
}

pub fn validate_version(version: u8) -> Result<()> {
    if !(1..=40).contains(&version) {
        return Err(Error::new(
            ErrorCode::InvalidVersion,
            "QR version must be from 1 to 40",
        ));
    }
    Ok(())
}
pub fn size(version: u8) -> Result<usize> {
    validate_version(version)?;
    Ok(usize::from(version) * 4 + 17)
}
pub fn raw_codeword_count(version: u8) -> Result<usize> {
    validate_version(version)?;
    let v = usize::from(version);
    let mut modules = (16 * v + 128) * v + 64;
    if v >= 2 {
        let count = v / 7 + 2;
        modules -= (25 * count - 10) * count - 55;
        if v >= 7 {
            modules -= 36;
        }
    }
    Ok(modules / 8)
}
pub fn data_codeword_count(version: u8, level: Ecc) -> Result<usize> {
    Ok(block_info(version, level)?.data_codewords())
}
pub fn block_info(version: u8, level: Ecc) -> Result<BlockInfo> {
    validate_version(version)?;
    let blocks = NUM_ERROR_CORRECTION_BLOCKS[level.ordinal()][usize::from(version)];
    let ecc_per_block = ECC_CODEWORDS_PER_BLOCK[level.ordinal()][usize::from(version)];
    let raw_codewords = raw_codeword_count(version)?;
    Ok(BlockInfo {
        blocks,
        ecc_per_block,
        raw_codewords,
        data_codewords: raw_codewords - blocks * ecc_per_block,
    })
}
pub fn alignment_positions(version: u8) -> Result<Vec<usize>> {
    validate_version(version)?;
    if version == 1 {
        return Ok(Vec::new());
    }
    let v = usize::from(version);
    let count = v / 7 + 2;
    let denominator = count * 2 - 2;
    let step = if v == 32 {
        26
    } else {
        (v * 4 + 4).div_ceil(denominator) * 2
    };
    let mut positions = vec![6];
    for index in (0..count - 1).rev() {
        positions.push(size(version)? - 7 - index * step);
    }
    Ok(positions)
}
pub fn character_count_bits(version: u8, mode: Mode) -> Result<u8> {
    validate_version(version)?;
    let group = if version <= 9 {
        0
    } else if version <= 26 {
        1
    } else {
        2
    };
    let widths = match mode {
        Mode::Numeric => [10, 12, 14],
        Mode::Alphanumeric => [9, 11, 13],
        Mode::Byte => [8, 16, 16],
        Mode::Kanji => [8, 10, 12],
        _ => {
            return Err(Error::new(
                ErrorCode::InvalidMode,
                "Control segments have no character count field",
            ));
        }
    };
    Ok(widths[group])
}
