use spin::Mutex;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Uuid {
    pub bytes: [u8; 16],
}

impl Uuid {
    pub const fn nil() -> Self {
        Uuid { bytes: [0; 16] }
    }

    pub fn new_v4() -> Self {
        let mut bytes = [0u8; 16];
        for byte in &mut bytes {
            *byte = random::simple_u8();
        }
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid { bytes }
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Uuid { bytes }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.len() < 36 {
            return None;
        }
        let bytes = s.as_bytes();
        let mut uuid = [0u8; 16];
        let mut j = 0;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'-' {
                i += 1;
                continue;
            }
            let hi = hex_val(bytes[i])?;
            let lo = hex_val(bytes[i + 1])?;
            uuid[j] = (hi << 4) | lo;
            j += 1;
            i += 2;
        }
        if j != 16 {
            return None;
        }
        Some(Uuid { bytes: uuid })
    }

    pub fn to_string(&self) -> alloc::string::String {
        let mut s = alloc::string::String::with_capacity(36);
        for i in 0..16 {
            s.push(HEX[(self.bytes[i] >> 4) as usize] as char);
            s.push(HEX[(self.bytes[i] & 0x0f) as usize] as char);
            if i == 3 || i == 5 || i == 7 || i == 9 {
                s.push('-');
            }
        }
        s
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub static SYSTEM_UUID: Mutex<Uuid> = Mutex::new(Uuid::nil());

pub fn init_uuid() {
    let uuid = Uuid::new_v4();
    *SYSTEM_UUID.lock() = uuid;
    println!("UUID: {}", uuid.to_string());
}

#[allow(unused_macros)]
macro_rules! uuid {
    ($s:expr) => {
        Uuid::parse($s).unwrap()
    };
}

pub mod random {
    use spin::Mutex;

    static RANDOM_STATE: Mutex<u64> = Mutex::new(0xdeadbeefcafebabe);

    fn xorshift64(state: &mut u64) -> u64 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    pub fn simple_u8() -> u8 {
        let mut state = RANDOM_STATE.lock();
        let _ = xorshift64(&mut state);
        let val = *state;
        (val & 0xff) as u8
    }

    pub fn simple_u16() -> u16 {
        let mut state = RANDOM_STATE.lock();
        let _ = xorshift64(&mut state);
        (*state & 0xffff) as u16
    }

    pub fn simple_u32() -> u32 {
        let mut state = RANDOM_STATE.lock();
        let _ = xorshift64(&mut state);
        (*state & 0xffffffff) as u32
    }

    pub fn simple_u64() -> u64 {
        let mut state = RANDOM_STATE.lock();
        xorshift64(&mut state)
    }

    pub fn fill(buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let val = simple_u64();
            let len = chunk.len();
            chunk.copy_from_slice(&val.to_le_bytes()[..len]);
        }
    }

    pub fn seed(value: u64) {
        *RANDOM_STATE.lock() = value;
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl Guid {
    pub const fn from_values(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Self {
        Guid { data1, data2, data3, data4 }
    }

    pub fn to_uuid(&self) -> Uuid {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&self.data1.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.data2.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.data3.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.data4);
        Uuid { bytes }
    }

    pub fn from_uuid(uuid: &Uuid) -> Self {
        let data1 = u32::from_le_bytes(uuid.bytes[0..4].try_into().unwrap());
        let data2 = u16::from_le_bytes(uuid.bytes[4..6].try_into().unwrap());
        let data3 = u16::from_le_bytes(uuid.bytes[6..8].try_into().unwrap());
        let mut data4 = [0u8; 8];
        data4.copy_from_slice(&uuid.bytes[8..16]);
        Guid { data1, data2, data3, data4 }
    }
}