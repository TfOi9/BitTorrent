use std::net::{IpAddr, Ipv4Addr, UdpSocket};

pub fn detect_local_ip() -> IpAddr {
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
    };

    if socket.connect("8.8.8.8:80").is_ok() {
        if let Ok(addr) = socket.local_addr() {
            return addr.ip();
        }
    }

    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
}
