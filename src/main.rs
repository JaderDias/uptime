use chrono::Local;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;
use std::time::SystemTime;

fn check_connectivity(ip_address: &IpAddr) -> bool {
    let data = [1,2,3,4];  // ping data
    let timeout = Duration::from_secs(1);
    let options = ping_rs::PingOptions { ttl: 128, dont_fragment: true };
    let result = ping_rs::send_ping(ip_address, timeout, &data, Some(&options));
    result.is_ok()
}

fn main() {
    let ip_addresses = vec![
        IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
        IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
    ];

    let now = SystemTime::now();
    let datetime: chrono::DateTime<Local> = now.into();
    let ten_seconds = Duration::from_secs(10);
    println!("{datetime} start");
    loop {
        for ip_address in &ip_addresses {
            if !check_connectivity(ip_address) {
                let now = SystemTime::now();
                let datetime: chrono::DateTime<Local> = now.into();
                println!("{datetime} {ip_address:?} failed");
            }
        }

        std::thread::sleep(ten_seconds);
    }
}
