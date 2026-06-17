use spin::Mutex;

pub struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    pub const fn new(seed: u64) -> Self {
        Xorshift64 { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    pub fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    pub fn next_u8(&mut self) -> u8 {
        self.next_u64() as u8
    }
}

pub struct RandomPool {
    pub entropy: [u8; 64],
    pub pool_size: usize,
    pub rng: Xorshift64,
    pub seed_initialized: bool,
}

impl RandomPool {
    pub const fn new() -> Self {
        RandomPool {
            entropy: [0; 64],
            pool_size: 0,
            rng: Xorshift64::new(0xdeadbeefcafebabeu64),
            seed_initialized: false,
        }
    }

    pub fn add_entropy(&mut self, data: &[u8]) {
        for &byte in data {
            if self.pool_size < 64 {
                self.entropy[self.pool_size] ^= byte;
                self.pool_size += 1;
            } else {
                let idx = (byte as usize) % 64;
                self.entropy[idx] ^= byte;
            }
        }

        if self.pool_size >= 32 && !self.seed_initialized {
            let mut seed = 0u64;
            for i in 0..8 {
                seed = (seed << 8) | self.entropy[i] as u64;
            }
            self.rng = Xorshift64::new(seed);
            self.seed_initialized = true;
        }
    }

    pub fn random_u64(&mut self) -> u64 {
        if !self.seed_initialized {
            self.add_entropy(&[0xfe, 0xca, 0xba, 0xbe, 0xde, 0xad, 0xbe, 0xef]);
        }
        self.rng.next_u64()
    }

    pub fn random_u32(&mut self) -> u32 {
        self.random_u64() as u32
    }

    pub fn fill_buffer(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let val = self.random_u64().to_ne_bytes();
            let len = chunk.len().min(8);
            chunk.copy_from_slice(&val[..len]);
        }
    }

    pub fn random_range(&mut self, min: u64, max: u64) -> u64 {
        if min >= max {
            return min;
        }
        let range = max - min;
        let val = self.random_u64();
        min + (val % range)
    }
}

pub static RNG: Mutex<RandomPool> = Mutex::new(RandomPool::new());

pub fn init_rng() {
    println!("RNG: generateur de nombres aleatoires");
    RNG.lock().add_entropy(&[
        0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe,
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
    ]);
}

pub mod hwrng {
    use super::*;
    

    pub const RDRAND_ATTEMPTS: u32 = 10;

    pub fn rdrand() -> Option<u64> {
        for _ in 0..RDRAND_ATTEMPTS {
            unsafe {
                let mut val: u64 = 0;
                let result: u8;
                core::arch::asm!(
                    "rdrand {0}",
                    "setc {1}",
                    out(reg) val,
                    out(reg_byte) result,
                    options(nostack, preserves_flags)
                );
                if result != 0 {
                    return Some(val);
                }
            }
        }
        None
    }

    pub fn rdseed() -> Option<u64> {
        unsafe {
            let mut val: u64 = 0;
            let result: u8;
            core::arch::asm!(
                "rdseed {0}",
                "setc {1}",
                out(reg) val,
                out(reg_byte) result,
                options(nostack, preserves_flags)
            );
            if result != 0 {
                return Some(val);
            }
        }
        None
    }

    pub fn mix_hw_rng() {
        if let Some(val) = rdrand() {
            let bytes = val.to_ne_bytes();
            RNG.lock().add_entropy(&bytes);
        }
        if let Some(val) = rdseed() {
            let bytes = val.to_ne_bytes();
            RNG.lock().add_entropy(&bytes);
        }
    }
}