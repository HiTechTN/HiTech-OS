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
    pub irq_line: u8,
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
            irq_line: 0,
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

    pub fn enable_interrupts(&mut self) {
        let imr: u16 = IMR_TOK | IMR_RER | IMR_TER;
        unsafe { Port::<u16>::new(self.io_base + REG_IMR as u16).write(imr); }
    }

    pub fn clear_interrupt(&mut self) {
        let isr = unsafe { Port::<u16>::new(self.io_base + REG_ISR as u16).read() };
        unsafe { Port::<u16>::new(self.io_base + REG_ISR as u16).write(isr); }
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

    let (io_base, irq_line) = crate::pcie::find_device_by_id(RTL8139_VENDOR_ID, RTL8139_DEVICE_ID)
        .map(|(b, d, f)| {
            let bar = crate::acpi::pci::read_bar(b, d, f, 0);
            let irq = crate::acpi::peci::read_interrupt_line(b, d, f);
            (bar, irq)
        })
        .map(|(bar_opt, irq)| {
            let io = bar_opt.map(|(addr, _is_io)| {
                if addr > 0xffff { RTL8139_IO_BASE } else { addr as u16 }
            }).unwrap_or(RTL8139_IO_BASE);
            (io, irq)
        })
        .unwrap_or_else(|| {
            println!("  RTL8139 non trouve sur PCI, utilise bar par defaut");
            (RTL8139_IO_BASE, 11)
        });

    unsafe {
        NIC.init(io_base)?;
        NIC.irq_line = irq_line;
        NIC.enable_interrupts();
    }
    
    println!("  MAC: {}", unsafe { NIC.mac_str() });
    println!("  IRQ: {}", irq_line);
    println!("  reseau initialise");
    
    Ok(())
}

pub fn handle_nic_irq() {
    unsafe {
        if !NIC.present { return; }
        NIC.clear_interrupt();
        let mut buf = [0u8; 1518];
        while let Some(len) = NIC.receive_packet(&mut buf) {
            process_incoming_packet(&buf[..len]);
        }
    }
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
                        if icmp.type_ == icmp::ICMP_TYPE_ECHO_REPLY {
                            let id = (icmp.rest_of_header >> 16) as u16;
                            let seq = icmp.rest_of_header as u16;
                            let mut reply = PING_REPLY.lock();
                            *reply = Some(PingInfo {
                                id,
                                seq,
                                reply_ip: src_ip,
                                rtt: 0,
                            });
                        }
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
                } else if protocol == ipv4::IP_PROTOCOL_TCP && buffer.len() >= 54 {
                    let ip_header_len = ((ip_header[0] & 0x0f) * 4) as usize;
                    let tcp_data = &buffer[14 + ip_header_len..];
                    tcp::handle_tcp_packet(tcp_data, &src_ip, &dst_ip);
                } else if protocol == ipv4::IP_PROTOCOL_UDP && buffer.len() >= 42 {
                    let ip_header_len = ((ip_header[0] & 0x0f) * 4) as usize;
                    let udp_payload = &buffer[14 + ip_header_len + 8..];
                    if let Some(udp_hdr) = udp::parse(&buffer[14 + ip_header_len..]) {
                        if udp_hdr.dst_port == crate::dhcp::DHCP_CLIENT_PORT {
                            if let Some(result) = crate::dhcp::parse_dhcp_reply(udp_payload) {
                                let mut ip = crate::network::IP_ADDRESS.lock();
                                *ip = result.yiaddr;
                                println!("DHCP: IP {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
                                println!("DHCP: Masque {}.{}.{}.{}", result.subnet_mask[0], result.subnet_mask[1], result.subnet_mask[2], result.subnet_mask[3]);
                                println!("DHCP: Passerelle {}.{}.{}.{}", result.gateway[0], result.gateway[1], result.gateway[2], result.gateway[3]);
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
                ethertype: u16::from_be_bytes([data[12], data[13]]),
            })
        }
    }

    pub fn build_frame(dst: &[u8; 6], src: &[u8; 6], ethertype: u16, payload: &[u8]) -> Option<Vec<u8>> {
        let mut packet = Vec::with_capacity(14 + payload.len());
        packet.extend_from_slice(dst);
        packet.extend_from_slice(src);
        packet.extend_from_slice(&ethertype.to_be_bytes());
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
                let word = if i + 1 < data.len() {
                    u16::from_be_bytes([data[i], data[i + 1]]) as u32
                } else {
                    (data[i] as u32) << 8
                };
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
            bytes[2..4].copy_from_slice(&self.total_length.to_be_bytes());
            bytes[4..6].copy_from_slice(&self.identification.to_be_bytes());
            bytes[6..8].copy_from_slice(&self.flags_fragment.to_be_bytes());
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
                    u16::from_be_bytes([data[i], data[i + 1]]) as u32
                } else {
                    (data[i] as u32) << 8
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

    pub fn parse(data: &[u8]) -> Option<UdpHeader> {
        if data.len() < 8 {
            return None;
        }
        Some(UdpHeader {
            src_port: u16::from_be_bytes([data[0], data[1]]),
            dst_port: u16::from_be_bytes([data[2], data[3]]),
            length: u16::from_be_bytes([data[4], data[5]]),
            checksum: u16::from_be_bytes([data[6], data[7]]),
        })
    }

    pub fn to_bytes(hdr: &UdpHeader) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..2].copy_from_slice(&hdr.src_port.to_be_bytes());
        buf[2..4].copy_from_slice(&hdr.dst_port.to_be_bytes());
        buf[4..6].copy_from_slice(&hdr.length.to_be_bytes());
        buf[6..8].copy_from_slice(&hdr.checksum.to_be_bytes());
        buf
    }

    pub fn pseudo_checksum(src_ip: &[u8; 4], dst_ip: &[u8; 4], udp_len: u16) -> u32 {
        let mut sum: u32 = 0;
        for i in (0..4).step_by(2) {
            sum += u16::from_be_bytes([src_ip[i], src_ip[i + 1]]) as u32;
            sum += u16::from_be_bytes([dst_ip[i], dst_ip[i + 1]]) as u32;
        }
        sum += 0u16 as u32;
        sum += crate::network::ipv4::IP_PROTOCOL_UDP as u32;
        sum += udp_len as u32;
        sum
    }

    pub fn compute_checksum(src_ip: &[u8; 4], dst_ip: &[u8; 4], hdr: &UdpHeader, payload: &[u8]) -> u16 {
        let mut sum = pseudo_checksum(src_ip, dst_ip, hdr.length);
        let hb = to_bytes(hdr);
        for i in (0..8).step_by(2) {
            sum += u16::from_be_bytes([hb[i], hb[i + 1]]) as u32;
        }
        for i in (0..payload.len()).step_by(2) {
            let w = if i + 1 < payload.len() {
                u16::from_be_bytes([payload[i], payload[i + 1]]) as u32
            } else {
                (payload[i] as u32) << 8
            };
            sum += w;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        !(sum as u16)
    }

    pub fn send_packet(dst_ip: &[u8; 4], dst_port: u16, src_port: u16, payload: &[u8]) -> Option<()> {
        let ip = crate::network::IP_ADDRESS.lock();
        let src_ip = *ip;
        drop(ip);

        let hdr = UdpHeader::new(src_port, dst_port, payload.len() as u16);
        let checksum = compute_checksum(&src_ip, dst_ip, &hdr, payload);
        let mut hdr_cs = hdr;
        hdr_cs.checksum = checksum;
        let hb = to_bytes(&hdr_cs);

        let mut udp_data = alloc::vec::Vec::with_capacity(8 + payload.len());
        udp_data.extend_from_slice(&hb);
        udp_data.extend_from_slice(payload);

        let ip_hdr = crate::network::ipv4::Ipv4Header::new(
            src_ip, *dst_ip,
            crate::network::ipv4::IP_PROTOCOL_UDP,
            udp_data.len() as u16,
        );
        let mut ip_bytes = ip_hdr.as_bytes();
        let ip_cs = crate::network::ipv4::Ipv4Header::calculate_checksum(&ip_bytes);
        ip_bytes[10..12].copy_from_slice(&ip_cs.to_be_bytes());

        let mut ip_pkt = alloc::vec::Vec::with_capacity(20 + udp_data.len());
        ip_pkt.extend_from_slice(&ip_bytes);
        ip_pkt.extend_from_slice(&udp_data);

        let dst_mac = crate::network::arp_resolve(dst_ip)?;
        unsafe {
            let nic_mac = crate::network::NIC.mac_address;
            let frame = crate::network::ethernet::build_frame(
                &dst_mac, &nic_mac,
                crate::network::ethernet::ETH_TYPE_IPV4,
                &ip_pkt,
            )?;
            crate::network::NIC.send_packet(&frame).ok()
        }
    }
}

pub mod tcp {

    pub const TCP_FIN: u8 = 0x01;
    pub const TCP_SYN: u8 = 0x02;
    pub const TCP_RST: u8 = 0x04;
    pub const TCP_PSH: u8 = 0x08;
    pub const TCP_ACK: u8 = 0x10;
    pub const TCP_URG: u8 = 0x20;

    pub const TCP_STATE_CLOSED: u8 = 0;
    pub const TCP_STATE_LISTEN: u8 = 1;
    pub const TCP_STATE_SYN_SENT: u8 = 2;
    pub const TCP_STATE_SYN_RECEIVED: u8 = 3;
    pub const TCP_STATE_ESTABLISHED: u8 = 4;
    pub const TCP_STATE_FIN_WAIT_1: u8 = 5;
    pub const TCP_STATE_FIN_WAIT_2: u8 = 6;
    pub const TCP_STATE_CLOSE_WAIT: u8 = 7;
    pub const TCP_STATE_CLOSING: u8 = 8;
    pub const TCP_STATE_LAST_ACK: u8 = 9;
    pub const TCP_STATE_TIME_WAIT: u8 = 10;

    pub const MAX_TCP_CONNS: usize = 16;
    pub const TCP_WINDOW: u16 = 65535;

    #[derive(Copy, Clone)]
    pub struct TcpHeader {
        pub src_port: u16,
        pub dst_port: u16,
        pub seq_num: u32,
        pub ack_num: u32,
        pub data_offset_reserved_flags: u16,
        pub window: u16,
        pub checksum: u16,
        pub urgent: u16,
    }

    impl TcpHeader {
        pub fn data_offset(&self) -> usize {
            ((self.data_offset_reserved_flags >> 12) as usize) * 4
        }

        pub fn flags(&self) -> u8 {
            (self.data_offset_reserved_flags & 0x3f) as u8
        }

        pub fn set_flags(&mut self, flags: u8) {
            self.data_offset_reserved_flags = (self.data_offset_reserved_flags & 0xffc0) | flags as u16;
        }

        pub fn has_flag(&self, flag: u8) -> bool {
            self.flags() & flag != 0
        }
    }

    pub fn parse_tcp_header(data: &[u8]) -> Option<TcpHeader> {
        if data.len() < 20 {
            return None;
        }
        Some(TcpHeader {
            src_port: u16::from_be_bytes([data[0], data[1]]),
            dst_port: u16::from_be_bytes([data[2], data[3]]),
            seq_num: u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            ack_num: u32::from_be_bytes([data[8], data[9], data[10], data[11]]),
            data_offset_reserved_flags: u16::from_be_bytes([data[12], data[13]]),
            window: u16::from_be_bytes([data[14], data[15]]),
            checksum: u16::from_be_bytes([data[16], data[17]]),
            urgent: u16::from_be_bytes([data[18], data[19]]),
        })
    }

    pub fn tcp_to_bytes(hdr: &TcpHeader) -> [u8; 20] {
        let mut buf = [0u8; 20];
        buf[0..2].copy_from_slice(&hdr.src_port.to_be_bytes());
        buf[2..4].copy_from_slice(&hdr.dst_port.to_be_bytes());
        buf[4..8].copy_from_slice(&hdr.seq_num.to_be_bytes());
        buf[8..12].copy_from_slice(&hdr.ack_num.to_be_bytes());
        buf[12..14].copy_from_slice(&hdr.data_offset_reserved_flags.to_be_bytes());
        buf[14..16].copy_from_slice(&hdr.window.to_be_bytes());
        buf[16..18].copy_from_slice(&hdr.checksum.to_be_bytes());
        buf[18..20].copy_from_slice(&hdr.urgent.to_be_bytes());
        buf
    }

    pub fn tcp_pseudo_cs(src_ip: &[u8; 4], dst_ip: &[u8; 4], tcp_len: u16) -> u32 {
        let mut sum: u32 = 0;
        for i in (0..4).step_by(2) {
            sum += u16::from_be_bytes([src_ip[i], src_ip[i + 1]]) as u32;
            sum += u16::from_be_bytes([dst_ip[i], dst_ip[i + 1]]) as u32;
        }
        sum += 0u16 as u32;
        sum += crate::network::ipv4::IP_PROTOCOL_TCP as u32;
        sum += tcp_len as u32;
        sum
    }

    pub fn tcp_compute_cs(src_ip: &[u8; 4], dst_ip: &[u8; 4], hdr: &TcpHeader, payload: &[u8]) -> u16 {
        let total_len = 20 + payload.len();
        let mut sum = tcp_pseudo_cs(src_ip, dst_ip, total_len as u16);
        let hb = tcp_to_bytes(hdr);
        for i in (0..20).step_by(2) {
            sum += u16::from_be_bytes([hb[i], hb[i + 1]]) as u32;
        }
        for i in (0..payload.len()).step_by(2) {
            let w = if i + 1 < payload.len() {
                u16::from_be_bytes([payload[i], payload[i + 1]]) as u32
            } else {
                (payload[i] as u32) << 8
            };
            sum += w;
        }
        while sum >> 16 != 0 {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        !(sum as u16)
    }

    pub struct TcpConnection {
        pub state: u8,
        pub src_ip: [u8; 4],
        pub dst_ip: [u8; 4],
        pub src_port: u16,
        pub dst_port: u16,
        pub seq: u32,
        pub ack: u32,
        pub send_buf: alloc::vec::Vec<u8>,
        pub recv_buf: alloc::vec::Vec<u8>,
        pub tx_buf: alloc::vec::Vec<u8>,
        pub rto_ticks: u32,
        pub rto_max: u32,
    }

    fn new_connection() -> TcpConnection {
        TcpConnection {
            state: TCP_STATE_CLOSED,
            src_ip: [0; 4],
            dst_ip: [0; 4],
            src_port: 0,
            dst_port: 0,
            seq: 0,
            ack: 0,
            send_buf: alloc::vec::Vec::new(),
            recv_buf: alloc::vec::Vec::new(),
            tx_buf: alloc::vec::Vec::new(),
            rto_ticks: 0,
            rto_max: 0,
        }
    }

    pub static TCP_TABLE: spin::Mutex<[Option<TcpConnection>; MAX_TCP_CONNS]> =
        spin::Mutex::new([
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
        ]);

    pub const MAX_LISTENERS: usize = 4;

    pub use alloc::collections::VecDeque;

    pub struct TcpListener {
        pub port: u16,
        pub backlog: VecDeque<u16>,
    }

    pub static TCP_LISTENERS: spin::Mutex<[Option<TcpListener>; MAX_LISTENERS]> =
        spin::Mutex::new([None, None, None, None]);

    pub fn alloc_listener(port: u16) -> Option<usize> {
        let mut listeners = TCP_LISTENERS.lock();
        for (i, slot) in listeners.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(TcpListener { port, backlog: VecDeque::new() });
                return Some(i);
            }
        }
        None
    }

    pub fn free_listener(idx: usize) {
        let mut listeners = TCP_LISTENERS.lock();
        if let Some(slot) = listeners.get_mut(idx) {
            *slot = None;
        }
    }

    pub fn find_listener(port: u16) -> Option<usize> {
        let listeners = TCP_LISTENERS.lock();
        for (i, slot) in listeners.iter().enumerate() {
            if let Some(ref l) = slot {
                if l.port == port {
                    return Some(i);
                }
            }
        }
        None
    }

    pub fn alloc_conn() -> Option<u16> {
        let mut table = TCP_TABLE.lock();
        for (i, slot) in table.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(new_connection());
                return Some(i as u16);
            }
        }
        None
    }

    pub fn free_conn(id: u16) {
        let mut table = TCP_TABLE.lock();
        if let Some(slot) = table.get_mut(id as usize) {
            *slot = None;
        }
    }

    fn send_segment(conn_id: u16, flags: u8, payload: &[u8]) -> bool {
        let (src_ip, dst_ip, src_port, dst_port, seq, ack) = {
            let table = TCP_TABLE.lock();
            let slot = match table.get(conn_id as usize) {
                Some(Some(c)) => c,
                _ => return false,
            };
            (slot.src_ip, slot.dst_ip, slot.src_port, slot.dst_port, slot.seq, slot.ack)
        };

        let payload_len = payload.len() as u16;
        let data_offset: u16 = (20 / 4) as u16;
        let tcp_hdr = TcpHeader {
            src_port,
            dst_port,
            seq_num: seq,
            ack_num: ack,
            data_offset_reserved_flags: (data_offset << 12) | flags as u16,
            window: TCP_WINDOW,
            checksum: 0,
            urgent: 0,
        };
        let cs = tcp_compute_cs(&src_ip, &dst_ip, &tcp_hdr, payload);
        let mut hdr_cs = tcp_hdr;
        hdr_cs.checksum = cs;
        let hdr_bytes = tcp_to_bytes(&hdr_cs);

        let mut segment = alloc::vec::Vec::with_capacity(20 + payload.len());
        segment.extend_from_slice(&hdr_bytes);
        segment.extend_from_slice(payload);

        let ip_hdr = crate::network::ipv4::Ipv4Header::new(
            src_ip, dst_ip,
            crate::network::ipv4::IP_PROTOCOL_TCP,
            segment.len() as u16,
        );
        let mut ip_bytes = ip_hdr.as_bytes();
        let ip_cs = crate::network::ipv4::Ipv4Header::calculate_checksum(&ip_bytes);
        ip_bytes[10..12].copy_from_slice(&ip_cs.to_be_bytes());
        let mut ip_packet = alloc::vec::Vec::with_capacity(20 + segment.len());
        ip_packet.extend_from_slice(&ip_bytes);
        ip_packet.extend_from_slice(&segment);

        if let Some(dst_mac) = crate::network::arp_resolve(&dst_ip) {
            unsafe {
                let nic_mac = crate::network::NIC.mac_address;
                if let Some(frame) = crate::network::ethernet::build_frame(
                    &dst_mac, &nic_mac,
                    crate::network::ethernet::ETH_TYPE_IPV4,
                    &ip_packet,
                ) {
                    let _ = crate::network::NIC.send_packet(&frame);
                }
            }
        }

        if (flags & TCP_SYN) != 0 || (flags & TCP_FIN) != 0 || payload_len > 0 {
            let mut table = TCP_TABLE.lock();
            if let Some(Some(ref mut c)) = table.get_mut(conn_id as usize) {
                c.seq = seq.wrapping_add(payload_len as u32 + if (flags & TCP_SYN) != 0 || (flags & TCP_FIN) != 0 { 1 } else { 0 });
                if payload_len > 0 || (flags & TCP_SYN) != 0 || (flags & TCP_FIN) != 0 {
                    c.tx_buf.clear();
                    c.tx_buf.extend_from_slice(&segment);
                    c.rto_ticks = 50;
                    c.rto_max = 800;
                }
            }
        }
        true
    }

    pub fn retransmit_check() {
        let mut table = TCP_TABLE.lock();
        for i in 0..table.len() {
            if let Some(Some(ref mut c)) = table.get_mut(i) {
                if c.rto_ticks > 0 && !c.tx_buf.is_empty() {
                    if c.state == TCP_STATE_ESTABLISHED {
                        c.rto_ticks = c.rto_ticks.saturating_sub(1);
                    }
                }
            }
        }
        drop(table);

        let to_retransmit: alloc::vec::Vec<(u16, alloc::vec::Vec<u8>)> = {
            let table = TCP_TABLE.lock();
            table.iter().enumerate().filter_map(|(i, slot)| {
                if let Some(c) = slot {
                    if c.rto_ticks == 0 && !c.tx_buf.is_empty() {
                        let state = c.state;
                        if state == TCP_STATE_ESTABLISHED {
                            return Some((i as u16, c.tx_buf.clone()));
                        }
                    }
                }
                None
            }).collect()
        };

        for (id, seg) in to_retransmit {
            let (src_ip, dst_ip) = {
                let table = TCP_TABLE.lock();
                let slot = table.get(id as usize).and_then(|s| s.as_ref());
                match slot {
                    Some(c) => (c.src_ip, c.dst_ip),
                    None => continue,
                }
            };
            let dst_mac = match crate::network::arp_resolve(&dst_ip) {
                Some(m) => m,
                None => continue,
            };
            let ip_h = crate::network::ipv4::Ipv4Header::new(src_ip, dst_ip, crate::network::ipv4::IP_PROTOCOL_TCP, seg.len() as u16);
            let mut ip_b = ip_h.as_bytes();
            let ip_c = crate::network::ipv4::Ipv4Header::calculate_checksum(&ip_b);
            ip_b[10..12].copy_from_slice(&ip_c.to_be_bytes());
            let mut ip_p = alloc::vec::Vec::new();
            ip_p.extend_from_slice(&ip_b);
            ip_p.extend_from_slice(&seg);
            unsafe {
                let nic_mac = crate::network::NIC.mac_address;
                if let Some(frame) = crate::network::ethernet::build_frame(&dst_mac, &nic_mac,
                    crate::network::ethernet::ETH_TYPE_IPV4, &ip_p) {
                    let _ = crate::network::NIC.send_packet(&frame);
                }
            }
            let mut table = TCP_TABLE.lock();
            if let Some(Some(ref mut c)) = table.get_mut(id as usize) {
                if c.rto_ticks == 0 && !c.tx_buf.is_empty() {
                    c.rto_ticks = core::cmp::min(c.rto_max / 2, 200).max(5);
                    c.rto_max = core::cmp::min(c.rto_max * 2, 1600);
                }
            }
        }
    }

    pub fn handle_tcp_packet(data: &[u8], src_ip: &[u8; 4], dst_ip: &[u8; 4]) {
        let tcp_hdr = match parse_tcp_header(data) {
            Some(h) => h,
            None => return,
        };
        let payload = &data[tcp_hdr.data_offset()..];
        let flags = tcp_hdr.flags();

        let mut table = TCP_TABLE.lock();
        let mut conn_idx = None;
        let mut is_listener = false;
        for (i, slot) in table.iter().enumerate() {
            if let Some(ref c) = slot {
                if c.dst_port == tcp_hdr.dst_port && c.state != TCP_STATE_CLOSED {
                    if c.state == TCP_STATE_LISTEN || (c.src_port == tcp_hdr.dst_port && c.dst_port == tcp_hdr.src_port) {
                        conn_idx = Some(i);
                        if c.state == TCP_STATE_LISTEN {
                            is_listener = true;
                        }
                        break;
                    }
                }
            }
        }

        if is_listener && (flags & TCP_SYN) != 0 {
            let src = *src_ip;
            let dst = *dst_ip;
            let src_port = tcp_hdr.dst_port;
            let dst_port = tcp_hdr.src_port;
            if let Some(new_id) = alloc_conn() {
                {
                    let mut t = TCP_TABLE.lock();
                    if let Some(Some(ref mut nc)) = t.get_mut(new_id as usize) {
                        nc.state = TCP_STATE_SYN_RECEIVED;
                        nc.src_ip = dst;
                        nc.dst_ip = src;
                        nc.src_port = src_port;
                        nc.dst_port = dst_port;
                        nc.seq = 1000;
                        nc.ack = tcp_hdr.seq_num.wrapping_add(1);
                    }
                }
                send_segment(new_id, TCP_SYN | TCP_ACK, &[]);
                let mut listeners = TCP_LISTENERS.lock();
                for slot in listeners.iter_mut() {
                    if let Some(ref mut l) = slot {
                        if l.port == src_port {
                            l.backlog.push_back(new_id);
                            break;
                        }
                    }
                }
            }
            return;
        }

        let idx = match conn_idx {
            Some(i) => i,
            None => {
                if (flags & TCP_RST) == 0 {
                    let src = src_ip;
                    let dst = dst_ip;
                    let rst_hdr = TcpHeader {
                        src_port: tcp_hdr.dst_port,
                        dst_port: tcp_hdr.src_port,
                        seq_num: 0,
                        ack_num: tcp_hdr.seq_num.wrapping_add(1),
                        data_offset_reserved_flags: ((20 / 4) as u16) << 12 | (TCP_RST | TCP_ACK) as u16,
                        window: TCP_WINDOW,
                        checksum: 0,
                        urgent: 0,
                    };
                    let cs = tcp_compute_cs(src, dst, &rst_hdr, &[]);
                    let mut hdr_cs = rst_hdr;
                    hdr_cs.checksum = cs;
                    let hb = tcp_to_bytes(&hdr_cs);
                    let mut seg = alloc::vec::Vec::new();
                    seg.extend_from_slice(&hb);
                    let ip_h = crate::network::ipv4::Ipv4Header::new(*src, *dst, crate::network::ipv4::IP_PROTOCOL_TCP, 20);
                    let mut ip_b = ip_h.as_bytes();
                    let ip_c = crate::network::ipv4::Ipv4Header::calculate_checksum(&ip_b);
                    ip_b[10..12].copy_from_slice(&ip_c.to_be_bytes());
                    let mut ip_p = alloc::vec::Vec::new();
                    ip_p.extend_from_slice(&ip_b);
                    ip_p.extend_from_slice(&seg);
                    if let Some(dst_mac) = crate::network::arp_resolve(dst) {
                        unsafe {
                            let nic_mac = crate::network::NIC.mac_address;
                            if let Some(frame) = crate::network::ethernet::build_frame(&dst_mac, &nic_mac,
                                crate::network::ethernet::ETH_TYPE_IPV4, &ip_p) {
                                let _ = crate::network::NIC.send_packet(&frame);
                            }
                        }
                    }
                }
                return;
            }
        };

        let slot = &mut table[idx];
        let conn = match slot.as_mut() {
            Some(c) => c,
            None => return,
        };

        match conn.state {
            TCP_STATE_SYN_SENT => {
                if (flags & TCP_SYN) != 0 && (flags & TCP_ACK) != 0 {
                    conn.ack = tcp_hdr.seq_num.wrapping_add(1);
                    conn.seq = conn.seq.wrapping_add(1);
                    conn.state = TCP_STATE_ESTABLISHED;
                    conn.tx_buf.clear();
                    conn.rto_ticks = 0;
                    drop(table);
                    send_segment(idx as u16, TCP_ACK, &[]);
                }
            }
            TCP_STATE_SYN_RECEIVED => {
                if (flags & TCP_ACK) != 0 {
                    conn.state = TCP_STATE_ESTABLISHED;
                    conn.tx_buf.clear();
                    conn.rto_ticks = 0;
                }
            }
            TCP_STATE_ESTABLISHED => {
                if (flags & TCP_FIN) != 0 {
                    conn.ack = tcp_hdr.seq_num.wrapping_add(1);
                    conn.state = TCP_STATE_CLOSE_WAIT;
                    drop(table);
                    send_segment(idx as u16, TCP_ACK, &[]);
                } else if !payload.is_empty() {
                    conn.ack = tcp_hdr.seq_num.wrapping_add(payload.len() as u32);
                    conn.recv_buf.extend_from_slice(payload);
                    conn.tx_buf.clear();
                    conn.rto_ticks = 0;
                    drop(table);
                    send_segment(idx as u16, TCP_ACK, &[]);
                } else if (flags & TCP_ACK) != 0 && !conn.tx_buf.is_empty() {
                    conn.tx_buf.clear();
                    conn.rto_ticks = 0;
                }
            }
            TCP_STATE_FIN_WAIT_1 => {
                if (flags & TCP_ACK) != 0 {
                    conn.state = TCP_STATE_FIN_WAIT_2;
                }
                if (flags & TCP_FIN) != 0 {
                    conn.state = TCP_STATE_CLOSING;
                }
            }
            TCP_STATE_FIN_WAIT_2 => {
                if (flags & TCP_FIN) != 0 {
                    conn.ack = tcp_hdr.seq_num.wrapping_add(1);
                    conn.state = TCP_STATE_TIME_WAIT;
                    drop(table);
                    send_segment(idx as u16, TCP_ACK, &[]);
                }
            }
            TCP_STATE_CLOSE_WAIT => {}
            TCP_STATE_CLOSING | TCP_STATE_LAST_ACK => {
                if (flags & TCP_ACK) != 0 {
                    conn.state = TCP_STATE_CLOSED;
                }
            }
            _ => {}
        }
    }

    pub fn tcp_connect(src_port: u16, dst_ip: [u8; 4], dst_port: u16) -> Option<u16> {
        let id = alloc_conn()?;
        {
            let mut table = TCP_TABLE.lock();
            let conn = table.get_mut(id as usize)?.as_mut()?;
            conn.state = TCP_STATE_SYN_SENT;
            let ip = crate::network::IP_ADDRESS.lock();
            conn.src_ip = *ip;
            drop(ip);
            conn.dst_ip = dst_ip;
            conn.src_port = src_port;
            conn.dst_port = dst_port;
            conn.seq = 1000;
            conn.ack = 0;
        }
        send_segment(id, TCP_SYN, &[]);
        Some(id)
    }

    pub fn tcp_listen(port: u16) -> Option<u16> {
        let idx = alloc_listener(port)?;
        Some(idx as u16)
    }

    pub fn tcp_accept(listener_idx: u16) -> Option<u16> {
        let mut listeners = TCP_LISTENERS.lock();
        let listener = listeners.get_mut(listener_idx as usize)?;
        let l = listener.as_mut()?;
        l.backlog.pop_front()
    }

    pub fn tcp_send(conn_id: u16, data: &[u8]) -> bool {
        send_segment(conn_id, TCP_PSH | TCP_ACK, data)
    }

    pub fn tcp_recv(conn_id: u16, buf: &mut [u8]) -> Option<usize> {
        let mut table = TCP_TABLE.lock();
        let conn = table.get_mut(conn_id as usize)?.as_mut()?;
        if conn.recv_buf.is_empty() {
            return None;
        }
        let len = conn.recv_buf.len().min(buf.len());
        buf[..len].copy_from_slice(&conn.recv_buf[..len]);
        conn.recv_buf.drain(..len);
        Some(len)
    }

    pub fn tcp_close(conn_id: u16) {
        send_segment(conn_id, TCP_FIN | TCP_ACK, &[]);
        let mut table = TCP_TABLE.lock();
        if let Some(Some(ref mut c)) = table.get_mut(conn_id as usize) {
            if c.state == TCP_STATE_ESTABLISHED || c.state == TCP_STATE_CLOSE_WAIT {
                c.state = TCP_STATE_FIN_WAIT_1;
            }
        }
    }
}

pub mod socket {
    use crate::network::tcp;

    pub const SOCK_TYPE_TCP: u8 = 0;
    pub const SOCK_TYPE_UDP: u8 = 1;

    pub fn create_socket(sock_type: u8, local_port: u16) -> Option<u16> {
        match sock_type {
            SOCK_TYPE_TCP => tcp::tcp_listen(local_port),
            SOCK_TYPE_UDP => Some(local_port as u16),
            _ => None,
        }
    }

    pub fn connect(sock_id: u16, dst_ip: [u8; 4], dst_port: u16) -> bool {
        let src_port = {
            let table = tcp::TCP_TABLE.lock();
            let mut found = None;
            for (i, slot) in table.iter().enumerate() {
                if let Some(ref c) = slot {
                    if i as u16 == sock_id && c.state == tcp::TCP_STATE_LISTEN {
                        found = Some(c.src_port);
                        break;
                    }
                }
            }
            found
        };
        if let Some(port) = src_port {
            tcp::tcp_connect(port, dst_ip, dst_port).is_some()
        } else {
            false
        }
    }

    pub fn send(sock_id: u16, data: &[u8]) -> bool {
        tcp::tcp_send(sock_id, data)
    }

    pub fn recv(sock_id: u16, buf: &mut [u8]) -> Option<usize> {
        tcp::tcp_recv(sock_id, buf)
    }

    pub fn close(sock_id: u16) {
        tcp::tcp_close(sock_id)
    }
}

pub static PING_REPLY: spin::Mutex<Option<PingInfo>> = spin::Mutex::new(None);

pub struct PingInfo {
    pub id: u16,
    pub seq: u16,
    pub reply_ip: [u8; 4],
    pub rtt: u64,
}

pub fn send_echo_request(dst_ip: &[u8; 4], id: u16, seq: u16) -> bool {
    let ip = IP_ADDRESS.lock();
    let src_ip = *ip;
    drop(ip);

    let icmp_packet = icmp::IcmpHeader::new_request(id, seq);
    let mut icmp_bytes = icmp_packet.to_bytes();
    let checksum = icmp::IcmpHeader::calculate_checksum(&icmp_bytes);
    icmp_bytes[2..4].copy_from_slice(&checksum.to_be_bytes());

    let ip_hdr = ipv4::Ipv4Header::new(src_ip, *dst_ip, ipv4::IP_PROTOCOL_ICMP, 8);
    let mut ip_bytes = ip_hdr.as_bytes();
    let ip_cs = ipv4::Ipv4Header::calculate_checksum(&ip_bytes);
    ip_bytes[10..12].copy_from_slice(&ip_cs.to_be_bytes());

    let mut ip_packet = alloc::vec::Vec::with_capacity(28);
    ip_packet.extend_from_slice(&ip_bytes);
    ip_packet.extend_from_slice(&icmp_bytes);

    let dst_mac = match arp_resolve(dst_ip) {
        Some(m) => m,
        None => return false,
    };
    unsafe {
        let nic_mac = NIC.mac_address;
        if let Some(frame) = ethernet::build_frame(
            &dst_mac, &nic_mac,
            ethernet::ETH_TYPE_IPV4,
            &ip_packet,
        ) {
            NIC.send_packet(&frame).is_ok()
        } else {
            false
        }
    }
}

pub fn network_tick() {
    unsafe {
        if !NIC.present {
            return;
        }
        let mut buf = [0u8; 1518];
        if let Some(len) = NIC.receive_packet(&mut buf) {
            process_incoming_packet(&buf[..len]);
        }
    }
    crate::network::tcp::retransmit_check();
}