//! Minimal Ethernet / ARP / IPv4 / UDP helpers for the link between the Mac and the phone.

use std::net::Ipv4Addr;

pub const ETH_HDR: usize = 14;
pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const BROADCAST: [u8; 6] = [0xff; 6];

pub fn eth_frame(dst: [u8; 6], src: [u8; 6], ethertype: u16, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity((ETH_HDR + payload.len()).max(60));
    f.extend_from_slice(&dst);
    f.extend_from_slice(&src);
    f.extend_from_slice(&ethertype.to_be_bytes());
    f.extend_from_slice(payload);
    if f.len() < 60 {
        f.resize(60, 0);
    }
    f
}

pub struct Eth<'a> {
    pub dst: [u8; 6],
    pub ethertype: u16,
    pub payload: &'a [u8],
}

pub fn parse_eth(f: &[u8]) -> Option<Eth<'_>> {
    if f.len() < ETH_HDR {
        return None;
    }
    Some(Eth {
        dst: f[0..6].try_into().unwrap(),
        ethertype: u16::from_be_bytes([f[12], f[13]]),
        payload: &f[ETH_HDR..],
    })
}

pub fn is_unicast(mac: &[u8; 6]) -> bool {
    mac[0] & 1 == 0
}

pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for c in data.chunks(2) {
        let w = if c.len() == 2 {
            u16::from_be_bytes([c[0], c[1]])
        } else {
            (c[0] as u16) << 8
        };
        sum += w as u32;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Builds an IPv4/UDP packet (UDP checksum 0 = none, permitted for IPv4).
pub fn udp_ipv4(src: Ipv4Addr, dst: Ipv4Addr, sport: u16, dport: u16, payload: &[u8]) -> Vec<u8> {
    let udp_len = 8 + payload.len();
    let total = 20 + udp_len;
    let mut p = Vec::with_capacity(total);
    p.extend_from_slice(&[0x45, 0x10]);
    p.extend_from_slice(&(total as u16).to_be_bytes());
    p.extend_from_slice(&[0, 0, 0, 0, 64, 17, 0, 0]);
    p.extend_from_slice(&src.octets());
    p.extend_from_slice(&dst.octets());
    let c = checksum(&p[..20]);
    p[10..12].copy_from_slice(&c.to_be_bytes());
    p.extend_from_slice(&sport.to_be_bytes());
    p.extend_from_slice(&dport.to_be_bytes());
    p.extend_from_slice(&(udp_len as u16).to_be_bytes());
    p.extend_from_slice(&[0, 0]);
    p.extend_from_slice(payload);
    p
}

/// If `ip` is an IPv4 UDP packet to `dport`, returns its UDP payload.
pub fn udp_payload_to(ip: &[u8], dport: u16) -> Option<&[u8]> {
    if ip.len() < 20 || ip[0] >> 4 != 4 || ip[9] != 17 {
        return None;
    }
    // Ignore fragments (DHCP replies are never fragmented in practice).
    if u16::from_be_bytes([ip[6], ip[7]]) & 0x3fff != 0 {
        return None;
    }
    let ihl = ((ip[0] & 0x0f) as usize) * 4;
    let total = (u16::from_be_bytes([ip[2], ip[3]]) as usize).min(ip.len());
    let udp = ip.get(ihl..total)?;
    if udp.len() < 8 || u16::from_be_bytes([udp[2], udp[3]]) != dport {
        return None;
    }
    let ulen = (u16::from_be_bytes([udp[4], udp[5]]) as usize).clamp(8, udp.len());
    Some(&udp[8..ulen])
}

/// Trims Ethernet padding from an IPv4 packet using its total-length field.
pub fn trim_ipv4(ip: &[u8]) -> Option<&[u8]> {
    if ip.len() < 20 || ip[0] >> 4 != 4 {
        return None;
    }
    let total = u16::from_be_bytes([ip[2], ip[3]]) as usize;
    if total < 20 || total > ip.len() {
        return None;
    }
    Some(&ip[..total])
}

pub fn mask_prefix(mask: Ipv4Addr) -> u32 {
    u32::from(mask).leading_ones()
}

pub fn fmt_mac(m: &[u8; 6]) -> String {
    m.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_of_known_header() {
        // Example header from RFC 1071 discussions / Wikipedia IPv4 checksum.
        let h = [
            0x45, 0x00, 0x00, 0x73, 0x00, 0x00, 0x40, 0x00, 0x40, 0x11, 0x00, 0x00, 0xc0, 0xa8,
            0x00, 0x01, 0xc0, 0xa8, 0x00, 0xc7,
        ];
        assert_eq!(checksum(&h), 0xb861);
    }

    #[test]
    fn udp_roundtrip() {
        let p = udp_ipv4(
            Ipv4Addr::new(1, 2, 3, 4),
            Ipv4Addr::BROADCAST,
            68,
            67,
            b"hello",
        );
        assert_eq!(checksum(&p[..20]), 0);
        assert_eq!(udp_payload_to(&p, 67), Some(&b"hello"[..]));
        assert_eq!(udp_payload_to(&p, 68), None);
        let mut padded = p.clone();
        padded.extend_from_slice(&[0; 10]);
        assert_eq!(trim_ipv4(&padded), Some(&p[..]));
    }

    #[test]
    fn prefix() {
        assert_eq!(mask_prefix(Ipv4Addr::new(255, 255, 255, 0)), 24);
    }
}
