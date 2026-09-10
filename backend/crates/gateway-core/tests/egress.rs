use gateway_core::egress::ProxyEndpoint;

#[test]
fn proxy_endpoint_validates_scheme_port_and_redacts_credentials() {
    for value in [
        "socks5://127.0.0.1:1080",
        "socks5h://u:p%40ss@proxy.example:1080",
        "socks5h://[::1]:1080",
    ] {
        let endpoint = ProxyEndpoint::parse(value).unwrap();
        assert!(!format!("{endpoint:?}").contains("proxy.example"));
        assert!(!endpoint.address().contains('@'));
    }
    for value in [
        "http://proxy:80",
        "socks5h://proxy",
        "socks5://proxy:0",
        "socks5://proxy:1080/path",
        "socks5h://proxy:1080?x=1",
        "socks5h://proxy:1080#x",
        " socks5://proxy:1080",
        "socks5://u:%0a@proxy:1080",
        "socks5://:password@proxy:1080",
        "socks5h://u:p%zz@proxy:1080",
        "socks5://u:p%@proxy:1080",
    ] {
        assert!(ProxyEndpoint::parse(value).is_err(), "accepted {value}");
    }
}
