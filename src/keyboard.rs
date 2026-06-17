static SCANCODES: [u8; 128] = [
    0, 27, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0',
    b'-', b'=', 8, b'\t', b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i',
    b'o', b'p', b'[', b']', b'\n', 0, b'a', b's', b'd',
    b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`', 0,
    b'\\', b'z', b'x', b'c', b'v', b'b', b'n', b'm', b',', b'.', b'/', 0,
    b'*', 0, b' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0,
];

pub fn translate(scancode: u8) -> Option<char> {
    if scancode as usize >= SCANCODES.len() {
        return None;
    }
    let c = SCANCODES[scancode as usize];
    if c == 0 { None } else { Some(c as char) }
}

const BUFFER_SIZE: usize = 128;

static KEYBOARD_BUFFER: spin::Mutex<[u8; BUFFER_SIZE]> = spin::Mutex::new([0; BUFFER_SIZE]);
static BUFFER_HEAD: spin::Mutex<usize> = spin::Mutex::new(0);
static BUFFER_TAIL: spin::Mutex<usize> = spin::Mutex::new(0);

pub fn push_key(scancode: u8) {
    if let Some(c) = translate(scancode) {
        let tail = *BUFFER_TAIL.lock();
        let next = (tail + 1) % BUFFER_SIZE;
        if next != *BUFFER_HEAD.lock() {
            KEYBOARD_BUFFER.lock()[tail] = c as u8;
            *BUFFER_TAIL.lock() = next;
        }
    }
}

pub fn read_key() -> Option<u8> {
    let head = *BUFFER_HEAD.lock();
    if head != *BUFFER_TAIL.lock() {
        let c = KEYBOARD_BUFFER.lock()[head];
        *BUFFER_HEAD.lock() = (head + 1) % BUFFER_SIZE;
        Some(c)
    } else {
        None
    }
}

pub fn has_key() -> bool {
    *BUFFER_HEAD.lock() != *BUFFER_TAIL.lock()
}