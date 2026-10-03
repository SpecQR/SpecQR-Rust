//! Pure Rust GF(256), Reed–Solomon blocks, module placement, and mask selection.
//!
//! Behavior follows the user-owned SpecQR source at commit
//! `15ad15e5c770ea0e39072f8f88b2733018f02ffd`. Source SHA-256 fingerprints:
//! - `src/core/codewords.js`: `e06d096eb1ade1f118a9627c419503f289027d2044428309204600d2bd74ed19`
//! - `src/core/galois-field.js`: `2928b5520268b0a795c1a4e14fac460f0dd37ef89a4366e10b1ac49aa3494631`
//! - `src/core/reed-solomon.js`: `65dccf4baa39df55d4eb828418bb3bd0b00d3e7fb3520bff27b569c5010657b2`
//! - `src/core/matrix.js`: `afdf61b811f10041df6cbb4e6a9bb9f55b4a70c66152564131545457cb84ba50`
//! - `src/core/mask.js`: `5df22969324cfc45bf6be7a768b09fae91e741582abfafd021422cc0eaa4fb34`
//!
//! Matrix rows are indexed `[y][x]`. No third-party encoder, FFI, or host codec
//! participates in encoding; every operation is implemented in this crate.

use crate::{
    Error, ErrorCode, Result,
    tables::{self, Ecc},
};

pub const MAX_CODEWORDS: usize = 3_706;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodewordBlock {
    data: Vec<u8>,
    ecc: Vec<u8>,
}
impl CodewordBlock {
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    pub fn ecc(&self) -> &[u8] {
        &self.ecc
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterleavedResult {
    codewords: Vec<u8>,
    blocks: Vec<CodewordBlock>,
    data_codewords: usize,
    error_correction_codewords: usize,
}
impl InterleavedResult {
    pub fn codewords(&self) -> &[u8] {
        &self.codewords
    }
    pub fn blocks(&self) -> &[CodewordBlock] {
        &self.blocks
    }
    pub const fn data_codewords(&self) -> usize {
        self.data_codewords
    }
    pub const fn error_correction_codewords(&self) -> usize {
        self.error_correction_codewords
    }
    pub fn total_codewords(&self) -> usize {
        self.codewords.len()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaskPenalty {
    mask_pattern: u8,
    penalty: usize,
}
impl MaskPenalty {
    pub const fn mask_pattern(&self) -> u8 {
        self.mask_pattern
    }
    pub const fn penalty(&self) -> usize {
        self.penalty
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatrixResult {
    matrix: Vec<Vec<bool>>,
    mask_pattern: u8,
    penalty: usize,
    mask_penalties: Vec<MaskPenalty>,
}
impl MatrixResult {
    pub fn matrix(&self) -> &[Vec<bool>] {
        &self.matrix
    }
    pub const fn mask_pattern(&self) -> u8 {
        self.mask_pattern
    }
    pub const fn penalty(&self) -> usize {
        self.penalty
    }
    pub fn mask_penalties(&self) -> &[MaskPenalty] {
        &self.mask_penalties
    }
}

/// Add the zero terminator, byte alignment, and alternating EC/11 pad codewords.
/// Input length and every 0/1 element are checked before their use.
pub fn pad_data_bits(bits: &[u8], version: u8, level: Ecc) -> Result<Vec<u8>> {
    let capacity = tables::data_codeword_count(version, level)?;
    if bits.len() > capacity * 8 {
        return Err(Error::new(
            ErrorCode::DataTooLong,
            format!(
                "Input requires {} bits; version {}-{} has {}",
                bits.len(),
                version,
                level,
                capacity * 8
            ),
        ));
    }
    if bits.iter().any(|&bit| bit > 1) {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "Bits must contain only 0 or 1",
        ));
    }
    let mut data = vec![0u8; capacity];
    for (index, &bit) in bits.iter().enumerate() {
        data[index / 8] |= bit << (7 - index % 8);
    }
    let terminated = bits.len() + 4.min(capacity * 8 - bits.len());
    let padded_length = terminated.div_ceil(8);
    for (index, value) in data[padded_length..].iter_mut().enumerate() {
        *value = if index % 2 == 0 { 0xec } else { 0x11 };
    }
    Ok(data)
}

/// Multiply bytes in QR's GF(256), with reduction polynomial x^8+x^4+x^3+x^2+1.
pub fn gf_multiply(left: u8, right: u8) -> u8 {
    let mut left = u16::from(left);
    let mut right = right;
    let mut product = 0u16;
    while right != 0 {
        if right & 1 != 0 {
            product ^= left;
        }
        right >>= 1;
        left <<= 1;
        if left & 0x100 != 0 {
            left ^= 0x11d;
        }
    }
    product as u8
}

/// Descending-power generator coefficients, including the leading monic 1.
pub fn reed_solomon_divisor(degree: u8) -> Result<Vec<u8>> {
    if degree == 0 {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "Reed-Solomon degree must be from 1 to 255",
        ));
    }
    let degree = usize::from(degree);
    let mut coefficients = vec![0u8; degree + 1];
    coefficients[0] = 1;
    let mut root = 1u8;
    for factor in 0..degree {
        for index in (1..=factor + 1).rev() {
            coefficients[index] ^= gf_multiply(coefficients[index - 1], root);
        }
        root = gf_multiply(root, 2);
    }
    Ok(coefficients)
}

fn remainder(data: &[u8], divisor: &[u8]) -> Vec<u8> {
    // The only callers obtain a nonempty monic polynomial of degree 1..=255.
    let degree = divisor.len() - 1;
    let mut result = vec![0u8; degree];
    for &value in data {
        let factor = value ^ result[0];
        for index in 0..degree - 1 {
            result[index] = result[index + 1] ^ gf_multiply(divisor[index + 1], factor);
        }
        result[degree - 1] = gf_multiply(divisor[degree], factor);
    }
    result
}
/// Compute QR Reed–Solomon parity for at most one symbol's codewords.
pub fn reed_solomon_remainder(data: &[u8], degree: u8) -> Result<Vec<u8>> {
    if data.len() > MAX_CODEWORDS {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "Data exceeds maximum QR codeword count",
        ));
    }
    Ok(remainder(data, &reed_solomon_divisor(degree)?))
}

/// Divide exact-capacity padded data into blocks, calculate parity, then interleave.
pub fn interleave_codewords(data: &[u8], version: u8, level: Ecc) -> Result<InterleavedResult> {
    let info = tables::block_info(version, level)?;
    if data.len() != info.data_codewords() {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            format!(
                "Expected {} data codewords; got {}",
                info.data_codewords(),
                data.len()
            ),
        ));
    }
    let short_count = info.blocks() - info.raw_codewords() % info.blocks();
    let short_length = info.raw_codewords() / info.blocks() - info.ecc_per_block();
    let divisor = reed_solomon_divisor(info.ecc_per_block() as u8)?;
    let mut blocks = Vec::with_capacity(info.blocks());
    let mut offset = 0;
    for index in 0..info.blocks() {
        let length = short_length + usize::from(index >= short_count);
        let block = data[offset..offset + length].to_vec();
        blocks.push(CodewordBlock {
            ecc: remainder(&block, &divisor),
            data: block,
        });
        offset += length;
    }
    let mut codewords = Vec::with_capacity(info.raw_codewords());
    for column in 0..=short_length {
        for block in &blocks {
            if let Some(&value) = block.data.get(column) {
                codewords.push(value);
            }
        }
    }
    for column in 0..info.ecc_per_block() {
        for block in &blocks {
            codewords.push(block.ecc[column]);
        }
    }
    Ok(InterleavedResult {
        codewords,
        blocks,
        data_codewords: info.data_codewords(),
        error_correction_codewords: info.raw_codewords() - info.data_codewords(),
    })
}

fn mask_at(mask: u8, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y) % 2 == 0,
        1 => y % 2 == 0,
        2 => x % 3 == 0,
        3 => (x + y) % 3 == 0,
        4 => (y / 2 + x / 3) % 2 == 0,
        5 => (x * y) % 2 + (x * y) % 3 == 0,
        6 => ((x * y) % 2 + (x * y) % 3) % 2 == 0,
        _ => ((x + y) % 2 + (x * y) % 3) % 2 == 0,
    }
}
pub fn mask_condition(mask: u8, x: usize, y: usize) -> Result<bool> {
    if mask > 7 || x > 176 || y > 176 {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "Mask must be 0..7 and module coordinates 0..176",
        ));
    }
    Ok(mask_at(mask, x, y))
}

fn line_penalty(line: impl Iterator<Item = bool>) -> usize {
    let mut penalty = 0;
    let mut color = false;
    let mut run = 0;
    let mut window = 0u16;
    for (index, value) in line.enumerate() {
        if run > 0 && value == color {
            run += 1;
        } else {
            if run >= 5 {
                penalty += run - 2;
            }
            color = value;
            run = 1;
        }
        window = ((window << 1) | u16::from(value)) & 0x7ff;
        if index >= 10 && (window == 0b10111010000 || window == 0b00001011101) {
            penalty += 40;
        }
    }
    if run >= 5 {
        penalty += run - 2;
    }
    penalty
}
fn penalty_unchecked(matrix: &[Vec<bool>]) -> usize {
    let side = matrix.len();
    let mut penalty: usize = matrix
        .iter()
        .map(|row| line_penalty(row.iter().copied()))
        .sum();
    for column in 0..side {
        penalty += line_penalty(matrix.iter().map(|row| row[column]));
    }
    for y in 0..side - 1 {
        for x in 0..side - 1 {
            let value = matrix[y][x];
            if value == matrix[y][x + 1]
                && value == matrix[y + 1][x]
                && value == matrix[y + 1][x + 1]
            {
                penalty += 3;
            }
        }
    }
    let total = side * side;
    let dark = matrix
        .iter()
        .flat_map(|row| row.iter())
        .filter(|&&value| value)
        .count();
    penalty + (dark * 20).abs_diff(total * 10) / total * 10
}
/// Score a square 1–177-module matrix using SpecQR's exact N1–N4 rules.
pub fn penalty_score(matrix: &[Vec<bool>]) -> Result<usize> {
    if !(1..=177).contains(&matrix.len()) || matrix.iter().any(|row| row.len() != matrix.len()) {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "Matrix must be square with 1 to 177 rows",
        ));
    }
    Ok(penalty_unchecked(matrix))
}

#[derive(Clone)]
struct Grid {
    side: usize,
    modules: Vec<Vec<bool>>,
    functions: Vec<Vec<bool>>,
}
impl Grid {
    fn new(side: usize) -> Self {
        Self {
            side,
            modules: vec![vec![false; side]; side],
            functions: vec![vec![false; side]; side],
        }
    }
    fn function(&mut self, x: i32, y: i32, dark: bool) {
        if x >= 0 && y >= 0 && (x as usize) < self.side && (y as usize) < self.side {
            self.modules[y as usize][x as usize] = dark;
            self.functions[y as usize][x as usize] = true;
        }
    }
    fn finder(&mut self, left: i32, top: i32) {
        for dy in -1..=7 {
            for dx in -1..=7 {
                let inside = (0..=6).contains(&dx) && (0..=6).contains(&dy);
                let dark = inside
                    && (dx == 0
                        || dx == 6
                        || dy == 0
                        || dy == 6
                        || ((2..=4).contains(&dx) && (2..=4).contains(&dy)));
                self.function(left + dx, top + dy, dark);
            }
        }
    }
    fn draw_format(&mut self, level: Ecc, mask: u8) {
        let data = (u32::from(level.format_bits()) << 3) | u32::from(mask);
        let mut remainder = data;
        for _ in 0..10 {
            remainder = (remainder << 1) ^ (((remainder >> 9) & 1) * 0x537);
        }
        let bits = ((data << 10) | remainder) ^ 0x5412;
        for index in 0..6 {
            self.function(8, index, (bits >> index) & 1 != 0);
        }
        self.function(8, 7, (bits >> 6) & 1 != 0);
        self.function(8, 8, (bits >> 7) & 1 != 0);
        self.function(7, 8, (bits >> 8) & 1 != 0);
        for index in 9..15 {
            self.function(14 - index, 8, (bits >> index) & 1 != 0);
        }
        for index in 0..8 {
            self.function(self.side as i32 - 1 - index, 8, (bits >> index) & 1 != 0);
        }
        for index in 8..15 {
            self.function(8, self.side as i32 - 15 + index, (bits >> index) & 1 != 0);
        }
    }
    fn draw_functions(&mut self, version: u8, level: Ecc) -> Result<()> {
        self.finder(0, 0);
        self.finder(self.side as i32 - 7, 0);
        self.finder(0, self.side as i32 - 7);
        for index in 8..self.side - 8 {
            self.function(index as i32, 6, index % 2 == 0);
            self.function(6, index as i32, index % 2 == 0);
        }
        let positions = tables::alignment_positions(version)?;
        for (yi, &y) in positions.iter().enumerate() {
            for (xi, &x) in positions.iter().enumerate() {
                let last = positions.len() - 1;
                if (yi == 0 && (xi == 0 || xi == last)) || (xi == 0 && yi == last) {
                    continue;
                }
                for dy in -2i32..=2 {
                    for dx in -2i32..=2 {
                        self.function(x as i32 + dx, y as i32 + dy, dx.abs().max(dy.abs()) != 1);
                    }
                }
            }
        }
        self.draw_format(level, 0);
        self.function(8, self.side as i32 - 8, true);
        if version >= 7 {
            let mut remainder = u32::from(version);
            for _ in 0..12 {
                remainder = (remainder << 1) ^ (((remainder >> 11) & 1) * 0x1f25);
            }
            let bits = (u32::from(version) << 12) | remainder;
            for index in 0..18 {
                let a = self.side as i32 - 11 + index % 3;
                let b = index / 3;
                let dark = (bits >> index) & 1 != 0;
                self.function(a, b, dark);
                self.function(b, a, dark);
            }
        }
        Ok(())
    }
    fn draw_codewords(&mut self, codewords: &[u8]) {
        let mut bit = 0usize;
        let mut right = self.side as i32 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            for vertical in 0..self.side {
                let y = if (right + 1) & 2 == 0 {
                    self.side - 1 - vertical
                } else {
                    vertical
                };
                for x in [right as usize, right as usize - 1] {
                    if !self.functions[y][x] {
                        if bit < codewords.len() * 8 {
                            self.modules[y][x] = (codewords[bit / 8] >> (7 - bit % 8)) & 1 != 0;
                        }
                        bit += 1;
                    }
                }
            }
            right -= 2;
        }
    }
    fn masked(&self, level: Ecc, mask: u8) -> Self {
        let mut candidate = self.clone();
        for y in 0..self.side {
            for x in 0..self.side {
                if !self.functions[y][x] && mask_at(mask, x, y) {
                    candidate.modules[y][x] = !candidate.modules[y][x];
                }
            }
        }
        candidate.draw_format(level, mask);
        candidate
    }
}

/// Place all function patterns and codewords. Automatic mask ties choose the
/// lowest mask number; a forced mask returns just that mask's diagnostic score.
pub fn build_matrix(
    codewords: &[u8],
    version: u8,
    level: Ecc,
    mask: Option<u8>,
) -> Result<MatrixResult> {
    let side = tables::size(version)?;
    if mask.is_some_and(|value| value > 7) {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            "Mask pattern must be from 0 to 7",
        ));
    }
    let expected = tables::raw_codeword_count(version)?;
    if codewords.len() != expected {
        return Err(Error::new(
            ErrorCode::InvalidInput,
            format!(
                "Expected {} interleaved codewords; got {}",
                expected,
                codewords.len()
            ),
        ));
    }
    let mut base = Grid::new(side);
    base.draw_functions(version, level)?;
    base.draw_codewords(codewords);
    let mut best: Option<MatrixResult> = None;
    let mut penalties = Vec::with_capacity(if mask.is_some() { 1 } else { 8 });
    let first = mask.unwrap_or(0);
    let last = mask.unwrap_or(7);
    for candidate_mask in first..=last {
        let candidate = base.masked(level, candidate_mask);
        let penalty = penalty_unchecked(&candidate.modules);
        penalties.push(MaskPenalty {
            mask_pattern: candidate_mask,
            penalty,
        });
        if best.as_ref().is_none_or(|best| penalty < best.penalty) {
            best = Some(MatrixResult {
                matrix: candidate.modules,
                mask_pattern: candidate_mask,
                penalty,
                mask_penalties: Vec::new(),
            });
        }
    }
    let mut best = best.ok_or_else(|| Error::new(ErrorCode::InvalidInput, "No mask candidate"))?;
    best.mask_penalties = penalties;
    Ok(best)
}
