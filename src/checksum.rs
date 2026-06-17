
pub trait Checksum {
    fn update(&mut self, data: &[u8]);
    fn result(&self) -> u32;
    fn reset(&mut self);
}

pub struct Crc32 {
    state: u32,
    table: [u32; 256],
}

impl Crc32 {
    pub fn new() -> Self {
        let mut table = [0u32; 256];
        for i in 0..256 {
            let mut crc = i as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xedb88320;
                } else {
                    crc >>= 1;
                }
            }
            table[i] = crc;
        }
        Crc32 { state: 0xffffffff, table }
    }

    pub fn compute(data: &[u8]) -> u32 {
        let mut crc = 0xffffffffu32;
        for &byte in data {
            crc = (crc >> 8) ^ CRC32_TABLE[((crc ^ byte as u32) & 0xff) as usize];
        }
        crc ^ 0xffffffff
    }

    pub fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.state = (self.state >> 8) ^ self.table[((self.state ^ byte as u32) & 0xff) as usize];
        }
    }

    pub fn finalize(&self) -> u32 {
        self.state ^ 0xffffffff
    }

    pub fn reset(&mut self) {
        self.state = 0xffffffff;
    }
}

impl Checksum for Crc32 {
    fn update(&mut self, data: &[u8]) { self.update(data); }
    fn result(&self) -> u32 { self.finalize() }
    fn reset(&mut self) { self.reset(); }
}

const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xedb88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

pub fn crc32(data: &[u8]) -> u32 {
    Crc32::compute(data)
}

#[allow(dead_code)]
pub struct Crc16(u16);

impl Crc16 {
    pub fn compute(data: &[u8]) -> u16 {
        let mut crc = 0xffffu16;
        for &byte in data {
            crc ^= byte as u16;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xa001;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc
    }
}

pub struct Adler32(u32);

impl Adler32 {
    pub fn new() -> Self {
        Adler32(1)
    }

    pub fn compute(data: &[u8]) -> u32 {
        let mut a = 1u32;
        let mut b = 0u32;
        for &byte in data {
            a = (a + byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        (b << 16) | a
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut a = self.0 & 0xffff;
        let mut b = (self.0 >> 16) & 0xffff;
        for &byte in data {
            a = (a + byte as u32) % 65521;
            b = (b + a) % 65521;
        }
        self.0 = (b << 16) | a;
    }

    pub fn finalize(&self) -> u32 {
        self.0
    }
}

pub struct Sha1 {
    h: [u32; 5],
    buffer: [u8; 64],
    count: u64,
    buf_idx: usize,
}

impl Sha1 {
    pub fn new() -> Self {
        Sha1 {
            h: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0],
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

    pub fn finalize(&mut self) -> [u8; 20] {
        let bits = self.count * 8;
        let padding = 55usize.wrapping_sub(self.buf_idx) % 64 + 1;
        self.buffer[self.buf_idx] = 0x80;
        for i in 1..padding {
            self.buffer[self.buf_idx + i] = 0;
        }
        if self.buf_idx + padding + 8 > 64 {
            self.process_block();
            for i in 0..56 {
                self.buffer[i] = 0;
            }
        }
        for i in 0..8 {
            self.buffer[56 + i] = ((bits >> (56 - i * 8)) & 0xff) as u8;
        }
        self.process_block();
        let mut result = [0u8; 20];
        for i in 0..5 {
            result[i * 4] = (self.h[i] >> 24) as u8;
            result[i * 4 + 1] = ((self.h[i] >> 16) & 0xff) as u8;
            result[i * 4 + 2] = ((self.h[i] >> 8) & 0xff) as u8;
            result[i * 4 + 3] = (self.h[i] & 0xff) as u8;
        }
        result
    }

    fn process_block(&mut self) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = (self.buffer[i * 4] as u32) << 24
                | (self.buffer[i * 4 + 1] as u32) << 16
                | (self.buffer[i * 4 + 2] as u32) << 8
                | (self.buffer[i * 4 + 3] as u32);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (self.h[0], self.h[1], self.h[2], self.h[3], self.h[4]);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5a827999),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
    }

    pub fn hex(result: &[u8; 20]) -> alloc::string::String {
        let hex_chars = b"0123456789abcdef";
        let mut s = alloc::string::String::with_capacity(40);
        for &byte in result {
            s.push(hex_chars[(byte >> 4) as usize] as char);
            s.push(hex_chars[(byte & 0x0f) as usize] as char);
        }
        s
    }
}

pub fn sha1_hex(data: &[u8]) -> alloc::string::String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    let hash = hasher.finalize();
    Sha1::hex(&hash)
}

pub fn verify_checksum(data: &[u8], expected: u32) -> bool {
    crc32(data) == expected
}