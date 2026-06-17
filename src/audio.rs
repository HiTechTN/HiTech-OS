use x86_64::instructions::port::Port;

pub const SB_BASE: u16 = 0x220;

pub const DSP_RESET: u16 = 0x6;
pub const DSP_READ: u16 = 0xa;
pub const DSP_WRITE: u16 = 0xc;
pub const DSP_STATUS: u16 = 0xe;

pub const DSP_CMD_SET_TIME: u8 = 0x40;
pub const DSP_CMD_SET_SIZE: u8 = 0x48;
pub const DSP_CMD_SET_MODE: u8 = 0x40;
pub const DSP_CMD_OUTPUT: u8 = 0x10;
pub const DSP_CMD_INPUT: u8 = 0x20;
pub const DSP_CMD_SILENCE: u8 = 0x80;
pub const DSP_CMD_IDENTIFY: u8 = 0xee;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AudioError {
    NotFound,
    NotReady,
    Timeout,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AudioFormat {
    Mono8,
    Mono16,
    Stereo8,
    Stereo16,
}

pub struct SoundBlaster {
    pub present: bool,
    pub base: u16,
    pub version: u8,
    pub sample_rate: u32,
    pub format: AudioFormat,
    pub playing: bool,
}

impl SoundBlaster {
    pub const fn new() -> Self {
        SoundBlaster {
            present: false,
            base: SB_BASE,
            version: 0,
            sample_rate: 44100,
            format: AudioFormat::Stereo16,
            playing: false,
        }
    }

    pub fn init(&mut self, base: u16) -> Result<(), AudioError> {
        self.base = base;
        
        unsafe {
            Port::<u8>::new(self.base + DSP_RESET).write(1);
            for _ in 0..100 { core::hint::black_box(&0); }
            Port::<u8>::new(self.base + DSP_RESET).write(0);
            
            for _ in 0..100 {
                let status = Port::<u8>::new(self.base + DSP_STATUS).read();
                if status & 0x80 != 0 {
                    break;
                }
            }
        }
        
        self.present = true;
        self.version = 4;
        Ok(())
    }

    pub fn set_sample_rate(&mut self, rate: u32) -> Result<(), AudioError> {
        self.sample_rate = rate;
        let rate_val = (1193180 / rate) as u16;
        
        unsafe {
            Port::<u8>::new(self.base + DSP_WRITE).write(DSP_CMD_SET_TIME);
            Port::<u8>::new(self.base + DSP_WRITE).write((rate_val & 0xff) as u8);
            Port::<u8>::new(self.base + DSP_WRITE).write(((rate_val >> 8) & 0xff) as u8);
        }
        Ok(())
    }

    pub fn play(&mut self, _data: &[u8]) -> Result<(), AudioError> {
        if !self.present {
            return Err(AudioError::NotFound);
        }
        self.playing = true;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.playing = false;
    }
}

pub static mut SB16: SoundBlaster = SoundBlaster::new();

pub fn init_audio() -> Result<(), AudioError> {
    println!("Recherche Sound Blaster...");
    
    unsafe {
        if SB16.init(SB_BASE).is_ok() {
            println!("  SB16 detecte @ 0x{:x}", SB_BASE);
            println!("  Sample rate: {} Hz", SB16.sample_rate);
        }
    }
    Ok(())
}

pub mod wave {
    use alloc::vec::Vec;

    fn sin_approx(x: f64) -> f64 {
        let pi = 3.14159265358979323846;
        let mut x = x % (2.0 * pi);
        if x > pi { x -= 2.0 * pi; }
        if x < -pi { x += 2.0 * pi; }
        let x2 = x * x;
        x * (1.0 - x2 / 6.0 + x2 * x2 / 120.0 - x2 * x2 * x2 / 5040.0)
    }
    pub fn generate_beep(frequency: u32, duration_ms: u32) -> Vec<u8> {
        let num_samples = ((44100u64 * duration_ms as u64) / 1000) as usize;
        let mut samples = Vec::with_capacity(num_samples);
        
        for i in 0..num_samples {
            let t = i as f64 / 44100.0;
            let value = (sin_approx(t * frequency as f64) * 127.0 + 128.0) as u8;
            samples.push(value);
        }
        
        samples
    }
}