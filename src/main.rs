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
        let one_week_ago = now - Duration::days(7);
        while results.front().map_or(false, |check| check.timestamp < one_week_ago) {
            results.pop_front();
        }

        // Calculate statistics
        let intervals = vec![
            Duration::minutes(1),
            Duration::minutes(10),
            Duration::minutes(30),
            Duration::hours(1),
            Duration::hours(2),
            Duration::hours(4),
            Duration::hours(6),
            Duration::hours(12),
            Duration::hours(24),
            Duration::days(2),
            Duration::days(4),
            Duration::days(7),
        ];
        let mut failed_counts = vec![0; intervals.len()];
        let mut total_counts = vec![0; intervals.len()];

        for check in &results {
            for (i, &interval) in intervals.iter().enumerate() {
                if check.timestamp > now - interval {
                    total_counts[i] += 1;
                    if !check.success {
                        failed_counts[i] += 1;
                    }
                }
            }
        }

        let labels = vec![
            "1 min", "10 min", "30 min", "1 hour", "2 hours", "4 hours",
            "6 hours", "12 hours", "24 hours", "2 days", "4 days", "7 days",
        ];

        for (i, &label) in labels.iter().enumerate() {
            println!("failed last {}:\t{:.0} %\t{}/{}",
                     label,
                     calculate_percentage(failed_counts[i], total_counts[i]),
                     failed_counts[i],
                     total_counts[i]);
        }

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
