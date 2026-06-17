use alloc::vec::Vec;
use alloc::vec;

pub const AES_BLOCK_SIZE: usize = 16;
pub const AES_128_KEY_SIZE: usize = 16;
pub const AES_192_KEY_SIZE: usize = 24;
pub const AES_256_KEY_SIZE: usize = 32;

pub const AES_MODE_ECB: u8 = 0;
pub const AES_MODE_CBC: u8 = 1;
pub const AES_MODE_CTR: u8 = 2;
pub const AES_MODE_GCM: u8 = 3;

pub const RSA_KEY_SIZE_1024: usize = 128;
pub const RSA_KEY_SIZE_2048: usize = 256;
pub const RSA_KEY_SIZE_4096: usize = 512;

pub const RSA_PUBLIC_EXPONENT: u32 = 65537;

static SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

static RCON: [u8; 11] = [0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36];

pub struct Aes128 {
    round_keys: [u8; 176],
    nk: usize,
    nr: usize,
}

impl Aes128 {
    pub fn new(key: &[u8]) -> Self {
        let nk = match key.len() {
            16 => 4,
            24 => 6,
            32 => 8,
            _ => 4,
        };
        let nr = nk + 6;
        let mut cipher = Aes128 {
            round_keys: [0; 176],
            nk,
            nr,
        };
        cipher.key_expansion(key);
        cipher
    }

    fn key_expansion(&mut self, key: &[u8]) {
        let nb = 4;
        let word_count = nb * (self.nr + 1);

        for i in 0..self.nk {
            let offset = i * 4;
            self.round_keys[offset] = key[offset];
            self.round_keys[offset + 1] = key[offset + 1];
            self.round_keys[offset + 2] = key[offset + 2];
            self.round_keys[offset + 3] = key[offset + 3];
        }

        for i in self.nk..word_count {
            let mut temp = [
                self.round_keys[(i - 1) * 4],
                self.round_keys[(i - 1) * 4 + 1],
                self.round_keys[(i - 1) * 4 + 2],
                self.round_keys[(i - 1) * 4 + 3],
            ];

            if i % self.nk == 0 {
                temp = Self::rot_word(temp);
                temp = Self::sub_word(temp);
                temp[0] ^= RCON[i / self.nk];
            } else if self.nk > 6 && i % self.nk == 4 {
                temp = Self::sub_word(temp);
            }

            let offset = i * 4;
            let prev_offset = (i - self.nk) * 4;
            self.round_keys[offset] = self.round_keys[prev_offset] ^ temp[0];
            self.round_keys[offset + 1] = self.round_keys[prev_offset + 1] ^ temp[1];
            self.round_keys[offset + 2] = self.round_keys[prev_offset + 2] ^ temp[2];
            self.round_keys[offset + 3] = self.round_keys[prev_offset + 3] ^ temp[3];
        }
    }

    fn rot_word(word: [u8; 4]) -> [u8; 4] {
        [word[1], word[2], word[3], word[0]]
    }

    fn sub_word(word: [u8; 4]) -> [u8; 4] {
        [SBOX[word[0] as usize], SBOX[word[1] as usize], SBOX[word[2] as usize], SBOX[word[3] as usize]]
    }

    fn gf_mul(a: u8, b: u8) -> u8 {
        let mut p = 0u8;
        let mut a = a;
        let mut b = b;

        for _ in 0..8 {
            if b & 1 != 0 {
                p ^= a;
            }
            let high_bit = a & 0x80;
            a <<= 1;
            if high_bit != 0 {
                a ^= 0x1b;
            }
            b >>= 1;
        }
        p
    }

    pub fn encrypt_block(&self, block: &mut [u8]) {
        let _nb = 4;
        let mut state = [0u8; 16];
        state.copy_from_slice(block);

        self.add_round_key(&mut state, 0);

        for round in 1..self.nr {
            self.sub_bytes(&mut state);
            self.shift_rows(&mut state);
            self.mix_columns(&mut state);
            self.add_round_key(&mut state, round);
        }

        self.sub_bytes(&mut state);
        self.shift_rows(&mut state);
        self.add_round_key(&mut state, self.nr);

        block.copy_from_slice(&state);
    }

    pub fn decrypt_block(&self, block: &mut [u8]) {
        let _nb = 4;
        let mut state = [0u8; 16];
        state.copy_from_slice(block);

        self.add_round_key(&mut state, self.nr);

        for round in (1..self.nr).rev() {
            self.inv_shift_rows(&mut state);
            self.inv_sub_bytes(&mut state);
            self.add_round_key(&mut state, round);
            self.inv_mix_columns(&mut state);
        }

        self.inv_shift_rows(&mut state);
        self.inv_sub_bytes(&mut state);
        self.add_round_key(&mut state, 0);

        block.copy_from_slice(&state);
    }

    fn sub_bytes(&self, state: &mut [u8; 16]) {
        for i in 0..16 {
            state[i] = SBOX[state[i] as usize];
        }
    }

    fn inv_sub_bytes(&self, state: &mut [u8; 16]) {
        let inv_sbox = [
            0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
            0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
            0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
            0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
            0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
            0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
            0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
            0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
            0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
            0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
            0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
            0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
            0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
            0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
            0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
            0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
        ];

        for i in 0..16 {
            state[i] = inv_sbox[state[i] as usize];
        }
    }

    fn shift_rows(&self, state: &mut [u8; 16]) {
        let s = *state;
        state[0] = s[0]; state[1] = s[5]; state[2] = s[10]; state[3] = s[15];
        state[4] = s[4]; state[5] = s[9]; state[6] = s[14]; state[7] = s[3];
        state[8] = s[8]; state[9] = s[13]; state[10] = s[2]; state[11] = s[7];
        state[12] = s[12]; state[13] = s[1]; state[14] = s[6]; state[15] = s[11];
    }

    fn inv_shift_rows(&self, state: &mut [u8; 16]) {
        let s = *state;
        state[0] = s[0]; state[1] = s[13]; state[2] = s[10]; state[3] = s[7];
        state[4] = s[4]; state[5] = s[1]; state[6] = s[14]; state[7] = s[11];
        state[8] = s[8]; state[9] = s[5]; state[10] = s[2]; state[11] = s[15];
        state[12] = s[12]; state[13] = s[9]; state[14] = s[6]; state[15] = s[3];
    }

    fn mix_columns(&self, state: &mut [u8; 16]) {
        for c in 0..4 {
            let offset = c * 4;
            let s0 = state[offset];
            let s1 = state[offset + 1];
            let s2 = state[offset + 2];
            let s3 = state[offset + 3];

            state[offset] = Self::gf_mul(2, s0) ^ Self::gf_mul(3, s1) ^ s2 ^ s3;
            state[offset + 1] = s0 ^ Self::gf_mul(2, s1) ^ Self::gf_mul(3, s2) ^ s3;
            state[offset + 2] = s0 ^ s1 ^ Self::gf_mul(2, s2) ^ Self::gf_mul(3, s3);
            state[offset + 3] = Self::gf_mul(3, s0) ^ s1 ^ s2 ^ Self::gf_mul(2, s3);
        }
    }

    fn inv_mix_columns(&self, state: &mut [u8; 16]) {
        for c in 0..4 {
            let offset = c * 4;
            let s0 = state[offset];
            let s1 = state[offset + 1];
            let s2 = state[offset + 2];
            let s3 = state[offset + 3];

            state[offset] = Self::gf_mul(14, s0) ^ Self::gf_mul(11, s1) ^ Self::gf_mul(13, s2) ^ Self::gf_mul(9, s3);
            state[offset + 1] = Self::gf_mul(9, s0) ^ Self::gf_mul(14, s1) ^ Self::gf_mul(11, s2) ^ Self::gf_mul(13, s3);
            state[offset + 2] = Self::gf_mul(13, s0) ^ Self::gf_mul(9, s1) ^ Self::gf_mul(14, s2) ^ Self::gf_mul(11, s3);
            state[offset + 3] = Self::gf_mul(11, s0) ^ Self::gf_mul(13, s1) ^ Self::gf_mul(9, s2) ^ Self::gf_mul(14, s3);
        }
    }

    fn add_round_key(&self, state: &mut [u8; 16], round: usize) {
        for i in 0..16 {
            state[i] ^= self.round_keys[round * 16 + i];
        }
    }

    pub fn encrypt_cbc(&self, data: &[u8], iv: &[u8; 16]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len());
        let mut prev = *iv;

        for chunk in data.chunks(16) {
            let mut block = [0u8; 16];
            let len = chunk.len();
            block[..len].copy_from_slice(chunk);

            if len < 16 {
                block[len] = 0x80;
            }

            for i in 0..16 {
                block[i] ^= prev[i];
            }

            self.encrypt_block(&mut block);
            prev = block;
            result.extend_from_slice(&block);
        }

        result
    }

    pub fn decrypt_cbc(&self, data: &[u8], iv: &[u8; 16]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len());
        let mut prev = *iv;

        for chunk in data.chunks(16) {
            let mut block = [0u8; 16];
            block.copy_from_slice(chunk);
            let original = block;

            self.decrypt_block(&mut block);

            for i in 0..16 {
                block[i] ^= prev[i];
            }
            prev = original;
            result.extend_from_slice(&block);
        }

        result
    }
}

pub struct Sha256 {
    h: [u32; 8],
    buffer: [u8; 64],
    count: u64,
    buf_idx: usize,
}

impl Sha256 {
    pub fn new() -> Self {
        Sha256 {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: [0; 64],
            count: 0,
            buf_idx: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.buffer[self.buf_idx] = byte;
            self.buf_idx += 1;
            self.count += 1;

            if self.buf_idx == 64 {
                self.process_block();
                self.buf_idx = 0;
            }
        }
    }

    pub fn finalize(&mut self) -> [u8; 32] {
        let bits = self.count * 8;

        let padding = 55usize.wrapping_sub(self.buf_idx) % 64 + 1;
        self.buffer[self.buf_idx] = 0x80;

        for i in 1..padding {
            self.buffer[self.buf_idx + i] = 0;
        }

        if self.buf_idx + padding + 8 > 64 {
            self.process_block();
            self.buffer.iter_mut().take(56).for_each(|b| *b = 0);
        }

        for i in 0..8 {
            self.buffer[56 + i] = ((bits >> (56 - i * 8)) & 0xff) as u8;
        }

        self.process_block();

        let mut result = [0u8; 32];
        for i in 0..8 {
            result[i * 4] = (self.h[i] >> 24) as u8;
            result[i * 4 + 1] = ((self.h[i] >> 16) & 0xff) as u8;
            result[i * 4 + 2] = ((self.h[i] >> 8) & 0xff) as u8;
            result[i * 4 + 3] = (self.h[i] & 0xff) as u8;
        }

        result
    }

    fn process_block(&mut self) {
        let k = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
            0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
            0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
            0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
            0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
            0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
            0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
            0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];

        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = (self.buffer[i * 4] as u32) << 24
                | (self.buffer[i * 4 + 1] as u32) << 16
                | (self.buffer[i * 4 + 2] as u32) << 8
                | (self.buffer[i * 4 + 3] as u32);
        }

        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d) = (self.h[0], self.h[1], self.h[2], self.h[3]);
        let (mut e, mut f, mut g, mut h) = (self.h[4], self.h[5], self.h[6], self.h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }

    pub fn hex(result: &[u8; 32]) -> alloc::string::String {
        let hex_chars = b"0123456789abcdef";
        let mut s = alloc::string::String::with_capacity(64);
        for &byte in result {
            s.push(hex_chars[(byte >> 4) as usize] as char);
            s.push(hex_chars[(byte & 0x0f) as usize] as char);
        }
        s
    }
}

pub fn sha256_hex(data: &[u8]) -> alloc::string::String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    Sha256::hex(&hash)
}

pub struct Rsa {
    pub n: Vec<u8>,
    pub e: Vec<u8>,
    pub d: Vec<u8>,
    pub bit_size: usize,
}

impl Rsa {
    pub fn new(n: &[u8], e: &[u8], d: &[u8]) -> Self {
        Rsa {
            n: Vec::from(&n[..]),
            e: Vec::from(&e[..]),
            d: Vec::from(&d[..]),
            bit_size: n.len() * 8,
        }
    }

    pub fn encrypt(&self, data: &[u8]) -> Vec<u8> {
        let block_size = self.bit_size / 8 - 11;
        let mut result = Vec::new();

        for chunk in data.chunks(block_size) {
            let mut block = vec![0u8; self.bit_size / 8];
            block[1] = 0x02;

            let padding_len = self.bit_size / 8 - chunk.len() - 3;
            for i in 0..padding_len {
                block[2 + i] = 0xff;  // simplified padding
            }

            block[2 + padding_len] = 0;
            block[3 + padding_len..].copy_from_slice(chunk);

            result.extend_from_slice(&self.mod_pow(&block, &self.e, &self.n));
        }

        result
    }

    pub fn decrypt(&self, data: &[u8]) -> Vec<u8> {
        let block_size = self.bit_size / 8;
        let mut result = Vec::new();

        for chunk in data.chunks(block_size) {
            let decrypted = self.mod_pow(chunk, &self.d, &self.n);
            if let Some(pos) = decrypted.iter().position(|&x| x == 0) {
                result.extend_from_slice(&decrypted[pos + 1..]);
            }
        }

        result
    }

    fn mod_pow(&self, base: &[u8], exp: &[u8], modulus: &[u8]) -> Vec<u8> {
        let mut result = vec![0u8; modulus.len()];
        result[modulus.len() - 1] = 1;

        let mut b = Vec::from(&base[..]);

        for &bit in exp.iter() {
            for j in (0..8).rev() {
                if (bit >> j) & 1 != 0 {
                    result = self.mod_mul(&result, &b, modulus);
                }
                b = self.mod_mul(&b, &b, modulus);
            }
        }

        result
    }

    fn mod_mul(&self, a: &[u8], b: &[u8], m: &[u8]) -> Vec<u8> {
        let mut result = vec![0u8; m.len()];

        for (_i, &byte) in a.iter().enumerate() {
            for j in (0..8).rev() {
                if (byte >> j) & 1 != 0 {
                    let mut temp = result.clone();
                    for k in 0..b.len() {
                        let idx = temp.len() - b.len() + k;
                        if idx < temp.len() {
                            temp[idx] = temp[idx].wrapping_add(b[k]);
                        }
                    }
                    result = self.mod_reduce(&temp, m);
                }

                let mut temp = result.clone();
                for k in 0..b.len() {
                    let idx = temp.len() - b.len() + k;
                    if idx < temp.len() {
                        temp[idx] = temp[idx].wrapping_add(b[k]);
                    }
                }
                result = self.mod_reduce(&temp, m);
            }
        }

        result
    }

    fn mod_reduce(&self, x: &[u8], m: &[u8]) -> Vec<u8> {
        let mut result = Vec::from(&x[..]);
        while result.len() > m.len() || (result.len() == m.len() && result.as_slice() >= m) {
            let mut borrow = false;
            for i in (0..m.len()).rev() {
                let idx = result.len() - m.len() + i;
                let (val, b) = if borrow {
                    result[idx].overflowing_sub(m[i].wrapping_add(1))
                } else {
                    result[idx].overflowing_sub(m[i])
                };
                result[idx] = val;
                borrow = b;
            }

            while !result.is_empty() && result[0] == 0 {
                result.remove(0);
            }
        }
        result
    }
}

pub fn init_crypto() {
    println!("Crypto: AES, RSA, SHA256, SHA1 supporte");
}