use x86_64::instructions::port::Port;
use alloc::vec::Vec;
use alloc::string::String;

pub const RTL8139_VENDOR_ID: u16 = 0x10ec;
pub const RTL8139_DEVICE_ID: u16 = 0x8139;
pub const RTL8139_IO_BASE: u16 = 0x3000;

pub const REG_MAC0: u8 = 0x00;
pub const REG_MAC4: u8 = 0x04;
pub const REG_TX0_START: u8 = 0x20;
pub const REG_CMD: u8 = 0x37;
pub const REG_IMR: u8 = 0x3c;
pub const REG_ISR: u8 = 0x3e;
pub const REG_RX_CONFIG: u8 = 0x44;
pub const REG_TX_CONFIG: u8 = 0x40;

pub const CMD_RX_ENABLE: u8 = 0x08;
pub const CMD_TX_ENABLE: u8 = 0x04;
pub const CMD_RESET: u8 = 0x10;

pub const IMR_ROK: u16 = 0x0001;
pub const IMR_TOK: u16 = 0x0004;
pub const IMR_RER: u16 = 0x0002;
pub const IMR_TER: u16 = 0x0008;

pub const RX_BUFFER_SIZE: usize = 8192;
pub const TX_BUFFER_SIZE: usize = 1792;
pub const NUM_TX_BUFFERS: usize = 4;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum NetworkError {
    NotFound,
    NotReady,
    IoError,
    NoMemory,
}

pub struct Rtl8139 {
    pub present: bool,
    pub io_base: u16,
    pub mac_address: [u8; 6],
    tx_buffer: [[u8; TX_BUFFER_SIZE]; NUM_TX_BUFFERS],
    tx_dirty: [bool; NUM_TX_BUFFERS],
    rx_buffer: [u8; RX_BUFFER_SIZE],
    rx_offset: usize,
}

impl Rtl8139 {
    pub const fn new() -> Self {
        Rtl8139 {
            present: false,
            io_base: RTL8139_IO_BASE,
            mac_address: [0; 6],
            tx_buffer: [[0; TX_BUFFER_SIZE]; NUM_TX_BUFFERS],
            tx_dirty: [false; NUM_TX_BUFFERS],
            rx_buffer: [0; RX_BUFFER_SIZE],
            rx_offset: 0,
        }
    }

    pub fn init(&mut self, io_base: u16) -> Result<(), NetworkError> {
        self.io_base = io_base;
        
        let cmd = unsafe { Port::<u8>::new(self.io_base + REG_CMD as u16).read() };
        
        if cmd == 0xff {
            return Err(NetworkError::NotFound);
        }
        
        self.reset()?;
        
        self.read_mac();
        
        self.present = true;
        Ok(())
    }

    fn reset(&mut self) -> Result<(), NetworkError> {
        unsafe { Port::<u8>::new(self.io_base + REG_CMD as u16).write(CMD_RESET); }
        
        for _ in 0..1000 {
            let cmd = unsafe { Port::<u8>::new(self.io_base + REG_CMD as u16).read() };
            if cmd & CMD_RESET == 0 {
                return Ok(());
            }
        }
        
        Err(NetworkError::NotReady)
    }

    fn read_mac(&mut self) {
        let mac_low = unsafe { Port::<u32>::new(self.io_base).read() };
        self.mac_address[0] = (mac_low & 0xff) as u8;
        self.mac_address[1] = ((mac_low >> 8) & 0xff) as u8;
        self.mac_address[2] = ((mac_low >> 16) & 0xff) as u8;
        self.mac_address[3] = ((mac_low >> 24) & 0xff) as u8;
        
        let mac_high = unsafe { Port::<u32>::new(self.io_base + REG_MAC4 as u16).read() };
        self.mac_address[4] = (mac_high & 0xff) as u8;
        self.mac_address[5] = ((mac_high >> 8) & 0xff) as u8;
    }

    pub fn configure(&mut self) {
        let rx_config = 0x0f | (7 << 11);
        unsafe { Port::<u32>::new(self.io_base + REG_RX_CONFIG as u16).write(rx_config); }
        
        let tx_config = 0x03000100;
        unsafe { Port::<u32>::new(self.io_base + REG_TX_CONFIG as u16).write(tx_config); }
    }

    pub fn enable_rx_tx(&mut self) {
        unsafe { Port::<u8>::new(self.io_base + REG_CMD as u16).write(CMD_RX_ENABLE | CMD_TX_ENABLE); }
    }

    pub fn send_packet(&mut self, data: &[u8]) -> Result<(), NetworkError> {
        if data.len() > TX_BUFFER_SIZE {
            return Err(NetworkError::NoMemory);
        }

        for i in 0..NUM_TX_BUFFERS {
            if !self.tx_dirty[i] {
                self.tx_buffer[i][..data.len()].copy_from_slice(data);
                self.tx_dirty[i] = true;
                
                let offset = REG_TX0_START + (i as u8 * 4);
                let phys_addr = 0x100000 + (i * TX_BUFFER_SIZE);
                unsafe { Port::<u32>::new(self.io_base + offset as u16).write(phys_addr as u32); }
                
                unsafe { Port::<u8>::new(self.io_base + REG_CMD as u16).write(0x10 | (i as u8)); }
                
                return Ok(());
            }
        }
        
        Err(NetworkError::NoMemory)
    }

    pub fn receive_packet(&mut self, buffer: &mut [u8]) -> Option<usize> {
        let rx_read_ptr = self.rx_offset;
        
        if self.rx_buffer[rx_read_ptr] == 0 {
            return None;
        }
        
        let packet = &self.rx_buffer[rx_read_ptr..];
        let len = ((packet[0] as usize) | ((packet[1] as usize) << 8)).saturating_sub(4);
        
        if len > buffer.len() || len > RX_BUFFER_SIZE {
            return None;
        }
        
        buffer[..len].copy_from_slice(&packet[4..len+4]);
        self.rx_offset = (self.rx_offset + len + 4 + 3) & !3;
        
        Some(len)
    }

    pub fn mac_str(&self) -> String {
        format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.mac_address[0], self.mac_address[1], self.mac_address[2],
            self.mac_address[3], self.mac_address[4], self.mac_address[5])
    }
}

pub static mut NIC: Rtl8139 = Rtl8139::new();

pub fn init_network() -> Result<(), NetworkError> {
    println!("Recherche NIC RTL8139...");
    
    unsafe {
        NIC.init(RTL8139_IO_BASE)?;
    }
    
    println!("  MAC: {}", unsafe { NIC.mac_str() });
    println!("  reseau initialise");
    
    Ok(())
}

pub mod ethernet {
    use super::*;

    pub const ETH_TYPE_IPV4: u16 = 0x0800;
    pub const ETH_TYPE_ARP: u16 = 0x0806;
    pub const ETH_TYPE_IPV6: u16 = 0x86dd;

    #[derive(Copy, Clone)]
    pub struct EthernetHeader {
        pub dst: [u8; 6],
        pub src: [u8; 6],
        pub ethertype: u16,
    }

    impl EthernetHeader {
        pub fn new(dst: &[u8; 6], src: &[u8; 6], ethertype: u16) -> Self {
            EthernetHeader {
                dst: *dst,
                src: *src,
                ethertype,
            }
        }

        pub fn parse(data: &[u8]) -> Option<EthernetHeader> {
            if data.len() < 14 {
                return None;
            }
            
            Some(EthernetHeader {
                dst: data[0..6].try_into().ok()?,
                src: data[6..12].try_into().ok()?,
                ethertype: (data[12] as u16) | ((data[13] as u16) << 8),
            })
        }
    }

    pub fn send_ipv4(src: &[u8; 6], dst: &[u8; 6], payload: &[u8]) -> Option<Vec<u8>> {
        let mut packet = Vec::with_capacity(14 + payload.len());
        
        packet.extend_from_slice(dst);
        packet.extend_from_slice(src);
        packet.push((ETH_TYPE_IPV4 & 0xff) as u8);
        packet.push(((ETH_TYPE_IPV4 >> 8) & 0xff) as u8);
        packet.extend_from_slice(payload);
        
        Some(packet)
    }
}

pub mod ipv4 {
    

    pub const IP_VERSION: u8 = 4;
    pub const IP_IHL: u8 = 5;
    pub const IP_TOS: u8 = 0;
    pub const IP_TTL: u8 = 64;
    pub const IP_PROTOCOL_TCP: u8 = 6;
    pub const IP_PROTOCOL_UDP: u8 = 17;
    pub const IP_PROTOCOL_ICMP: u8 = 1;

    #[derive(Copy, Clone)]
    pub struct Ipv4Header {
        pub version_ihl: u8,
        pub tos: u8,
        pub total_length: u16,
        pub identification: u16,
        pub flags_fragment: u16,
        pub ttl: u8,
        pub protocol: u8,
        pub checksum: u16,
        pub src_ip: [u8; 4],
        pub dst_ip: [u8; 4],
    }

    impl Ipv4Header {
        pub fn new(src_ip: [u8; 4], dst_ip: [u8; 4], protocol: u8, payload_len: u16) -> Self {
            Ipv4Header {
                version_ihl: (IP_VERSION << 4) | IP_IHL,
                tos: IP_TOS,
                total_length: 20 + payload_len,
                identification: 0,
                flags_fragment: 0,
                ttl: IP_TTL,
                protocol,
                checksum: 0,
                src_ip,
                dst_ip,
            }
        }

        pub fn calculate_checksum(&self) -> u16 {
            let mut sum: u32 = 0;
            let data = self.as_bytes();
            
            for i in (0..data.len()).step_by(2) {
                let word = (data[i] as u32) | ((data[i+1] as u32) << 8);
                sum += word;
            }
            
            while sum >> 16 != 0 {
                sum = (sum & 0xffff) + (sum >> 16);
            }
            
            !(sum as u16)
        }

        pub fn as_bytes(&self) -> [u8; 20] {
            let mut bytes = [0u8; 20];
            bytes[0] = self.version_ihl;
            bytes[1] = self.tos;
            bytes[2] = (self.total_length & 0xff) as u8;
            bytes[3] = ((self.total_length >> 8) & 0xff) as u8;
            bytes[4] = (self.identification & 0xff) as u8;
            bytes[5] = ((self.identification >> 8) & 0xff) as u8;
            bytes[6] = (self.flags_fragment & 0xff) as u8;
            bytes[7] = ((self.flags_fragment >> 8) & 0xff) as u8;
            bytes[8] = self.ttl;
            bytes[9] = self.protocol;
            bytes[10] = 0;
            bytes[11] = 0;
            bytes[12..16].copy_from_slice(&self.src_ip);
            bytes[16..20].copy_from_slice(&self.dst_ip);
            bytes
        }
    }
}

pub mod udp {
    

    pub const UDP_HEADER_SIZE: usize = 8;

    #[derive(Copy, Clone)]
    pub struct UdpHeader {
        pub src_port: u16,
        pub dst_port: u16,
        pub length: u16,
        pub checksum: u16,
    }

    impl UdpHeader {
        pub fn new(src_port: u16, dst_port: u16, payload_len: u16) -> Self {
            UdpHeader {
                src_port,
                dst_port,
                length: UDP_HEADER_SIZE as u16 + payload_len,
                checksum: 0,
            }
        }
    }
}