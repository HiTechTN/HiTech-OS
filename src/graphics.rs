use x86_64::instructions::port::Port;

pub const VBE_MODE_VESA: u16 = 0x4f02;
pub const VBE_GET_MODE: u16 = 0x4f01;
pub const VBE_GET_INFO: u16 = 0x4f00;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum GraphicsError {
    NotSupported,
    ModeNotFound,
    VesaError,
}

#[derive(Copy, Clone, Debug)]
pub struct VbeModeInfo {
    pub mode_attributes: u16,
    pub win_a_attributes: u8,
    pub win_b_attributes: u8,
    pub win_granularity: u16,
    pub win_size: u16,
    pub win_a_start: u16,
    pub win_b_start: u16,
    pub win_func: u32,
    pub bytes_per_scanline: u16,
    pub x_resolution: u16,
    pub y_resolution: u16,
    pub x_char_size: u8,
    pub y_char_size: u8,
    pub memory_planes: u8,
    pub bits_per_pixel: u8,
    pub memory_model: u8,
    pub bank_size: u8,
    pub image_pages: u8,
    pub reserved: u8,
    pub red_mask_size: u8,
    pub red_field_position: u8,
    pub green_mask_size: u8,
    pub green_field_position: u8,
    pub blue_mask_size: u8,
    pub blue_field_position: u8,
    pub rsvd_mask_size: u8,
    pub rsvd_field_position: u8,
    pub direct_color_mode_info: u8,
    pub phys_base: u32,
    pub offscreen_mem_offset: u32,
    pub offscreen_mem_size: u16,
}

impl VbeModeInfo {
    pub fn new() -> Self {
        VbeModeInfo {
            mode_attributes: 0,
            win_a_attributes: 0,
            win_b_attributes: 0,
            win_granularity: 0,
            win_size: 0,
            win_a_start: 0,
            win_b_start: 0,
            win_func: 0,
            bytes_per_scanline: 0,
            x_resolution: 0,
            y_resolution: 0,
            x_char_size: 0,
            y_char_size: 0,
            memory_planes: 0,
            bits_per_pixel: 0,
            memory_model: 0,
            bank_size: 0,
            image_pages: 0,
            reserved: 0,
            red_mask_size: 0,
            red_field_position: 0,
            green_mask_size: 0,
            green_field_position: 0,
            blue_mask_size: 0,
            blue_field_position: 0,
            rsvd_mask_size: 0,
            rsvd_field_position: 0,
            direct_color_mode_info: 0,
            phys_base: 0,
            offscreen_mem_offset: 0,
            offscreen_mem_size: 0,
        }
    }

    pub fn is_graphic_mode(&self) -> bool {
        (self.mode_attributes & 0x01) != 0
    }

    pub fn is_linear(&self) -> bool {
        (self.mode_attributes & 0x40) != 0
    }
}

pub struct Framebuffer {
    pub address: *mut u8,
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub pitch: u32,
    pub present: bool,
}

impl Framebuffer {
    pub const fn new() -> Self {
        Framebuffer {
            address: core::ptr::null_mut(),
            width: 0,
            height: 0,
            bpp: 0,
            pitch: 0,
            present: false,
        }
    }

    pub fn init(&mut self, _mode: u16) -> Result<(), GraphicsError> {
        let mut mode_info = VbeModeInfo::new();
        
        unsafe {
            let _ptr = &mut mode_info as *mut VbeModeInfo as *mut u8;
            Port::<u16>::new(0x10).write(VBE_GET_INFO);
        }
        
        if mode_info.mode_attributes & 0x01 == 0 {
            return Err(GraphicsError::NotSupported);
        }
        
        self.width = mode_info.x_resolution as u32;
        self.height = mode_info.y_resolution as u32;
        self.bpp = mode_info.bits_per_pixel as u32;
        self.pitch = mode_info.bytes_per_scanline as u32;
        
        if mode_info.is_linear() {
            self.address = mode_info.phys_base as *mut u8;
        }
        
        self.present = true;
        Ok(())
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: u32) {
        if !self.present || x >= self.width || y >= self.height {
            return;
        }
        
        let offset = (y * self.pitch + x * (self.bpp / 8)) as isize;
        
        match self.bpp {
            32 => {
                unsafe {
                    let ptr = self.address.offset(offset) as *mut u32;
                    ptr.write_volatile(color);
                }
            }
            24 => {
                unsafe {
                    let ptr = self.address.offset(offset);
                    ptr.write_volatile((color & 0xff) as u8);
                    ptr.offset(1).write_volatile(((color >> 8) & 0xff) as u8);
                    ptr.offset(2).write_volatile(((color >> 16) & 0xff) as u8);
                }
            }
            16 => {
                unsafe {
                    let ptr = self.address.offset(offset) as *mut u16;
                    ptr.write_volatile((color & 0xffff) as u16);
                }
            }
            8 => {
                unsafe {
                    let ptr = self.address.offset(offset);
                    ptr.write_volatile(color as u8);
                }
            }
            _ => {}
        }
    }

    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        for row in y..y.saturating_add(h) {
            for col in x..x.saturating_add(w) {
                self.set_pixel(col, row, color);
            }
        }
    }

    pub fn clear(&mut self, color: u32) {
        self.fill_rect(0, 0, self.width, self.height, color);
    }

    pub fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        
        let mut x = x0;
        let mut y = y0;
        
        loop {
            self.set_pixel(x as u32, y as u32, color);
            
            if x == x1 && y == y1 {
                break;
            }
            
            let e2 = 2 * err;
            
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn draw_circle(&mut self, cx: i32, cy: i32, r: u32, color: u32) {
        let mut x = r as i32;
        let mut y = 0;
        let mut err = 0;
        
        while x >= y {
            self.set_pixel((cx + x) as u32, (cy + y) as u32, color);
            self.set_pixel((cx - x) as u32, (cy + y) as u32, color);
            self.set_pixel((cx + x) as u32, (cy - y) as u32, color);
            self.set_pixel((cx - x) as u32, (cy - y) as u32, color);
            self.set_pixel((cx + y) as u32, (cy + x) as u32, color);
            self.set_pixel((cx - y) as u32, (cy + x) as u32, color);
            self.set_pixel((cx + y) as u32, (cy - x) as u32, color);
            self.set_pixel((cx - y) as u32, (cy - x) as u32, color);
            
            y += 1;
            err += 1 + 2 * y;
            if 2 * (x - y) + err > r as i32 {
                x -= 1;
                err += 2 * (x - y);
            }
        }
    }
}

pub static mut FRAMEBUFFER: Framebuffer = Framebuffer::new();

pub fn init_graphics() -> Result<(), GraphicsError> {
    println!("Initialisation mode graphique VESA...");
    
    unsafe {
        let mode = 0x118;
        
        if FRAMEBUFFER.init(mode).is_ok() {
            println!("  Resolution: {}x{} @ {}bpp", 
                FRAMEBUFFER.width, FRAMEBUFFER.height, FRAMEBUFFER.bpp);
            println!("  Framebuffer: 0x{:p}", FRAMEBUFFER.address);
        }
    }
    
    Ok(())
}