use alloc::vec::Vec;

pub const DHCP_SERVER_PORT: u16 = 67;
pub const DHCP_CLIENT_PORT: u16 = 68;
pub const DHCP_MAGIC: u32 = 0x63825363;

pub const DHCP_DISCOVER: u8 = 1;
pub const DHCP_OFFER: u8 = 2;
pub const DHCP_REQUEST: u8 = 3;
pub const DHCP_ACK: u8 = 5;
pub const DHCP_NAK: u8 = 6;

pub const OPT_SUBNET_MASK: u8 = 1;
pub const OPT_ROUTER: u8 = 3;
pub const OPT_DNS: u8 = 6;
pub const OPT_LEASE_TIME: u8 = 51;
pub const OPT_MESSAGE_TYPE: u8 = 53;
pub const OPT_SERVER_ID: u8 = 54;
pub const OPT_REQUESTED_IP: u8 = 50;
pub const OPT_END: u8 = 255;

pub const DHCP_BROADCAST: [u8; 4] = [255, 255, 255, 255];

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct DhcpMessage {
    pub op: u8,
    pub htype: u8,
    pub hlen: u8,
    pub hops: u8,
    pub xid: u32,
    pub secs: u16,
    pub flags: u16,
    pub ciaddr: [u8; 4],
    pub yiaddr: [u8; 4],
    pub siaddr: [u8; 4],
    pub giaddr: [u8; 4],
    pub chaddr: [u8; 16],
    pub sname: [u8; 64],
    pub boot_file: [u8; 128],
}

pub struct DhcpResult {
    pub yiaddr: [u8; 4],
    pub server_ip: [u8; 4],
    pub subnet_mask: [u8; 4],
    pub gateway: [u8; 4],
}

pub fn build_discover(xid: u32, mac: &[u8; 6]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(300);
    msg.push(1); // op = bootrequest
    msg.push(1); // htype = ethernet
    msg.push(6); // hlen
    msg.push(0); // hops
    msg.extend_from_slice(&xid.to_be_bytes());
    msg.extend_from_slice(&[0u8; 2]); // secs
    msg.extend_from_slice(&(0x8000u16).to_be_bytes()); // broadcast flag
    msg.extend_from_slice(&[0u8; 4]); // ciaddr
    msg.extend_from_slice(&[0u8; 4]); // yiaddr
    msg.extend_from_slice(&[0u8; 4]); // siaddr
    msg.extend_from_slice(&[0u8; 4]); // giaddr
    msg.extend_from_slice(&mac[..]); // chaddr (16 bytes)
    msg.extend_from_slice(&[0u8; 10]);
    msg.extend_from_slice(&[0u8; 64]); // sname
    msg.extend_from_slice(&[0u8; 128]); // boot_file
    msg.extend_from_slice(&DHCP_MAGIC.to_be_bytes()); // magic cookie
    // Options
    msg.push(OPT_MESSAGE_TYPE);
    msg.push(1);
    msg.push(DHCP_DISCOVER);
    msg.push(OPT_END);
    msg
}

pub fn build_request(xid: u32, mac: &[u8; 6], yiaddr: &[u8; 4], server_ip: &[u8; 4]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(300);
    msg.push(1); // op = bootrequest
    msg.push(1); // htype
    msg.push(6); // hlen
    msg.push(0); // hops
    msg.extend_from_slice(&xid.to_be_bytes());
    msg.extend_from_slice(&[0u8; 2]);
    msg.extend_from_slice(&(0x8000u16).to_be_bytes());
    msg.extend_from_slice(&[0u8; 4]); // ciaddr
    msg.extend_from_slice(yiaddr); // yiaddr
    msg.extend_from_slice(&[0u8; 4]); // siaddr
    msg.extend_from_slice(&[0u8; 4]); // giaddr
    msg.extend_from_slice(&mac[..]);
    msg.extend_from_slice(&[0u8; 10]);
    msg.extend_from_slice(&[0u8; 64]);
    msg.extend_from_slice(&[0u8; 128]);
    msg.extend_from_slice(&DHCP_MAGIC.to_be_bytes());
    msg.push(OPT_MESSAGE_TYPE);
    msg.push(1);
    msg.push(DHCP_REQUEST);
    // Requested IP
    msg.push(OPT_REQUESTED_IP);
    msg.push(4);
    msg.extend_from_slice(yiaddr);
    // Server identifier
    msg.push(OPT_SERVER_ID);
    msg.push(4);
    msg.extend_from_slice(server_ip);
    msg.push(OPT_END);
    msg
}

pub fn parse_options(data: &[u8]) -> Option<DhcpResult> {
    if data.len() < 240 {
        return None;
    }
    let magic_offset = 236;
    if data[magic_offset..magic_offset + 4] != DHCP_MAGIC.to_be_bytes() {
        return None;
    }

    let yiaddr: [u8; 4] = data[16..20].try_into().ok()?;
    if yiaddr == [0; 4] {
        return None;
    }

    let mut result = DhcpResult {
        yiaddr,
        server_ip: [0; 4],
        subnet_mask: [255, 255, 255, 0],
        gateway: yiaddr,
    };

    let mut i = 240;
    while i + 1 < data.len() {
        let opt_type = data[i];
        if opt_type == OPT_END {
            break;
        }
        if opt_type == 0 {
            i += 1;
            continue;
        }
        let opt_len = data[i + 1] as usize;
        if i + 2 + opt_len > data.len() {
            break;
        }
        let opt_data = &data[i + 2..i + 2 + opt_len];
        match opt_type {
            OPT_MESSAGE_TYPE => {
                if opt_data.len() >= 1 && opt_data[0] != DHCP_OFFER && opt_data[0] != DHCP_ACK {
                    return None;
                }
            }
            OPT_SERVER_ID => {
                if opt_data.len() >= 4 {
                    result.server_ip.copy_from_slice(&opt_data[..4]);
                }
            }
            OPT_SUBNET_MASK => {
                if opt_data.len() >= 4 {
                    result.subnet_mask.copy_from_slice(&opt_data[..4]);
                }
            }
            OPT_ROUTER => {
                if opt_data.len() >= 4 {
                    result.gateway.copy_from_slice(&opt_data[..4]);
                }
            }
            _ => {}
        }
        i += 2 + opt_len;
    }

    if result.server_ip == [0; 4] {
        return None;
    }
    Some(result)
}

pub fn send_discover() -> Option<u32> {
    let xid: u32 = 0x12345678;
    unsafe {
        let mac = crate::network::NIC.mac_address;
        let discover = build_discover(xid, &mac);
        let _ = crate::network::udp::send_packet(
            &DHCP_BROADCAST,
            DHCP_SERVER_PORT,
            DHCP_CLIENT_PORT,
            &discover,
        );
    }
    Some(xid)
}

pub fn send_request(xid: u32, yiaddr: &[u8; 4], server_ip: &[u8; 4]) -> bool {
    unsafe {
        let mac = crate::network::NIC.mac_address;
        let req = build_request(xid, &mac, yiaddr, server_ip);
        crate::network::udp::send_packet(&DHCP_BROADCAST, DHCP_SERVER_PORT, DHCP_CLIENT_PORT, &req).is_some()
    }
}

pub fn parse_dhcp_reply(data: &[u8]) -> Option<DhcpResult> {
    if data.len() < 240 {
        return None;
    }
    parse_options(data)
}
