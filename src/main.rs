use chrono::{Local, Duration, DateTime};
use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};

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

    let mut failed_checks = 0;
    let mut total_checks = 0;
    loop {
        let now = Local::now();
        for ip_address in &ip_addresses {
            let success = check_connectivity(ip_address);
            total_checks += 1;
            results.push_back(ConnectivityCheck {
                timestamp: now,
                success,
            });
            if !success {
                failed_checks += 1;
                println!("{} {:?} failed", now, ip_address);
            }
        }

        // Remove old results
        let one_hour_ago = now - Duration::hours(1);
        while results.front().map_or(false, |check| check.timestamp < one_hour_ago) {
            results.pop_front();
        }

        // Calculate statistics
        let mut total_checks_min = 0;
        let mut total_checks_10_min = 0;
        let mut total_checks_30_min = 0;
        let mut total_checks_hour = 0;
        let mut failed_last_min = 0;
        let mut failed_last_10_min = 0;
        let mut failed_last_30_min = 0;
        let mut failed_last_hour = 0;

        for check in &results {
            if check.timestamp > now - Duration::minutes(1) {
                total_checks_min += 1;
            }
            if check.timestamp > now - Duration::minutes(10) {
                total_checks_10_min += 1;
            }
            if check.timestamp > now - Duration::minutes(30) {
                total_checks_30_min += 1;
            }
            total_checks_hour += 1;
            if !check.success {
                if check.timestamp > now - Duration::minutes(1) {
                    failed_last_min += 1;
                }
                if check.timestamp > now - Duration::minutes(10) {
                    failed_last_10_min += 1;
                }
                if check.timestamp > now - Duration::minutes(30) {
                    failed_last_30_min += 1;
                }
                failed_last_hour += 1;
            }
        }

        println!("% failed last 1 min:\t{:.0} %\t{failed_last_min}/{total_checks_min}", calculate_percentage(failed_last_min, total_checks_min));
        println!("% failed last 10 min:\t{:.0} %\t{failed_last_10_min}/{total_checks_10_min}", calculate_percentage(failed_last_10_min, total_checks_10_min));
        println!("% failed last 30 min:\t{:.0} %\t{failed_last_30_min}/{total_checks_30_min}", calculate_percentage(failed_last_30_min, total_checks_30_min));
        println!("% failed last 1 hour:\t{:.0} %\t{failed_last_hour}/{total_checks_hour}", calculate_percentage(failed_last_hour, total_checks_hour));
        println!("% failed total:\t\t{:.0} %\t{failed_checks}/{total_checks}", calculate_percentage(failed_checks, total_checks));

        // Print simple graph
        for check in &results {
            let symbol = if check.success { "█" } else { "░" };
            print!("{}", symbol);
        }
        println!();

        std::thread::sleep(ten_seconds);
    }
}
