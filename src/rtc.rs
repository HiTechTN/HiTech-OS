use spin::Mutex;
use x86_64::instructions::port::Port;

pub const RTC_INDEX: u16 = 0x70;
pub const RTC_DATA: u16 = 0x71;

pub const RTC_SECOND: u8 = 0x00;
pub const RTC_MINUTE: u8 = 0x02;
pub const RTC_HOUR: u8 = 0x04;
pub const RTC_DAY: u8 = 0x07;
pub const RTC_MONTH: u8 = 0x08;
pub const RTC_YEAR: u8 = 0x09;
pub const RTC_CENTURY: u8 = 0x32;
pub const RTC_REG_A: u8 = 0x0a;
pub const RTC_REG_B: u8 = 0x0b;
pub const RTC_REG_C: u8 = 0x0c;

pub const RTC_UPDATE: u8 = 0x80;
pub const RTC_24H: u8 = 0x02;
pub const RTC_BINARY: u8 = 0x04;

#[derive(Copy, Clone, Debug)]
pub struct DateTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u16,
    pub weekday: u8,
}

impl DateTime {
    pub fn now() -> Self {
        let mut dt = DateTime::default();
        dt.update();
        dt
    }

    pub fn update(&mut self) {
        unsafe { Port::<u8>::new(RTC_INDEX).write(RTC_REG_B) };
        let reg_b = unsafe { Port::<u8>::new(RTC_DATA).read() };
        
        let update_in_progress = || -> bool {
            unsafe {
                Port::<u8>::new(RTC_INDEX).write(RTC_REG_A);
                (Port::<u8>::new(RTC_DATA).read() & RTC_UPDATE) != 0
            }
        };
        
        while update_in_progress() {}
        
        self.second = self.read_register(RTC_SECOND);
        self.minute = self.read_register(RTC_MINUTE);
        self.hour = self.read_register(RTC_HOUR);
        self.day = self.read_register(RTC_DAY);
        self.month = self.read_register(RTC_MONTH);
        self.year = self.read_register(RTC_YEAR) as u16;
        self.weekday = self.read_register(0x06);
        
        let is_24h = (reg_b & RTC_24H) != 0;
        let is_binary = (reg_b & RTC_BINARY) != 0;
        
        if !is_binary {
            self.second = bcd_to_bin(self.second);
            self.minute = bcd_to_bin(self.minute);
            self.hour = bcd_to_bin(self.hour & 0x3f);
            self.day = bcd_to_bin(self.day);
            self.month = bcd_to_bin(self.month);
            self.year = bcd_to_bin(self.year as u8) as u16;
        }
        
        if !is_24h && (self.hour & 0x20) != 0 {
            self.hour = (self.hour & 0x1f) + 12;
        }
        
        self.year += if self.year < 80 { 2000 } else { 1900 };
    }

    fn read_register(&self, reg: u8) -> u8 {
        unsafe {
            Port::<u8>::new(RTC_INDEX).write(reg);
            Port::<u8>::new(RTC_DATA).read()
        }
    }

    pub fn format(&self) -> alloc::string::String {
        alloc::string::String::from(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        ))
    }
}

impl Default for DateTime {
    fn default() -> Self {
        DateTime {
            second: 0,
            minute: 0,
            hour: 0,
            day: 1,
            month: 1,
            year: 2000,
            weekday: 1,
        }
    }
}

fn bcd_to_bin(bcd: u8) -> u8 {
    (bcd & 0x0f) + 10 * ((bcd >> 4) & 0x0f) 
}

pub static RTC_TIME: Mutex<DateTime> = Mutex::new(DateTime {
    second: 0,
    minute: 0,
    hour: 0,
    day: 1,
    month: 1,
    year: 2000,
    weekday: 1,
});

pub fn init_rtc() {
    println!("RTC: detection...");
    
    let dt = DateTime::now();
    println!("  Date: {}", dt.format());
}

pub fn get_time() -> DateTime {
    *RTC_TIME.lock()
}

pub fn set_time(dt: &DateTime) {
    let mut time = RTC_TIME.lock();
    time.second = dt.second;
    time.minute = dt.minute;
    time.hour = dt.hour;
    time.day = dt.day;
    time.month = dt.month;
    time.year = dt.year;
    time.weekday = dt.weekday;
}