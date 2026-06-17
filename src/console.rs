use spin::Mutex;

pub const TTY_BUF_SIZE: usize = 4096;
pub const MAX_TTYS: usize = 8;
pub const TAB_WIDTH: usize = 8;

#[derive(Copy, Clone, PartialEq)]
pub struct TermSize {
    pub rows: usize,
    pub cols: usize,
}

impl TermSize {
    pub const fn default() -> Self {
        TermSize { rows: 25, cols: 80 }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum TermState {
    Normal,
    Escape,
    Csi,
    Osc,
}

#[derive(Copy, Clone)]
pub struct AnsiState {
    pub state: TermState,
    pub params: [i32; 16],
    pub param_count: usize,
    pub current_param: i32,
    pub question_mark: bool,
    pub buf: [u8; 64],
    pub buf_pos: usize,
}

impl AnsiState {
    pub const fn new() -> Self {
        AnsiState {
            state: TermState::Normal,
            params: [0; 16],
            param_count: 0,
            current_param: 0,
            question_mark: false,
            buf: [0; 64],
            buf_pos: 0,
        }
    }

    pub fn reset(&mut self) {
        self.state = TermState::Normal;
        self.param_count = 0;
        self.current_param = 0;
        self.question_mark = false;
        self.buf_pos = 0;
    }

    pub fn push_param(&mut self) {
        if self.param_count < 16 {
            self.params[self.param_count] = self.current_param;
            self.param_count += 1;
        }
        self.current_param = 0;
    }

    pub fn get_param(&self, idx: usize, default: i32) -> i32 {
        if idx < self.param_count { self.params[idx] } else { default }
    }
}

#[derive(Copy, Clone)]
pub struct Tty {
    pub input_buf: [u8; TTY_BUF_SIZE],
    pub input_head: usize,
    pub input_tail: usize,
    pub input_count: usize,
    pub output_buf: [u8; TTY_BUF_SIZE],
    pub output_head: usize,
    pub output_tail: usize,
    pub output_count: usize,
    pub echo: bool,
    pub cooked: bool,
    pub canonical: bool,
    pub ansi: AnsiState,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub size: TermSize,
    pub id: u8,
    pub foreground_pid: u32,
    pub open: bool,
}

impl Tty {
    pub fn new(id: u8) -> Self {
        Tty {
            input_buf: [0; TTY_BUF_SIZE],
            input_head: 0,
            input_tail: 0,
            input_count: 0,
            output_buf: [0; TTY_BUF_SIZE],
            output_head: 0,
            output_tail: 0,
            output_count: 0,
            echo: true,
            cooked: true,
            canonical: true,
            ansi: AnsiState::new(),
            cursor_x: 0,
            cursor_y: 0,
            size: TermSize::default(),
            id,
            foreground_pid: 0,
            open: true,
        }
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        let mut written = 0;
        for &byte in data {
            if self.output_count >= TTY_BUF_SIZE {
                break;
            }
            self.output_buf[self.output_head] = byte;
            self.output_head = (self.output_head + 1) % TTY_BUF_SIZE;
            self.output_count += 1;
            written += 1;
        }
        if written > 0 {
            self.flush_output();
        }
        written
    }

    pub fn read(&mut self, data: &mut [u8]) -> usize {
        let mut read = 0;
        for byte in data.iter_mut() {
            if self.input_count == 0 {
                break;
            }
            *byte = self.input_buf[self.input_tail];
            self.input_tail = (self.input_tail + 1) % TTY_BUF_SIZE;
            self.input_count -= 1;
            read += 1;
        }
        read
    }

    pub fn put_char(&mut self, c: u8) {
        if self.input_count >= TTY_BUF_SIZE {
            return;
        }

        if self.canonical && c == b'\n' {
            self.input_buf[self.input_head] = c;
            self.input_head = (self.input_head + 1) % TTY_BUF_SIZE;
            self.input_count += 1;

            if self.echo {
                let _ = self.write(&[c]);
            }
            return;
        }

        self.input_buf[self.input_head] = c;
        self.input_head = (self.input_head + 1) % TTY_BUF_SIZE;
        self.input_count += 1;

        if self.echo {
            let _ = self.write(&[c]);
        }
    }

    fn flush_output(&mut self) {
        while self.output_count > 0 {
            let c = self.output_buf[self.output_tail];
            self.output_tail = (self.output_tail + 1) % TTY_BUF_SIZE;
            self.output_count -= 1;

            self.process_ansi(c);
        }
    }

    fn process_ansi(&mut self, c: u8) {
        match self.ansi.state {
            TermState::Normal => {
                if c == 0x1b {
                    self.ansi.state = TermState::Escape;
                } else if c == b'\n' {
                    self.cursor_x = 0;
                    self.cursor_y += 1;
                    if self.cursor_y >= self.size.rows {
                        self.cursor_y = self.size.rows - 1;
                    }
                } else if c == b'\r' {
                    self.cursor_x = 0;
                } else if c == b'\t' {
                    self.cursor_x = (self.cursor_x + TAB_WIDTH) / TAB_WIDTH * TAB_WIDTH;
                    if self.cursor_x >= self.size.cols {
                        self.cursor_x = 0;
                        self.cursor_y += 1;
                    }
                } else {
                    self.cursor_x += 1;
                    if self.cursor_x >= self.size.cols {
                        self.cursor_x = 0;
                        self.cursor_y += 1;
                        if self.cursor_y >= self.size.rows {
                            self.cursor_y = self.size.rows - 1;
                        }
                    }
                }
            }
            TermState::Escape => {
                match c {
                    b'[' => { self.ansi.state = TermState::Csi; }
                    b']' => { self.ansi.state = TermState::Osc; }
                    b'c' => { self.cursor_x = 0; self.cursor_y = 0; self.ansi.reset(); }
                    b'D' => { if self.cursor_x > 0 { self.cursor_x -= 1; } self.ansi.reset(); }
                    b'M' => { if self.cursor_y > 0 { self.cursor_y -= 1; } self.ansi.reset(); }
                    b'E' => { self.cursor_x = 0; self.cursor_y += 1; self.ansi.reset(); }
                    _ => { self.ansi.reset(); }
                }
            }
            TermState::Csi => {
                match c {
                    b'0'..=b'9' => {
                        self.ansi.current_param = self.ansi.current_param * 10 + (c - b'0') as i32;
                    }
                    b';' => {
                        self.ansi.push_param();
                    }
                    b'?' => {
                        self.ansi.question_mark = true;
                    }
                    b'A' => {
                        let n = self.ansi.get_param(0, 1);
                        self.cursor_y = self.cursor_y.saturating_sub(n as usize);
                        self.ansi.reset();
                    }
                    b'B' => {
                        let n = self.ansi.get_param(0, 1);
                        self.cursor_y = (self.cursor_y + n as usize).min(self.size.rows - 1);
                        self.ansi.reset();
                    }
                    b'C' => {
                        let n = self.ansi.get_param(0, 1);
                        self.cursor_x = (self.cursor_x + n as usize).min(self.size.cols - 1);
                        self.ansi.reset();
                    }
                    b'D' => {
                        let n = self.ansi.get_param(0, 1);
                        self.cursor_x = self.cursor_x.saturating_sub(n as usize);
                        self.ansi.reset();
                    }
                    b'H' | b'f' => {
                        let row = self.ansi.get_param(0, 1) as usize - 1;
                        let col = self.ansi.get_param(1, 1) as usize - 1;
                        self.cursor_y = row.min(self.size.rows - 1);
                        self.cursor_x = col.min(self.size.cols - 1);
                        self.ansi.reset();
                    }
                    b'J' => {
                        self.ansi.reset();
                    }
                    b'K' => {
                        self.ansi.reset();
                    }
                    b'm' => {
                        self.ansi.reset();
                    }
                    b's' => {
                        self.ansi.reset();
                    }
                    b'u' => {
                        self.ansi.reset();
                    }
                    _ => {
                        self.ansi.reset();
                    }
                }
            }
            TermState::Osc => {
                if c == b'\x07' || c == b'\x1b' {
                    self.ansi.reset();
                } else if self.ansi.buf_pos < 64 {
                    self.ansi.buf[self.ansi.buf_pos] = c;
                    self.ansi.buf_pos += 1;
                }
            }
        }
    }
}

pub struct ConsoleManager {
    pub ttys: [Option<Tty>; MAX_TTYS],
    pub active: usize,
    pub count: usize,
}

impl ConsoleManager {
    pub const fn new() -> Self {
        ConsoleManager { ttys: [None; MAX_TTYS], active: 0, count: 0 }
    }

    pub fn init(&mut self) {
        for i in 0..MAX_TTYS {
            if i == 0 {
                self.ttys[i] = Some(Tty::new(i as u8));
                self.count += 1;
            }
        }
        if self.count > 0 {
            self.active = 0;
        }
    }

    pub fn get_active(&mut self) -> Option<&mut Tty> {
        self.ttys[self.active].as_mut()
    }

    pub fn switch_tty(&mut self, num: usize) -> bool {
        if num < MAX_TTYS && self.ttys[num].is_some() {
            self.active = num;
            true
        } else {
            false
        }
    }

    pub fn allocate_tty(&mut self) -> Option<u8> {
        for i in 0..MAX_TTYS {
            if self.ttys[i].is_none() {
                self.ttys[i] = Some(Tty::new(i as u8));
                self.count += 1;
                return Some(i as u8);
            }
        }
        None
    }

    pub fn write_all(&mut self, data: &[u8]) -> usize {
        let mut total = 0;
        for tty in self.ttys.iter_mut().flatten() {
            total += tty.write(data);
        }
        total
    }

    pub fn broadcast_signal(&mut self, sig: i32) {
        for tty in self.ttys.iter_mut().flatten() {
            if tty.foreground_pid > 0 {
                let _ = crate::signal::SIGNAL_MANAGER.lock().send_signal(sig);
            }
        }
    }
}

pub static CONSOLE: Mutex<ConsoleManager> = Mutex::new(ConsoleManager::new());

pub fn init_console() {
    println!("Console: terminal virtuel");
    CONSOLE.lock().init();
}

pub mod line_discipline {
    use super::*;

    pub const VEOF: u8 = 0x04;
    pub const VEOL: u8 = 0x00;
    pub const VERASE: u8 = 0x7f;
    pub const VINTR: u8 = 0x03;
    pub const VKILL: u8 = 0x15;
    pub const VQUIT: u8 = 0x1c;
    pub const VSUSP: u8 = 0x1a;
    pub const VSTART: u8 = 0x11;
    pub const VSTOP: u8 = 0x13;

    pub struct LineDiscipline {
        pub buf: [u8; 256],
        pub pos: usize,
        pub echo: bool,
    }

    impl LineDiscipline {
        pub const fn new() -> Self {
            LineDiscipline { buf: [0; 256], pos: 0, echo: true }
        }

        pub fn process(&mut self, c: u8, tty: &mut Tty) -> bool {
            match c {
                b'\n' | b'\r' => {
                    self.buf[self.pos] = b'\n';
                    self.pos = 0;
                    if self.echo {
                        let _ = tty.write(&[b'\n']);
                    }
                    true
                }
                VERASE => {
                    if self.pos > 0 {
                        self.pos -= 1;
                        if self.echo {
                            let _ = tty.write(&[0x08, b' ', 0x08]);
                        }
                    }
                    false
                }
                VKILL => {
                    if self.echo {
                        for _ in 0..self.pos {
                            let _ = tty.write(&[0x08, b' ', 0x08]);
                        }
                    }
                    self.pos = 0;
                    false
                }
                VINTR | VQUIT => {
                    self.pos = 0;
                    let sig = if c == VINTR { crate::signal::SIGINT } else { crate::signal::SIGQUIT };
                    let _ = crate::signal::SIGNAL_MANAGER.lock().send_signal(sig);
                    false
                }
                VSUSP => {
                    self.pos = 0;
                    let _ = crate::signal::SIGNAL_MANAGER.lock().send_signal(crate::signal::SIGTSTP);
                    false
                }
                0x20..=0x7e => {
                    if self.pos < 256 {
                        self.buf[self.pos] = c;
                        self.pos += 1;
                        if self.echo {
                            let _ = tty.write(&[c]);
                        }
                    }
                    false
                }
                _ => false,
            }
        }

        pub fn flush(&mut self) {
            self.pos = 0;
        }
    }
}