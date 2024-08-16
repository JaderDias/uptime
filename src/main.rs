use chrono::{DateTime, Duration, Local};
use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};

struct ConnectivityCheck {
    timestamp: DateTime<Local>,
    success: bool,
}

fn check_connectivity(ip_address: &IpAddr) -> bool {
    let data = [1, 2, 3, 4]; // ping data
    let timeout = std::time::Duration::from_secs(1);
    let options = ping_rs::PingOptions {
        ttl: 128,
        dont_fragment: true,
    };
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

    let start_time = Local::now();
    println!("{} start", start_time);

    let mut failed_checks = 0;
    let mut total_checks = 0;
    loop {
        let now = Local::now();
        let runtime = now - start_time;
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
        while results
            .front()
            .map_or(false, |check| check.timestamp < one_week_ago)
        {
            results.pop_front();
        }

        // Calculate statistics
        let intervals = vec![
            (Duration::minutes(1), "1 min"),
            (Duration::minutes(10), "10 min"),
            (Duration::minutes(30), "30 min"),
            (Duration::hours(1), "1 hour"),
            (Duration::hours(2), "2 hours"),
            (Duration::hours(4), "4 hours"),
            (Duration::hours(6), "6 hours"),
            (Duration::hours(12), "12 hours"),
            (Duration::hours(24), "24 hours"),
            (Duration::days(2), "2 days"),
            (Duration::days(4), "4 days"),
            (Duration::days(7), "7 days"),
        ];

        let mut failed_counts = vec![0; intervals.len()];
        let mut total_counts = vec![0; intervals.len()];

        for check in &results {
            for (i, &(interval, _)) in intervals.iter().enumerate() {
                if check.timestamp > now - interval {
                    total_counts[i] += 1;
                    if !check.success {
                        failed_counts[i] += 1;
                    }
                }
            }
        }

        for (i, &(_, label)) in intervals.iter().enumerate() {
            if runtime >= intervals[i].0 {
                println!(
                    "failed last {}:\t{:.0} %\t{}/{}",
                    label,
                    calculate_percentage(failed_counts[i], total_counts[i]),
                    failed_counts[i],
                    total_counts[i]
                );
            }
        }
        println!(
            "total failures:\t\t{:.0} %\t{failed_checks}/{total_checks}",
            calculate_percentage(failed_checks, total_checks)
        );

        print_combined_graph(&results);
        println!();

        std::thread::sleep(ten_seconds);
    }
}

fn print_combined_graph(results: &VecDeque<ConnectivityCheck>) {
    let mut i = 0;
    while i < results.len() {
        let a = results[i].success;
        let b = results.get(i + 1).map_or(false, |c| c.success);
        let c = results.get(i + 2).map_or(false, |c| c.success);
        let d = results.get(i + 3).map_or(false, |c| c.success);
        let symbol = match (a, b, c, d) {
            (true, true, true, true) => "█",
            (true, true, true, false) => "▛",
            (true, true, false, true) => "▜",
            (true, true, false, false) => "▀",
            (true, false, true, true) => "▙",
            (true, false, true, false) => "▌",
            (true, false, false, true) => "▚",
            (true, false, false, false) => "▘",
            (false, true, true, true) => "▟",
            (false, true, true, false) => "▞",
            (false, true, false, true) => "▐",
            (false, true, false, false) => "▝",
            (false, false, true, true) => "▄",
            (false, false, true, false) => "▖",
            (false, false, false, true) => "▗",
            (false, false, false, false) => "░",
        };
        print!("{}", symbol);
        i += 4;
    }
}
