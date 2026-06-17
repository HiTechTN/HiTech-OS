use spin::Mutex;

pub static TICKS: Mutex<u64> = Mutex::new(0);

const PIT_COMMAND_PORT: u16 = 0x43;
const PIT_DATA_PORT: u16 = 0x40;
const PIT_FREQUENCY: u32 = 100;

pub fn init() {
    let divisor = 1193182 / PIT_FREQUENCY;
    
    unsafe {
        x86_64::instructions::port::Port::<u8>::new(PIT_COMMAND_PORT).write(0x36);
        x86_64::instructions::port::Port::new(PIT_DATA_PORT).write((divisor & 0xff) as u8);
        x86_64::instructions::port::Port::new(PIT_DATA_PORT).write(((divisor >> 8) & 0xff) as u8);
    }
}

pub fn tick() {
    *TICKS.lock() += 1;
}

pub fn uptime_millis() -> u64 {
    *TICKS.lock() * (1000 / PIT_FREQUENCY as u64)
}

pub fn uptime_seconds() -> u64 {
    uptime_millis() / 1000
}

pub fn sleep(ms: u32) {
    let target = *TICKS.lock() + (ms as u64 * (PIT_FREQUENCY as u64 / 1000));
    while *TICKS.lock() < target {
        x86_64::instructions::hlt();
    }
}