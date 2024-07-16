use chrono::{Local, Duration, DateTime};
use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Instant;

struct ConnectivityCheck {
    timestamp: DateTime<Local>,
    success: bool,
}

fn check_connectivity(ip_address: &IpAddr) -> bool {
    let data = [1, 2, 3, 4];  // ping data
    let timeout = std::time::Duration::from_secs(1);
    let options = ping_rs::PingOptions { ttl: 128, dont_fragment: true };
    let result = ping_rs::send_ping(ip_address, timeout, &data, Some(&options));
    result.is_ok()
}

fn calculate_percentage(failures: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (failures as f64 / total as f64) * 100.0
    }
}

fn main() {
    let ip_addresses = vec![
        IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
        IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
    ];

    let mut results: VecDeque<ConnectivityCheck> = VecDeque::new();
    let ten_seconds = std::time::Duration::from_secs(10);

    println!("{} start", Local::now());

    loop {
        let now = Local::now();
        for ip_address in &ip_addresses {
            let success = check_connectivity(ip_address);
            results.push_back(ConnectivityCheck {
                timestamp: now,
                success,
            });
            if !success {
                println!("{} {:?} failed", now, ip_address);
            }
        }

        // Remove old results
        let one_hour_ago = now - Duration::hours(1);
        while results.front().map_or(false, |check| check.timestamp < one_hour_ago) {
            results.pop_front();
        }

        // Calculate statistics
        let mut total_checks = 0;
        let mut failed_checks = 0;
        let mut failed_last_min = 0;
        let mut failed_last_10_min = 0;
        let mut failed_last_30_min = 0;
        let mut failed_last_hour = 0;

        for check in &results {
            total_checks += 1;
            if !check.success {
                failed_checks += 1;
                if check.timestamp > now - Duration::minutes(1) {
                    failed_last_min += 1;
                }
                if check.timestamp > now - Duration::minutes(10) {
                    failed_last_10_min += 1;
                }
                if check.timestamp > now - Duration::minutes(30) {
                    failed_last_30_min += 1;
                }
                if check.timestamp > now - Duration::hours(1) {
                    failed_last_hour += 1;
                }
            }
        }

        println!("\nStatistics:");
        println!("% failed: {:.2}%", calculate_percentage(failed_checks, total_checks));
        println!("% failed last 1 min: {:.2}%", calculate_percentage(failed_last_min, total_checks));
        println!("% failed last 10 min: {:.2}%", calculate_percentage(failed_last_10_min, total_checks));
        println!("% failed last 30 min: {:.2}%", calculate_percentage(failed_last_30_min, total_checks));
        println!("% failed last 1 hour: {:.2}%", calculate_percentage(failed_last_hour, total_checks));

        // Print simple graph
        println!("\nGraph:");
        for check in &results {
            let symbol = if check.success { "█" } else { "░" };
            print!("{}", symbol);
        }
        println!();

        std::thread::sleep(ten_seconds);
    }
}
