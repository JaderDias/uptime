mod model;

use chrono::{DateTime, Datelike, Duration, Local, Timelike};
use dotenvy::dotenv;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::env;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use warp::Filter;
use crate::model::{MetricType, PingResult};

const NEW_CLEAR_LINE: &str = "\n\x1b[K";
const MOVE_CURSOR_UP: &str = "\r\x1b[";
const MIN_MTU_SIZE: usize = 1448;
const MAX_MTU_SIZE: usize = 1504;
const MTU_STEP: usize = 4;

const PING_OPTIONS: ping_rs::PingOptions = ping_rs::PingOptions {
    ttl: 128,
    dont_fragment: true,
};

fn check_connectivity(ip_address: &IpAddr, mtu_size: usize) -> Option<u128> {
    let timeout = std::time::Duration::from_secs(1);
    let data: Vec<u8> = vec![0; mtu_size];

    let start_time = std::time::Instant::now();
    let result = ping_rs::send_ping(ip_address, timeout, &data, Some(&PING_OPTIONS));
    let latency_micros = start_time.elapsed().as_micros();

    if result.is_ok() {
        Some(latency_micros)
    } else {
        None
    }
}

fn check_connectivity_with_mtu(ip_address: &IpAddr) -> Option<PingResult> {
    for mtu_size in (MIN_MTU_SIZE..=MAX_MTU_SIZE).rev().step_by(MTU_STEP) {
        if let Some(latency_micros) = check_connectivity(ip_address, mtu_size) {
            return Some(PingResult {
                mtu: mtu_size,
                latency_micros,
            }); // Return the successful MTU size
        }
    }
    None // Return None if all MTU sizes fail
}

fn get_rows_for_html_graph(
    results: &HashMap<IpAddr, Arc<Mutex<VecDeque<(DateTime<Local>, PingResult)>>>>,
    ip_addresses: &[IpAddr],
    metric_type: MetricType,
) -> String {
    let mut rows = vec![];

    // Collect all unique timestamps
    let mut timestamps_set = BTreeSet::new();
    for ip in ip_addresses {
        for &(timestamp, _) in results.get(ip).unwrap().lock().unwrap().iter() {
            timestamps_set.insert(timestamp);
        }
    }

    // For each timestamp, get the MTU sizes for each IP
    for timestamp in timestamps_set {
        let mut row = format!(
            "[new Date({}, {}, {}, {}, {}), ",
            timestamp.year(),
            timestamp.month() - 1, // month0 in Rust is 0-based, JavaScript months are 0-based
            timestamp.day(),
            timestamp.hour(),
            timestamp.minute()
        );

        for (i, ip) in ip_addresses.iter().enumerate() {
            let mtu_size = results
                .get(ip)
                .unwrap()
                .lock()
                .unwrap()
                .iter()
                .find(|&&(ts, _)| ts == timestamp)
                .map(|(_, ping_result)| match metric_type { MetricType::MTU => ping_result.mtu as f64, MetricType::Latency => ping_result.latency_micros as f64 })
                .unwrap_or(0.0); // If no data for that timestamp, use 0
            row.push_str(&format!("{}", mtu_size));
            if i < ip_addresses.len() - 1 {
                row.push_str(", ");
            }
        }
        row.push(']');
        rows.push(row);
    }
    rows.join(",\n")
}


#[tokio::main]
async fn main() {
    dotenv().ok();

    let ip_addresses: Vec<IpAddr> = env::var("IP_ADDRESSES")
        .expect("IP_ADDRESSES must be set in .env")
        .split(',')
        .map(|ip| ip.trim().parse().expect("Invalid IP address format"))
        .collect();

    // Clone ip_addresses before moving it into the async closure
    let ip_addresses_clone = ip_addresses.clone();

    let results: HashMap<IpAddr, Arc<Mutex<VecDeque<(DateTime<Local>, PingResult)>>>> =
        ip_addresses
            .iter()
            .map(|ip| (*ip, Arc::new(Mutex::new(VecDeque::new()))))
            .collect();

    let results_clone = results.clone();

    tokio::spawn(async move {
        println!("start time {}", Local::now());

        loop {
            let now = Local::now();
            print!("{MOVE_CURSOR_UP}7A");
            for ip_address in &ip_addresses_clone {
                // Check for successful MTU size
                if let Some(ping_result) = check_connectivity_with_mtu(ip_address) {
                    // Add the timestamp and successful MTU size to results
                    results_clone
                        .get(ip_address)
                        .unwrap()
                        .lock()
                        .unwrap()
                        .push_back((now, ping_result.clone()));
                    print!("{NEW_CLEAR_LINE}{ip_address}: MTU {} latency {} micros", ping_result.mtu, ping_result.latency_micros);
                } else {
                    // If all MTU sizes fail, store 0 as the MTU size
                    results_clone
                        .get(ip_address)
                        .unwrap()
                        .lock()
                        .unwrap()
                        .push_back((
                            now,
                            PingResult {
                                mtu: 0,
                                latency_micros: 1_000_000,
                            },
                        ));
                    print!("{NEW_CLEAR_LINE}{ip_address}: 0");
                }
            }

            println!();

            // Remove old results older than one week
            let one_week_ago = now - Duration::days(7);
            for ip_address in &ip_addresses_clone {
                let results_for_ip = results_clone.get(ip_address).unwrap();
                let mut results_lock = results_for_ip.lock().unwrap();
                while results_lock
                    .front()
                    .map_or(false, |(timestamp, _)| *timestamp < one_week_ago)
                {
                    results_lock.pop_front();
                }
            }
        }
    });

    // Serve the HTML version that graphs the MTU size of the most recent successful pings
    let report_route = warp::path::end()
        .and_then(move || {
            let results_clone = results.clone();
            let ip_addresses = ip_addresses.clone(); // Re-use the original ip_addresses
            async move {
                let rows1 = get_rows_for_html_graph(&results_clone, &ip_addresses, MetricType::Latency);
                let rows2 = get_rows_for_html_graph(&results_clone, &ip_addresses, MetricType::MTU);

                // Prepare the column definitions
                let mut columns1 = String::from("data1.addColumn('date', 'Date');\n");
                for ip_address in &ip_addresses {
                    columns1.push_str(&format!(
                        "data1.addColumn('number', '{}');\n",
                        ip_address
                    ));
                }
                let mut columns2 = String::from("data2.addColumn('date', 'Date');\n");
                for ip_address in &ip_addresses {
                    columns2.push_str(&format!(
                        "data2.addColumn('number', '{}');\n",
                        ip_address
                    ));
                }

                let html = format!(
                    r#"<html>
      <head>
        <script type='text/javascript' src='https://www.gstatic.com/charts/loader.js'></script>
        <script type='text/javascript'>
          google.charts.load('current', {{'packages':['annotationchart']}});
          google.charts.setOnLoadCallback(drawChart);
          function drawChart() {{
            var data1 = new google.visualization.DataTable();
            {columns1}
            data1.addRows([
                {rows1}
            ]);

            var chart1 = new google.visualization.AnnotationChart(document.getElementById('chart_div1'));
            chart1.draw(data1, {{
              displayAnnotations: true,
              scaleType: 'allfixed',
              legendPosition: 'newRow',
              thickness: 2,
              zoomStartTime: new Date(new Date().getTime() - 24*60*60*1000)  // Start from 24 hours ago
            }});

            var data2 = new google.visualization.DataTable();
            {columns2}
            data2.addRows([
                {rows2}
            ]);

            var chart2 = new google.visualization.AnnotationChart(document.getElementById('chart_div2'));
            chart2.draw(data2, {{
              displayAnnotations: true,
              scaleType: 'allfixed',
              legendPosition: 'newRow',
              thickness: 2,
              zoomStartTime: new Date(new Date().getTime() - 24*60*60*1000)  // Start from 24 hours ago
            }});
          }}
        </script>
      </head>

      <body>
        <div id='chart_div1' style='width: 900px; height: 500px;'></div>
        <div id='chart_div2' style='width: 900px; height: 500px;'></div>
      </body>
    </html>
    "#
                );
                Ok::<_, warp::Rejection>(warp::reply::html(html))
            }
        });

    println!("Report also available via HTTP port 8080");
    warp::serve(report_route).run(([0, 0, 0, 0], 8080)).await;
}
