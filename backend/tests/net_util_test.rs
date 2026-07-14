use backend::core::net_util::detect_local_ip;

#[test]
fn test_detect_local_ip_returns_valid_ip() {
    let ip = detect_local_ip();
    assert!(ip.is_ipv4(), "should detect an IPv4 address, got {ip:?}");
}
