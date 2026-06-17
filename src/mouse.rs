use x86_64::instructions::port::Port;

pub const MOUSE_DATA_PORT: u16 = 0x60;
pub const MOUSE_STATUS_PORT: u16 = 0x64;
pub const MOUSE_COMMAND_PORT: u16 = 0x64;

pub const PS2_CMD_DISABLE_MOUSE: u8 = 0xa7;
pub const PS2_CMD_ENABLE_MOUSE: u8 = 0xa8;
pub const PS2_CMD_READ_ID: u8 = 0xf2;
pub const PS2_CMD_SET_SAMPLE_RATE: u8 = 0xf3;
pub const PS2_CMD_ENABLE_PACKETS: u8 = 0xf4;
pub const PS2_CMD_RESET: u8 = 0xff;

pub const MOUSE_IRQ: u8 = 12;

pub const MOUSE_ACK: u8 = 0xfa;
pub const MOUSE_NACK: u8 = 0xfe;
pub const MOUSE_ERROR: u8 = 0xfc;
pub const MOUSE_ID: u8 = 0x00;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MouseError {
    NotFound,
    Timeout,
    SelfTestFailed,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct MousePacket {
    pub buttons: MouseButtons,
    pub movement_x: i8,
    pub movement_y: i8,
    pub wheel: i8,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

pub struct Ps2Mouse {
    pub present: bool,
    pub packet: MousePacket,
    pub data_ready: bool,
    pub x: u32,
    pub y: u32,
    pub screen_width: u32,
    pub screen_height: u32,
}

impl Ps2Mouse {
    pub const fn new() -> Self {
        Ps2Mouse {
            present: false,
            packet: MousePacket {
                buttons: MouseButtons {
                    left: false,
                    right: false,
                    middle: false,
                },
                movement_x: 0,
                movement_y: 0,
                wheel: 0,
            },
            data_ready: false,
            x: 0,
            y: 0,
            screen_width: 800,
            screen_height: 600,
        }
    }

    pub fn init(&mut self) -> Result<(), MouseError> {
        unsafe {
            let mut cmd_port = Port::<u8>::new(MOUSE_COMMAND_PORT);
            let mut data_port = Port::<u8>::new(MOUSE_DATA_PORT);
            
            cmd_port.write(0xa8);
            cmd_port.write(0x20);
            let status = data_port.read();
            
            if status & 0x20 == 0 {
                return Err(MouseError::NotFound);
            }
            
            cmd_port.write(0xd4);
            data_port.write(PS2_CMD_RESET);
            
            for _ in 0..1000 {
                let b = data_port.read();
                if b == MOUSE_ACK {
                    break;
                }
            }
            
            data_port.read();
            
            cmd_port.write(0xd4);
            data_port.write(PS2_CMD_READ_ID);
            
            for _ in 0..1000 {
                let b = data_port.read();
                if b != 0 {
                    if b != MOUSE_ID {
                        self.present = true;
                        return Ok(());
                    }
                }
            }
        }
        
        Err(MouseError::NotFound)
    }

    pub fn enable(&mut self) -> Result<(), MouseError> {
        unsafe {
            let mut cmd_port = Port::<u8>::new(MOUSE_COMMAND_PORT);
            let mut data_port = Port::<u8>::new(MOUSE_DATA_PORT);
            
            cmd_port.write(0xd4);
            data_port.write(PS2_CMD_SET_SAMPLE_RATE);
            data_port.read();
            
            cmd_port.write(0xd4);
            data_port.write(200);
            data_port.read();
            
            cmd_port.write(0xd4);
            data_port.write(PS2_CMD_ENABLE_PACKETS);
            data_port.read();
        }
        
        Ok(())
    }

    pub fn read_packet(&mut self) -> Option<MousePacket> {
        unsafe {
            let mut status_port = Port::<u8>::new(MOUSE_STATUS_PORT);
            let mut data_port = Port::<u8>::new(MOUSE_DATA_PORT);
            
            let status = status_port.read();
            if status & 0x21 != 0x21 {
                return None;
            }
            
            let b0 = data_port.read();
            if b0 & 0x08 == 0 {
                return None;
            }
            
            let b1 = data_port.read();
            let b2 = data_port.read();
            let b3 = data_port.read();
            
            let packet = MousePacket {
                buttons: MouseButtons {
                    left: (b0 & 0x01) != 0,
                    right: (b0 & 0x02) != 0,
                    middle: (b0 & 0x04) != 0,
                },
                movement_x: b1 as i8,
                movement_y: b2 as i8,
                wheel: b3 as i8,
            };
            
            self.packet = packet;
            self.data_ready = true;
            
            self.x = self.x.saturating_add(packet.movement_x as u32);
            self.y = self.y.saturating_sub(packet.movement_y as u32);
            
            Some(packet)
        }
    }

    pub fn get_position(&self) -> (u32, u32) {
        (self.x, self.y)
    }

    pub fn is_button_pressed(&self, button: MouseButton) -> bool {
        match button {
            MouseButton::Left => self.packet.buttons.left,
            MouseButton::Right => self.packet.buttons.right,
            MouseButton::Middle => self.packet.buttons.middle,
        }
    }
}

#[derive(Copy, Clone)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

pub static mut MOUSE: Ps2Mouse = Ps2Mouse::new();

pub fn init_mouse() -> Result<(), MouseError> {
    println!("Initialisation souris PS/2...");
    
    unsafe {
        if MOUSE.init().is_ok() {
            MOUSE.enable()?;
            println!("  Souris detectee et activee");
        }
    }
    
    Ok(())
}