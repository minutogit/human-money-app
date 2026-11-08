use crate::error::MarketError;
use crate::model::MAX_QR_SHARE_BYTES;

// QR Code Constants and Tables for ECC Level L (Versions 1..=40)

// RS block definitions for ECC Level L, versions 1..=40
const RS_BLOCKS_L: &[&[(usize, usize)]] = &[
    // v1
    &[(26, 19)],
    // v2
    &[(44, 34)],
    // v3
    &[(70, 55)],
    // v4
    &[(100, 80)],
    // v5
    &[(134, 108)],
    // v6
    &[(86, 68), (86, 68)],
    // v7
    &[(98, 78), (98, 78)],
    // v8
    &[(121, 97), (121, 97)],
    // v9
    &[(146, 116), (146, 116)],
    // v10
    &[(86, 68), (86, 68), (87, 69), (87, 69)],
    // v11
    &[(101, 81), (101, 81), (101, 81), (101, 81)],
    // v12
    &[(116, 92), (116, 92), (117, 93), (117, 93)],
    // v13
    &[(133, 107), (133, 107), (133, 107), (133, 107)],
    // v14
    &[(145, 115), (145, 115), (145, 115), (146, 116)],
    // v15
    &[(109, 87), (109, 87), (109, 87), (109, 87), (109, 87), (110, 88)],
    // v16
    &[(122, 98), (122, 98), (122, 98), (122, 98), (122, 98), (123, 99)],
    // v17
    &[(135, 107), (136, 108), (136, 108), (136, 108), (136, 108), (136, 108)],
    // v18
    &[(150, 120), (150, 120), (150, 120), (150, 120), (150, 120), (151, 121)],
    // v19
    &[(141, 113), (141, 113), (141, 113), (142, 114), (142, 114), (142, 114), (142, 114)],
    // v20
    &[(135, 107), (135, 107), (135, 107), (136, 108), (136, 108), (136, 108), (136, 108), (136, 108)],
    // v21
    &[(144, 116), (144, 116), (144, 116), (144, 116), (145, 117), (145, 117), (145, 117), (145, 117)],
    // v22
    &[(139, 111), (139, 111), (140, 112), (140, 112), (140, 112), (140, 112), (140, 112), (140, 112), (140, 112)],
    // v23
    &[(151, 121), (151, 121), (151, 121), (151, 121), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122)],
    // v24
    &[(147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (148, 118), (148, 118), (148, 118), (148, 118)],
    // v25
    &[(132, 106), (132, 106), (132, 106), (132, 106), (132, 106), (132, 106), (132, 106), (132, 106), (133, 107), (133, 107), (133, 107), (133, 107)],
    // v26
    &[(142, 114), (142, 114), (142, 114), (142, 114), (142, 114), (142, 114), (142, 114), (142, 114), (142, 114), (142, 114), (143, 115), (143, 115)],
    // v27
    &[(152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (153, 123), (153, 123), (153, 123), (153, 123)],
    // v28
    &[(147, 117), (147, 117), (147, 117), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118)],
    // v29
    &[(146, 116), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117)],
    // v30
    &[(145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116)],
    // v31
    &[(145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (146, 116), (146, 116), (146, 116)],
    // v32
    &[(145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115)],
    // v33
    &[(145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (146, 116)],
    // v34
    &[(145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (145, 115), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116), (146, 116)],
    // v35
    &[(151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122)],
    // v36
    &[(151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (151, 121), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122)],
    // v37
    &[(152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (152, 122), (153, 123), (153, 123), (153, 123), (153, 123)],
    // v38
    &[(152, 122), (152, 122), (152, 122), (152, 122), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123), (153, 123)],
    // v39
    &[(147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (147, 117), (148, 118), (148, 118), (148, 118), (148, 118)],
    // v40
    &[(148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (148, 118), (149, 119), (149, 119), (149, 119), (149, 119), (149, 119), (149, 119)],
];

const ALIGNMENT_PATTERN_POSITIONS: &[&[usize]] = &[
    &[],                          // v1
    &[6, 18],                     // v2
    &[6, 22],                     // v3
    &[6, 26],                     // v4
    &[6, 30],                     // v5
    &[6, 34],                     // v6
    &[6, 22, 38],                 // v7
    &[6, 24, 42],                 // v8
    &[6, 26, 46],                 // v9
    &[6, 28, 50],                 // v10
    &[6, 30, 54],                 // v11
    &[6, 32, 58],                 // v12
    &[6, 34, 62],                 // v13
    &[6, 26, 46, 66],             // v14
    &[6, 26, 48, 70],             // v15
    &[6, 26, 50, 74],             // v16
    &[6, 30, 54, 78],             // v17
    &[6, 30, 56, 82],             // v18
    &[6, 30, 58, 86],             // v19
    &[6, 34, 62, 90],             // v20
    &[6, 28, 50, 72, 94],         // v21
    &[6, 26, 50, 74, 98],         // v22
    &[6, 30, 54, 78, 102],        // v23
    &[6, 28, 54, 80, 106],        // v24
    &[6, 32, 58, 84, 110],        // v25
    &[6, 30, 58, 86, 114],        // v26
    &[6, 34, 62, 90, 118],        // v27
    &[6, 26, 50, 74, 98, 122],    // v28
    &[6, 30, 54, 78, 102, 126],   // v29
    &[6, 26, 52, 78, 104, 130],   // v30
    &[6, 30, 56, 82, 108, 134],   // v31
    &[6, 34, 60, 86, 112, 138],   // v32
    &[6, 30, 58, 86, 114, 142],   // v33
    &[6, 34, 62, 90, 118, 146],   // v34
    &[6, 30, 54, 78, 102, 126, 150], // v35
    &[6, 24, 50, 76, 102, 128, 154], // v36
    &[6, 28, 54, 80, 106, 132, 158], // v37
    &[6, 32, 58, 84, 110, 136, 162], // v38
    &[6, 26, 54, 82, 110, 138, 166], // v39
    &[6, 30, 58, 86, 114, 142, 170], // v40
];

// Precomputed Format Information bits for ECC-L (mask 0..7)
const FORMAT_INFO_L: [u16; 8] = [
    0x77c4, 0x72f3, 0x7daa, 0x789d, 0x662f, 0x6318, 0x6c41, 0x6976,
];

// Precomputed Version Information bits for versions 7..=40
const VERSION_INFO: [u32; 34] = [
    0x07c94, 0x085bc, 0x09a99, 0x0a4d3, 0x0bbf6, 0x0c762, 0x0d847, 0x0e60d, 0x0f928, 0x10b78,
    0x1145d, 0x12a17, 0x13532, 0x149a6, 0x15683, 0x168c9, 0x177ec, 0x18ec4, 0x191e1, 0x1afab,
    0x1b08e, 0x1cc1a, 0x1d33f, 0x1ed75, 0x1f250, 0x209d5, 0x216f0, 0x228ba, 0x2379f, 0x24b0b,
    0x2542e, 0x26a64, 0x27541, 0x28c69,
];

// GF(256) tables with primitive polynomial 0x11D
struct GF256 {
    exp: [u8; 512],
    log: [u8; 256],
}

impl GF256 {
    const fn init() -> Self {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        let mut x = 1u16;
        let mut i = 0usize;
        while i < 255 {
            exp[i] = x as u8;
            exp[i + 255] = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if x >= 256 {
                x ^= 0x11d;
            }
            i += 1;
        }
        exp[510] = exp[0];
        exp[511] = exp[1];
        Self { exp, log }
    }

    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            let idx = (self.log[a as usize] as usize) + (self.log[b as usize] as usize);
            self.exp[idx]
        }
    }
}

static GF: GF256 = GF256::init();

fn rs_generator_poly(ec_count: usize) -> Vec<u8> {
    let mut poly = vec![1u8];
    for i in 0..ec_count {
        let root = GF.exp[i];
        let mut next = vec![0u8; poly.len() + 1];
        for j in 0..poly.len() {
            next[j] ^= GF.mul(poly[j], root);
            next[j + 1] ^= poly[j];
        }
        poly = next;
    }
    poly
}

fn rs_remainder(data: &[u8], ec_count: usize) -> Vec<u8> {
    let gen = rs_generator_poly(ec_count);
    let mut rem = vec![0u8; ec_count];
    for &d in data {
        let factor = d ^ rem[0];
        rem.copy_within(1..ec_count, 0);
        rem[ec_count - 1] = 0;
        for i in 0..ec_count {
            rem[i] ^= GF.mul(gen[ec_count - 1 - i], factor);
        }
    }
    rem
}

fn select_version(data_len: usize) -> Result<usize, MarketError> {
    if data_len > MAX_QR_SHARE_BYTES {
        return Err(MarketError::Validation(format!(
            "Data length {} exceeds MAX_QR_SHARE_BYTES ({})",
            data_len, MAX_QR_SHARE_BYTES
        )));
    }

    for v in 1..=40 {
        let blocks = RS_BLOCKS_L[v - 1];
        let total_data_codewords: usize = blocks.iter().map(|b| b.1).sum();
        let char_count_bits = if v <= 9 { 8 } else { 16 };
        let header_bits = 4 + char_count_bits;
        let max_data_bytes = (total_data_codewords * 8 - header_bits) / 8;
        if data_len <= max_data_bytes {
            return Ok(v);
        }
    }

    Err(MarketError::Validation(format!(
        "Data length {} exceeds maximum QR capacity of 2900 bytes",
        data_len
    )))
}

struct BitWriter {
    bits: Vec<bool>,
}

impl BitWriter {
    fn new() -> Self {
        Self { bits: Vec::new() }
    }

    fn push_bits(&mut self, val: u32, count: usize) {
        for i in (0..count).rev() {
            self.bits.push(((val >> i) & 1) == 1);
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.bits.len().div_ceil(8));
        for chunk in self.bits.chunks(8) {
            let mut b = 0u8;
            for (idx, &bit) in chunk.iter().enumerate() {
                if bit {
                    b |= 1 << (7 - idx);
                }
            }
            bytes.push(b);
        }
        bytes
    }
}

#[allow(clippy::needless_range_loop, clippy::manual_range_contains)]
pub fn encode_qr_svg(data: &[u8]) -> Result<String, MarketError> {
    let version = select_version(data.len())?;
    let blocks = RS_BLOCKS_L[version - 1];
    let total_data_codewords: usize = blocks.iter().map(|b| b.1).sum();
    let char_count_bits = if version <= 9 { 8 } else { 16 };

    // 1. Bitstream encoding: Byte mode
    let mut bw = BitWriter::new();
    bw.push_bits(0b0100, 4); // Byte mode indicator
    bw.push_bits(data.len() as u32, char_count_bits);
    for &byte in data {
        bw.push_bits(byte as u32, 8);
    }

    let target_bits = total_data_codewords * 8;
    // Terminator: up to 4 zero bits
    let term_zeros = (target_bits - bw.bits.len()).min(4);
    for _ in 0..term_zeros {
        bw.bits.push(false);
    }
    // Align to byte boundary
    while !bw.bits.len().is_multiple_of(8) {
        bw.bits.push(false);
    }

    // Alternating padding bytes 0xEC, 0x11
    let pad_bytes = [0xEC, 0x11];
    let mut pad_idx = 0;
    while bw.bits.len() < target_bits {
        bw.push_bits(pad_bytes[pad_idx % 2] as u32, 8);
        pad_idx += 1;
    }

    let encoded_data = bw.to_bytes();

    // 2. Block partition and Reed-Solomon computation
    let mut dc_blocks = Vec::with_capacity(blocks.len());
    let mut ec_blocks = Vec::with_capacity(blocks.len());
    let mut offset = 0;

    for &block in blocks {
        let dc_count = block.1;
        let ec_count = block.0 - dc_count;
        let dc = &encoded_data[offset..offset + dc_count];
        offset += dc_count;
        let ec = rs_remainder(dc, ec_count);
        dc_blocks.push(dc);
        ec_blocks.push(ec);
    }

    // 3. Interleaving
    let max_dc = blocks.iter().map(|b| b.1).max().unwrap_or(0);
    let max_ec = blocks.iter().map(|b| b.0 - b.1).max().unwrap_or(0);
    let mut final_data = Vec::with_capacity(blocks.iter().map(|b| b.0).sum());

    for i in 0..max_dc {
        for dc in &dc_blocks {
            if i < dc.len() {
                final_data.push(dc[i]);
            }
        }
    }
    for i in 0..max_ec {
        for ec in &ec_blocks {
            if i < ec.len() {
                final_data.push(ec[i]);
            }
        }
    }

    // 4. Matrix initialization
    let dim = 4 * version + 17;
    // None = unassigned, Some(bool) = assigned (dark/light)
    let mut reserved = vec![vec![None; dim]; dim];

    // Helper: place probe pattern (finder pattern) + separator
    let mut place_finder = |start_r: usize, start_c: usize| {
        for r in 0..7 {
            for c in 0..7 {
                let is_dark = r == 0 || r == 6 || c == 0 || c == 6 || (2 <= r && r <= 4 && 2 <= c && c <= 4);
                reserved[start_r + r][start_c + c] = Some(is_dark);
            }
        }
        // Separators (white border around finder)
        for r in 0..8 {
            for c in 0..8 {
                let mr = start_r as isize + if start_r == 0 { r as isize } else { r as isize - 1 };
                let mc = start_c as isize + if start_c == 0 { c as isize } else { c as isize - 1 };
                if mr >= 0 && mr < dim as isize && mc >= 0 && mc < dim as isize {
                    let (ur, uc) = (mr as usize, mc as usize);
                    if reserved[ur][uc].is_none() {
                        reserved[ur][uc] = Some(false);
                    }
                }
            }
        }
    };

    place_finder(0, 0);
    place_finder(0, dim - 7);
    place_finder(dim - 7, 0);

    // Timing patterns
    for r in 8..(dim - 8) {
        if reserved[r][6].is_none() {
            reserved[r][6] = Some(r % 2 == 0);
        }
    }
    for c in 8..(dim - 8) {
        if reserved[6][c].is_none() {
            reserved[6][c] = Some(c % 2 == 0);
        }
    }

    // Alignment patterns
    let align_pos = ALIGNMENT_PATTERN_POSITIONS[version - 1];
    for &r in align_pos {
        for &c in align_pos {
            if reserved[r][c].is_some() {
                continue;
            }
            for dr in -2..=2 {
                for dc in -2..=2 {
                    let ur = (r as isize + dr) as usize;
                    let uc = (c as isize + dc) as usize;
                    let is_dark = dr == -2 || dr == 2 || dc == -2 || dc == 2 || (dr == 0 && dc == 0);
                    reserved[ur][uc] = Some(is_dark);
                }
            }
        }
    }

    // Dark module at (4V + 9, 8)
    reserved[4 * version + 9][8] = Some(true);

    // Reserve format information areas (will be filled during mask evaluation)
    for i in 0..9 {
        if reserved[i][8].is_none() {
            reserved[i][8] = Some(false);
        }
        if reserved[8][i].is_none() {
            reserved[8][i] = Some(false);
        }
    }
    for i in 0..8 {
        if reserved[dim - 1 - i][8].is_none() {
            reserved[dim - 1 - i][8] = Some(false);
        }
        if reserved[8][dim - 1 - i].is_none() {
            reserved[8][dim - 1 - i] = Some(false);
        }
    }

    // Reserve version information areas (for versions >= 7)
    if version >= 7 {
        for r in 0..6 {
            for c in 0..3 {
                reserved[r][dim - 11 + c] = Some(false);
                reserved[dim - 11 + c][r] = Some(false);
            }
        }
    }

    // 5. Evaluate all 8 masks and pick the one with lowest penalty
    let mask_functions: [fn(usize, usize) -> bool; 8] = [
        |r, c| (r + c) % 2 == 0,
        |r, _| r % 2 == 0,
        |_, c| c % 3 == 0,
        |r, c| (r + c) % 3 == 0,
        |r, c| (r / 2 + c / 3) % 2 == 0,
        |r, c| (r * c) % 2 + (r * c) % 3 == 0,
        |r, c| ((r * c) % 2 + (r * c) % 3) % 2 == 0,
        |r, c| ((r + c) % 2 + (r * c) % 3) % 2 == 0,
    ];

    let mut best_mask = 0;
    let mut min_penalty = usize::MAX;
    let mut best_matrix = vec![vec![false; dim]; dim];

    for mask_idx in 0..8 {
        let mask_fn = mask_functions[mask_idx];
        let mut mat = vec![vec![false; dim]; dim];

        // Copy template
        for r in 0..dim {
            for c in 0..dim {
                if let Some(b) = reserved[r][c] {
                    mat[r][c] = b;
                }
            }
        }

        // Map data
        let mut bit_idx = 7;
        let mut byte_idx = 0;
        let mut row = dim as isize - 1;
        let mut inc = -1isize;

        let mut col = dim as isize - 1;
        while col > 0 {
            if col == 6 {
                col -= 1;
            }
            let c_pair = [col as usize, (col - 1) as usize];
            loop {
                for &c in &c_pair {
                    if reserved[row as usize][c].is_none() {
                        let mut dark = false;
                        if byte_idx < final_data.len() {
                            dark = ((final_data[byte_idx] >> bit_idx) & 1) == 1;
                        }
                        if mask_fn(row as usize, c) {
                            dark = !dark;
                        }
                        mat[row as usize][c] = dark;
                        if bit_idx == 0 {
                            byte_idx += 1;
                            bit_idx = 7;
                        } else {
                            bit_idx -= 1;
                        }
                    }
                }
                row += inc;
                if row < 0 || row >= dim as isize {
                    row -= inc;
                    inc = -inc;
                    break;
                }
            }
            col -= 2;
        }

        // Apply format information bits
        let format_bits = FORMAT_INFO_L[mask_idx];
        // Vertical format bits
        for i in 0..15 {
            let mod_val = ((format_bits >> i) & 1) == 1;
            if i < 6 {
                mat[i][8] = mod_val;
            } else if i < 8 {
                mat[i + 1][8] = mod_val;
            } else {
                mat[dim - 15 + i][8] = mod_val;
            }
        }
        // Horizontal format bits
        for i in 0..15 {
            let mod_val = ((format_bits >> i) & 1) == 1;
            if i < 8 {
                mat[8][dim - 1 - i] = mod_val;
            } else if i < 9 {
                mat[8][15 - i] = mod_val;
            } else {
                mat[8][14 - i] = mod_val;
            }
        }

        // Apply version information bits
        if version >= 7 {
            let vbits = VERSION_INFO[version - 7];
            for i in 0..18 {
                let mod_val = ((vbits >> i) & 1) == 1;
                mat[i / 3][dim - 11 + (i % 3)] = mod_val;
                mat[dim - 11 + (i % 3)][i / 3] = mod_val;
            }
        }

        // Calculate penalty score (N1 + N2 + N3 + N4)
        let penalty = qr_penalty(&mat, dim);
        if penalty < min_penalty {
            min_penalty = penalty;
            best_mask = mask_idx;
            best_matrix = mat;
        }
    }

    let _ = best_mask;

    // 6. Render SVG
    let quiet_zone = 4;
    let total_dim = dim + 2 * quiet_zone;
    let mut path = String::new();

    for r in 0..dim {
        for c in 0..dim {
            if best_matrix[r][c] {
                path.push_str(&format!("M{},{}h1v1h-1z ", c + quiet_zone, r + quiet_zone));
            }
        }
    }

    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" shape-rendering="crispEdges"><rect width="100%" height="100%" fill="#ffffff"/><path fill="#000000" d="{}"/></svg>"##,
        total_dim,
        total_dim,
        path.trim_end()
    );

    Ok(svg)
}

#[allow(clippy::needless_range_loop)]
fn qr_penalty(mat: &[Vec<bool>], dim: usize) -> usize {
    let mut penalty = 0;

    // N1: 5 or more consecutive modules of same color
    for r in 0..dim {
        let mut count = 0;
        let mut last = false;
        for c in 0..dim {
            let cur = mat[r][c];
            if c == 0 || cur != last {
                if count >= 5 {
                    penalty += 3 + (count - 5);
                }
                count = 1;
                last = cur;
            } else {
                count += 1;
            }
        }
        if count >= 5 {
            penalty += 3 + (count - 5);
        }
    }

    for c in 0..dim {
        let mut count = 0;
        let mut last = false;
        for r in 0..dim {
            let cur = mat[r][c];
            if r == 0 || cur != last {
                if count >= 5 {
                    penalty += 3 + (count - 5);
                }
                count = 1;
                last = cur;
            } else {
                count += 1;
            }
        }
        if count >= 5 {
            penalty += 3 + (count - 5);
        }
    }

    // N2: 2x2 blocks of same color
    for r in 0..(dim - 1) {
        for c in 0..(dim - 1) {
            let color = mat[r][c];
            if color == mat[r][c + 1] && color == mat[r + 1][c] && color == mat[r + 1][c + 1] {
                penalty += 3;
            }
        }
    }

    // N3: 1:1:3:1:1 patterns preceded or followed by 4 light modules
    for r in 0..dim {
        for c in 0..(dim.saturating_sub(10)) {
            // Pattern 1: 1 0 1 1 1 0 1 0 0 0 0
            if mat[r][c] && !mat[r][c + 1] && mat[r][c + 2] && mat[r][c + 3] && mat[r][c + 4]
                && !mat[r][c + 5] && mat[r][c + 6] && !mat[r][c + 7] && !mat[r][c + 8] && !mat[r][c + 9] && !mat[r][c + 10]
            {
                penalty += 40;
            }
            // Pattern 2: 0 0 0 0 1 0 1 1 1 0 1
            if !mat[r][c] && !mat[r][c + 1] && !mat[r][c + 2] && !mat[r][c + 3] && mat[r][c + 4]
                && !mat[r][c + 5] && mat[r][c + 6] && mat[r][c + 7] && mat[r][c + 8] && !mat[r][c + 9] && mat[r][c + 10]
            {
                penalty += 40;
            }
        }
    }

    for c in 0..dim {
        for r in 0..(dim.saturating_sub(10)) {
            if mat[r][c] && !mat[r + 1][c] && mat[r + 2][c] && mat[r + 3][c] && mat[r + 4][c]
                && !mat[r + 5][c] && mat[r + 6][c] && !mat[r + 7][c] && !mat[r + 8][c] && !mat[r + 9][c] && !mat[r + 10][c]
            {
                penalty += 40;
            }
            if !mat[r][c] && !mat[r + 1][c] && !mat[r + 2][c] && !mat[r + 3][c] && mat[r + 4][c]
                && !mat[r + 5][c] && mat[r + 6][c] && mat[r + 7][c] && mat[r + 8][c] && !mat[r + 9][c] && mat[r + 10][c]
            {
                penalty += 40;
            }
        }
    }

    // N4: Ratio of dark modules
    let dark_count = mat.iter().map(|row| row.iter().filter(|&&b| b).count()).sum::<usize>();
    let total = dim * dim;
    let percent = (dark_count * 100) / total;
    let diff = (percent as isize - 50).unsigned_abs();
    penalty += (diff / 5) * 10;

    penalty
}
