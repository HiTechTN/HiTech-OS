
pub const E1000_DEVICE_ID_82540EM: u16 = 0x100e;
pub const E1000_VENDOR_ID_INTEL: u16 = 0x8086;

pub const REG_CTRL: u16 = 0x0000;
pub const REG_STATUS: u16 = 0x0008;
pub const REG_EEPROM: u16 = 0x0014;
pub const REG_CTRL_EXT: u16 = 0x0018;
pub const REG_RCTRL: u16 = 0x0100;
pub const REG_TCTRL: u16 = 0x0400;
pub const REG_TDLEN: u16 = 0x0408;
pub const REG_TDBAL: u16 = 0x0404;
pub const REG_TDBAH: u16 = 0x040c;
pub const REG_RDBAL: u16 = 0x2800;
pub const REG_RDBAH: u16 = 0x2804;
pub const REG_RDLEN: u16 = 0x2808;
pub const REG_RDH: u16 = 0x2810;
pub const REG_RDT: u16 = 0x2818;
pub const REG_RA: u16 = 0x5400;
pub const REG_MTA: u16 = 0x5200;

pub const CTRL_FD: u32 = 0x00000002;
pub const CTRL_ASDE: u32 = 0x00000040;
pub const CTRL_SPEED: u32 = 0x00000200;
pub const CTRRL_AUTO_NEG: u32 = 0x00000080;
pub const CTRRL_RST: u32 = 0x00000001;
pub const CTRL_SLU: u32 = 0x00000040;
pub const CTRL_ILOS: u32 = 0x00000080;

pub const RCTL_EN: u32 = 0x00000002;
pub const RCTL_SBP: u32 = 0x00000200;
pub const RCTL_UPE: u32 = 0x00000004;
pub const RCTL_MPE: u32 = 0x00000008;
pub const RCTL_LPE: u32 = 0x00000010;
pub const RCTL_LBM: u32 = 0x00000060;
pub const RCTL_BSIZE: u32 = 0x00030000;
pub const RCTL_BSEX: u32 = 0x00040000;
pub const RCTL_SECRC: u32 = 0x04000000;

pub const TCTL_EN: u32 = 0x00000002;
pub const TCTL_PSP: u32 = 0x00000008;
pub const TCTL_CT: u32 = 0x00000ff0;
pub const TCTL_COLD: u32 = 0x003ff000;

pub const E1000_NUM_RX_DESC: usize = 32;
pub const E1000_NUM_TX_DESC: usize = 8;

const MAX_PACKET_SIZE: usize = 1518;

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct E1000RxDesc {
    pub addr: u64,
    pub length: u16,
    pub checksum: u16,
    pub status: u8,
    pub errors: u8,
    pub special: u16,
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct E1000TxDesc {
    pub addr: u64,
    pub length: u16,
    pub cso: u8,
    pub cmd: u8,
    pub status: u8,
    pub css: u8,
    pub special: u16,
}

pub struct E1000Device {
    pub mmio_base: u32,
    pub iobase: u16,
    pub present: bool,
    pub mac: [u8; 6],
    pub irq: u8,
    pub rx_desc: [E1000RxDesc; E1000_NUM_RX_DESC],
    pub tx_desc: [E1000TxDesc; E1000_NUM_TX_DESC],
    pub rx_cur: usize,
    pub tx_cur: usize,
    pub rx_buffers: [[u8; MAX_PACKET_SIZE]; E1000_NUM_RX_DESC],
    pub tx_buffers: [[u8; MAX_PACKET_SIZE]; E1000_NUM_TX_DESC],
    pub link_up: bool,
}

impl E1000Device {
    pub const fn new() -> Self {
        E1000Device {
            mmio_base: 0,
            iobase: 0,
            present: false,
            mac: [0; 6],
            irq: 0,
            rx_desc: [E1000RxDesc { addr: 0, length: 0, checksum: 0, status: 0, errors: 0, special: 0 }; E1000_NUM_RX_DESC],
            tx_desc: [E1000TxDesc { addr: 0, length: 0, cso: 0, cmd: 0, status: 0, css: 0, special: 0 }; E1000_NUM_TX_DESC],
            rx_cur: 0,
            tx_cur: 0,
            rx_buffers: [[0; MAX_PACKET_SIZE]; E1000_NUM_RX_DESC],
            tx_buffers: [[0; MAX_PACKET_SIZE]; E1000_NUM_TX_DESC],
            link_up: false,
        }
    }

    fn read_reg(&self, reg: u16) -> u32 {
        unsafe { ((self.mmio_base + reg as u32) as *const u32).read_volatile() }
    }

    fn write_reg(&self, reg: u16, val: u32) {
        unsafe { ((self.mmio_base + reg as u32) as *mut u32).write_volatile(val) }
    }

    pub fn init(&mut self, mmio_base: u32, iobase: u16) -> bool {
        self.mmio_base = mmio_base;
        self.iobase = iobase;

        self.write_reg(REG_CTRL, CTRRL_RST);
        for _ in 0..10 {}

        self.write_reg(REG_CTRL, CTRL_SLU | CTRL_SPEED);

        self.read_mac();

        self.write_reg(REG_RCTRL, RCTL_EN | RCTL_SBP | RCTL_UPE | RCTL_MPE | RCTL_LBM);
        self.write_reg(REG_TCTRL, TCTL_EN | TCTL_PSP | (15 << 4) | (64 << 12));

        self.init_rx();
        self.init_tx();

        self.present = true;
        self.link_up = true;
        true
    }

    fn read_mac(&mut self) {
        let mac_low = self.read_reg(REG_RA);
        let mac_high = self.read_reg(REG_RA + 2);

        self.mac[0] = (mac_low & 0xff) as u8;
        self.mac[1] = ((mac_low >> 8) & 0xff) as u8;
        self.mac[2] = ((mac_low >> 16) & 0xff) as u8;
        self.mac[3] = ((mac_low >> 24) & 0xff) as u8;
        self.mac[4] = (mac_high & 0xff) as u8;
        self.mac[5] = ((mac_high >> 8) & 0xff) as u8;
    }

    fn init_rx(&mut self) {
        for i in 0..E1000_NUM_RX_DESC {
            self.rx_desc[i].addr = &self.rx_buffers[i] as *const _ as u64;
            self.rx_desc[i].status = 0;
        }

        let desc_addr = &self.rx_desc as *const _ as u64;
        self.write_reg(REG_RDBAL, (desc_addr & 0xffffffff) as u32);
        self.write_reg(REG_RDBAH, (desc_addr >> 32) as u32);
        self.write_reg(REG_RDLEN, (E1000_NUM_RX_DESC * 16) as u32);
        self.write_reg(REG_RDH, 0);
        self.write_reg(REG_RDT, (E1000_NUM_RX_DESC - 1) as u32);
    }

    fn init_tx(&mut self) {
        for i in 0..E1000_NUM_TX_DESC {
            self.tx_desc[i].addr = &self.tx_buffers[i] as *const _ as u64;
            self.tx_desc[i].cmd = 0;
            self.tx_desc[i].status = 0;
        }

        let desc_addr = &self.tx_desc as *const _ as u64;
        self.write_reg(REG_TDBAL, (desc_addr & 0xffffffff) as u32);
        self.write_reg(REG_TDBAH, (desc_addr >> 32) as u32);
        self.write_reg(REG_TDLEN, (E1000_NUM_TX_DESC * 16) as u32);
    }

    pub fn send_packet(&mut self, data: &[u8]) -> bool {
        if data.len() > MAX_PACKET_SIZE {
            return false;
        }

        let tx_desc = &mut self.tx_desc[self.tx_cur];
        let tx_buf = &mut self.tx_buffers[self.tx_cur];

        tx_buf[..data.len()].copy_from_slice(data);
        tx_desc.length = data.len() as u16;
        tx_desc.status = 0;
        tx_desc.cmd = 0x0c;

        self.tx_cur = (self.tx_cur + 1) % E1000_NUM_TX_DESC;

        self.write_reg(0x3808, self.tx_cur as u32);
        true
    }

    pub fn receive_packet(&mut self, buffer: &mut [u8]) -> Option<usize> {
        let rx_desc = &mut self.rx_desc[self.rx_cur];

        if (rx_desc.status & 0x01) == 0 {
            return None;
        }

        let len = rx_desc.length as usize;
        if len <= buffer.len() {
            buffer[..len].copy_from_slice(&self.rx_buffers[self.rx_cur][..len]);
        }

        rx_desc.status = 0;
        self.rx_cur = (self.rx_cur + 1) % E1000_NUM_RX_DESC;

        self.write_reg(REG_RDT, self.rx_cur as u32);
        Some(len)
    }

    pub fn mac_str(&self) -> alloc::string::String {
        alloc::string::String::from(format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac[0], self.mac[1], self.mac[2], self.mac[3], self.mac[4], self.mac[5]
        ))
    }
}

pub static mut E1000: E1000Device = E1000Device::new();

pub fn init_e1000() -> bool {
    println!("e1000: driver Intel PRO/1000");
    true
}