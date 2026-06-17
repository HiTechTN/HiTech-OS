use x86_64::instructions::port::Port;

pub const AC97_NAMBAR: u16 = 0x100;
pub const AC97_NABM_BASE: u16 = 0x200;

pub const AC97_RESET: u16 = 0x00;
pub const AC97_MASTER_VOL: u16 = 0x02;
pub const AC97_AUX_OUT_VOL: u16 = 0x04;
pub const AC97_MONO_VOL: u16 = 0x06;
pub const AC97_PCM_OUT_VOL: u16 = 0x18;
pub const AC97_POWER: u16 = 0x26;
pub const AC97_EXT_AUDIO_ID: u16 = 0x28;
pub const AC97_EXT_AUDIO_CTRL: u16 = 0x2a;
pub const AC97_PCM_FRONT_DAC: u16 = 0x1e;

pub const PCM_OUTPUT_RATE: u16 = 0x2c;

pub const AC97_CODEC_ID_MASTER: u16 = 0x00;
pub const AC97_CODEC_ID_SLAVE: u16 = 0x01;

#[derive(Copy, Clone)]
pub struct Ac97Controller {
    pub iobase: u16,
    pub nmiobase: u16,
    pub present: bool,
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
}

impl Ac97Controller {
    pub const fn new() -> Self {
        Ac97Controller {
            iobase: AC97_NAMBAR,
            nmiobase: AC97_NABM_BASE,
            present: false,
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 16,
        }
    }

    pub fn init(&mut self, iobase: u16, nmiobase: u16) -> bool {
        self.iobase = iobase;
        self.nmiobase = nmiobase;

        unsafe {
            let mut codec_id = Port::<u16>::new(iobase + AC97_RESET);
            let id = codec_id.read();

            if id == 0xffff || id == 0x0000 {
                return false;
            }

            codec_id.write(0);
        }

        self.present = true;
        self.detect_audio();
        true
    }

    fn detect_audio(&mut self) {
        unsafe {
            let mut ext_id = Port::<u16>::new(self.iobase + AC97_EXT_AUDIO_ID);
            let features = ext_id.read();

            if features & 0x0001 != 0 {
                self.sample_rate = 48000;
            }

            if features & 0x0400 != 0 {
                self.channels = 6;
            } else if features & 0x0200 != 0 {
                self.channels = 4;
            } else {
                self.channels = 2;
            }
        }
    }

    pub fn set_volume(&self, volume: u8) {
        let vol = (volume as u16) * 0x1010 / 100;
        unsafe {
            Port::<u16>::new(self.iobase + AC97_MASTER_VOL).write(vol);
        }
    }

    pub fn set_pcm_volume(&self, volume: u8) {
        let vol = (volume as u16) * 0x1010 / 100;
        unsafe {
            Port::<u16>::new(self.iobase + AC97_PCM_OUT_VOL).write(vol);
        }
    }

    pub fn set_sample_rate(&self, rate: u32) {
        unsafe {
            Port::<u16>::new(self.iobase + PCM_OUTPUT_RATE).write(rate as u16);
        }
    }

    pub fn reset(&self) {
        unsafe {
            Port::<u16>::new(self.iobase + AC97_RESET).write(0);
        }
    }

    pub fn power_down(&self) {
        unsafe {
            Port::<u16>::new(self.iobase + AC97_POWER).write(0x0f0f);
        }
    }

    pub fn power_up(&self) {
        unsafe {
            Port::<u16>::new(self.iobase + AC97_POWER).write(0x0000);
        }
    }
}

pub static mut AC97: Ac97Controller = Ac97Controller::new();

pub fn init_ac97() -> bool {
    println!("AC97: recherche...");
    unsafe {
        if AC97.init(AC97_NAMBAR, AC97_NABM_BASE) {
            println!("  AC97 codec detecte");
            println!("  Sample rate: {} Hz", AC97.sample_rate);
            println!("  Canaux: {}", AC97.channels);
            true
        } else {
            println!("  AC97 non trouve");
            false
        }
    }
}

pub mod hdaudio {
    

    pub const HDAUDIO_GCAP: u16 = 0x00;
    pub const HDAUDIO_MINOR: u16 = 0x02;
    pub const HDAUDIO_MAJOR: u16 = 0x03;
    pub const HDAUDIO_OUTPAY: u16 = 0x04;
    pub const HDAUDIO_INPAY: u16 = 0x06;
    pub const HDAUDIO_GCTL: u16 = 0x08;
    pub const HDAUDIO_WAKEEN: u16 = 0x0c;
    pub const HDAUDIO_STATESTS: u16 = 0x0e;
    pub const HDAUDIO_GSTS: u16 = 0x10;
    pub const HDAUDIO_OUTSTRMPAY: u16 = 0x18;
    pub const HDAUDIO_INSTRMPAY: u16 = 0x1a;

    pub struct HdAudioController {
        pub mmio: u32,
        pub present: bool,
    }

    impl HdAudioController {
        pub const fn new() -> Self {
            HdAudioController { mmio: 0, present: false }
        }

        pub fn init(&mut self, mmio: u32) -> bool {
            self.mmio = mmio;
            self.present = true;
            true
        }
    }
}