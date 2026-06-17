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
pub static ARP_CACHE: spin::Mutex<arp::ArpCache> = spin::Mutex::new(arp::ArpCache::new());

pub static IP_ADDRESS: spin::Mutex<[u8; 4]> = spin::Mutex::new([0, 0, 0, 0]);

pub fn init_network() -> Result<(), NetworkError> {
    println!("Recherche NIC RTL8139...");
    
    unsafe {
        NIC.init(RTL8139_IO_BASE)?;
    }
    
    println!("  MAC: {}", unsafe { NIC.mac_str() });
    println!("  reseau initialise");
    
    Ok(())
}

pub fn set_ip_address(ip: [u8; 4]) {
    *IP_ADDRESS.lock() = ip;
    println!("  IP: {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
}

pub fn arp_resolve(target_ip: &[u8; 4]) -> Option<[u8; 6]> {
    let cache = ARP_CACHE.lock();
    cache.lookup(target_ip).copied()
}

pub fn arp_request(target_ip: &[u8; 4]) {
    use ethernet::ETH_TYPE_ARP;
    let ip = *IP_ADDRESS.lock();
    let mac = unsafe { NIC.mac_address };
    let packet = arp::ArpPacket::new_request(&mac, &ip, target_ip);
    let arp_bytes = packet.to_bytes();
    if let Some(eth_packet) = ethernet::build_frame(&arp::BROADCAST_MAC, &mac, ETH_TYPE_ARP, &arp_bytes) {
        unsafe { let _ = NIC.send_packet(&eth_packet); }
    }
}

pub fn arp_handle_packet(data: &[u8]) {
    if let Some(packet) = arp::ArpPacket::parse(data) {
        let ip = *IP_ADDRESS.lock();
        if packet.operation == arp::ARP_OP_REQUEST && packet.target_ip == ip {
            unsafe {
                ARP_CACHE.lock().update(packet.sender_ip, packet.sender_mac);
                let mac = NIC.mac_address;
                let reply = arp::ArpPacket::new_reply(&mac, &ip, &packet.sender_mac, &packet.sender_ip);
                let reply_bytes = reply.to_bytes();
                if let Some(eth_packet) = ethernet::build_frame(&packet.sender_mac, &mac, ethernet::ETH_TYPE_ARP, &reply_bytes) {
                    let _ = NIC.send_packet(&eth_packet);
                }
            }
        } else if packet.operation == arp::ARP_OP_REPLY {
            ARP_CACHE.lock().update(packet.sender_ip, packet.sender_mac);
        }
    }
}

pub fn process_incoming_packet(buffer: &[u8]) {
    if let Some(eth) = ethernet::EthernetHeader::parse(buffer) {
        match eth.ethertype {
            ethernet::ETH_TYPE_ARP => {
                if buffer.len() >= 42 {
                    arp_handle_packet(&buffer[14..]);
                }
            }
            ethernet::ETH_TYPE_IPV4 => {
                if buffer.len() < 34 { return; }
                let ip_header = &buffer[14..34];
                let protocol = ip_header[9];
                let src_ip: [u8; 4] = ip_header[12..16].try_into().unwrap_or([0; 4]);
                let dst_ip: [u8; 4] = ip_header[16..20].try_into().unwrap_or([0; 4]);

                if protocol == ipv4::IP_PROTOCOL_ICMP && buffer.len() >= 42 {
                    let ip_header_len = ((ip_header[0] & 0x0f) * 4) as usize;
                    let icmp_data = &buffer[14 + ip_header_len..];
                    if let Some(icmp) = icmp::IcmpHeader::parse(icmp_data) {
                        if icmp.type_ == icmp::ICMP_TYPE_ECHO_REQUEST {
                            let mut reply_hdr = icmp::IcmpHeader {
                                type_: icmp::ICMP_TYPE_ECHO_REPLY,
                                code: 0,
                                checksum: 0,
                                rest_of_header: icmp.rest_of_header,
                            };
                            let reply_bytes = reply_hdr.to_bytes();
                            let payload = &icmp_data[8..];
                            let mut full_reply = Vec::with_capacity(8 + payload.len());
                            full_reply.extend_from_slice(&reply_bytes);
                            full_reply.extend_from_slice(payload);
                            reply_hdr.checksum = icmp::IcmpHeader::calculate_checksum(&full_reply);
                            full_reply[2..4].copy_from_slice(&reply_hdr.checksum.to_be_bytes());

                            let ip_hdr = ipv4::Ipv4Header::new(dst_ip, src_ip, ipv4::IP_PROTOCOL_ICMP, full_reply.len() as u16);
                            let mut ip_bytes = ip_hdr.as_bytes();
                            let checksum = ipv4::Ipv4Header::calculate_checksum(&ip_bytes);
                            ip_bytes[10..12].copy_from_slice(&checksum.to_be_bytes());

                            let mut ip_packet = Vec::with_capacity(20 + full_reply.len());
                            ip_packet.extend_from_slice(&ip_bytes);
                            ip_packet.extend_from_slice(&full_reply);

                            unsafe {
                                let nic_mac = NIC.mac_address;
                                if let Some(frame) = ethernet::build_frame(&eth.src, &nic_mac, ethernet::ETH_TYPE_IPV4, &ip_packet) {
                                    let _ = NIC.send_packet(&frame);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub mod arp {

    pub const ARP_HARDWARE_ETHERNET: u16 = 1;
    pub const ARP_PROTOCOL_IPV4: u16 = 0x0800;
    pub const ARP_OP_REQUEST: u16 = 1;
    pub const ARP_OP_REPLY: u16 = 2;

    pub const BROADCAST_MAC: [u8; 6] = [0xff; 6];

    pub struct ArpPacket {
        pub hardware_type: u16,
        pub protocol_type: u16,
        pub hardware_size: u8,
        pub protocol_size: u8,
        pub operation: u16,
        pub sender_mac: [u8; 6],
        pub sender_ip: [u8; 4],
        pub target_mac: [u8; 6],
        pub target_ip: [u8; 4],
    }

    impl ArpPacket {
        pub fn new_request(sender_mac: &[u8; 6], sender_ip: &[u8; 4], target_ip: &[u8; 4]) -> Self {
            ArpPacket {
                hardware_type: ARP_HARDWARE_ETHERNET,
                protocol_type: ARP_PROTOCOL_IPV4,
                hardware_size: 6,
                protocol_size: 4,
                operation: ARP_OP_REQUEST,
                sender_mac: *sender_mac,
                sender_ip: *sender_ip,
                target_mac: [0; 6],
                target_ip: *target_ip,
            }
        }

        pub fn new_reply(sender_mac: &[u8; 6], sender_ip: &[u8; 4], target_mac: &[u8; 6], target_ip: &[u8; 4]) -> Self {
            ArpPacket {
                hardware_type: ARP_HARDWARE_ETHERNET,
                protocol_type: ARP_PROTOCOL_IPV4,
                hardware_size: 6,
                protocol_size: 4,
                operation: ARP_OP_REPLY,
                sender_mac: *sender_mac,
                sender_ip: *sender_ip,
                target_mac: *target_mac,
                target_ip: *target_ip,
            }
        }

        pub fn parse(data: &[u8]) -> Option<ArpPacket> {
            if data.len() < 28 { return None; }
            Some(ArpPacket {
                hardware_type: u16::from_be_bytes([data[0], data[1]]),
                protocol_type: u16::from_be_bytes([data[2], data[3]]),
                hardware_size: data[4],
                protocol_size: data[5],
                operation: u16::from_be_bytes([data[6], data[7]]),
                sender_mac: data[8..14].try_into().ok()?,
                sender_ip: data[14..18].try_into().ok()?,
                target_mac: data[18..24].try_into().ok()?,
                target_ip: data[24..28].try_into().ok()?,
            })
        }

        pub fn to_bytes(&self) -> [u8; 28] {
            let mut bytes = [0u8; 28];
            bytes[0..2].copy_from_slice(&self.hardware_type.to_be_bytes());
            bytes[2..4].copy_from_slice(&self.protocol_type.to_be_bytes());
            bytes[4] = self.hardware_size;
            bytes[5] = self.protocol_size;
            bytes[6..8].copy_from_slice(&self.operation.to_be_bytes());
            bytes[8..14].copy_from_slice(&self.sender_mac);
            bytes[14..18].copy_from_slice(&self.sender_ip);
            bytes[18..24].copy_from_slice(&self.target_mac);
            bytes[24..28].copy_from_slice(&self.target_ip);
            bytes
        }
    }

    #[derive(Copy, Clone)]
    pub struct ArpEntry {
        pub ip: [u8; 4],
        pub mac: [u8; 6],
    }

    pub const ARP_CACHE_SIZE: usize = 16;

    pub struct ArpCache {
        pub entries: [Option<ArpEntry>; ARP_CACHE_SIZE],
        pub count: usize,
    }

    impl ArpCache {
        pub const fn new() -> Self {
            ArpCache {
                entries: [None; ARP_CACHE_SIZE],
                count: 0,
            }
        }

        pub fn lookup(&self, ip: &[u8; 4]) -> Option<&[u8; 6]> {
            for entry in &self.entries {
                if let Some(e) = entry {
                    if e.ip == *ip {
                        return Some(&e.mac);
                    }
                }
            }
            None
        }

        pub fn update(&mut self, ip: [u8; 4], mac: [u8; 6]) {
            for entry in &mut self.entries {
                if let Some(e) = entry {
                    if e.ip == ip {
                        e.mac = mac;
                        return;
                    }
                }
            }
            if self.count < ARP_CACHE_SIZE {
                self.entries[self.count] = Some(ArpEntry { ip, mac });
                self.count += 1;
            }
        }

        pub fn remove(&mut self, ip: &[u8; 4]) {
            for i in 0..self.count {
                if let Some(ref e) = self.entries[i] {
                    if &e.ip == ip {
                        self.entries[i] = None;
                        self.entries[i..self.count].rotate_left(1);
                        self.count -= 1;
                        return;
                    }
                }
            }
        }
    }
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

    pub fn build_frame(dst: &[u8; 6], src: &[u8; 6], ethertype: u16, payload: &[u8]) -> Option<Vec<u8>> {
        let mut packet = Vec::with_capacity(14 + payload.len());
        packet.extend_from_slice(dst);
        packet.extend_from_slice(src);
        packet.push((ethertype & 0xff) as u8);
        packet.push(((ethertype >> 8) & 0xff) as u8);
        packet.extend_from_slice(payload);
        Some(packet)
    }

    pub fn send_ipv4(src: &[u8; 6], dst: &[u8; 6], payload: &[u8]) -> Option<Vec<u8>> {
        build_frame(dst, src, ETH_TYPE_IPV4, payload)
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

        pub fn calculate_checksum(data: &[u8]) -> u16 {
            let mut sum: u32 = 0;
            for i in (0..data.len()).step_by(2) {
                let word = (data[i] as u32) | ((if i+1 < data.len() { data[i+1] } else { 0 }) as u32) << 8;
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

pub mod icmp {
    pub const ICMP_TYPE_ECHO_REPLY: u8 = 0;
    pub const ICMP_TYPE_ECHO_REQUEST: u8 = 8;

    pub struct IcmpHeader {
        pub type_: u8,
        pub code: u8,
        pub checksum: u16,
        pub rest_of_header: u32,
    }

    impl IcmpHeader {
        pub fn new_request(identifier: u16, sequence: u16) -> Self {
            IcmpHeader {
                type_: ICMP_TYPE_ECHO_REQUEST,
                code: 0,
                checksum: 0,
                rest_of_header: ((identifier as u32) << 16) | sequence as u32,
            }
        }

        pub fn parse(data: &[u8]) -> Option<IcmpHeader> {
            if data.len() < 8 { return None; }
            Some(IcmpHeader {
                type_: data[0],
                code: data[1],
                checksum: u16::from_be_bytes([data[2], data[3]]),
                rest_of_header: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            })
        }

        pub fn to_bytes(&self) -> [u8; 8] {
            let mut bytes = [0u8; 8];
            bytes[0] = self.type_;
            bytes[1] = self.code;
            bytes[2..4].copy_from_slice(&self.checksum.to_be_bytes());
            bytes[4..8].copy_from_slice(&self.rest_of_header.to_be_bytes());
            bytes
        }

        pub fn calculate_checksum(data: &[u8]) -> u16 {
            let mut sum: u32 = 0;
            for i in (0..data.len()).step_by(2) {
                let word = if i + 1 < data.len() {
                    (data[i] as u32) | ((data[i + 1] as u32) << 8)
                } else {
                    data[i] as u32
                };
                sum += word;
            }
            while sum >> 16 != 0 {
                sum = (sum & 0xffff) + (sum >> 16);
            }
            !(sum as u16)
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