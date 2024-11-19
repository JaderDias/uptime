mod model;

use crate::model::{IpResults, MetricType, PingResult};
use chrono::{Datelike, Duration, Local, Timelike};
use dotenvy::dotenv;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::env;
use std::net::IpAddr;
use std::sync::Arc;
use warp::Filter;
use tokio::sync::Mutex;

const NEW_CLEAR_LINE: &str = "\n\x1b[K";
const MOVE_CURSOR_UP: &str = "\r\x1b[";
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

fn check_connectivity_with_mtu(
    ip_address: &IpAddr,
    min_mtu_size: usize,
    max_mtu_size: usize,
) -> Option<PingResult> {
    for mtu_size in (min_mtu_size..=max_mtu_size).rev().step_by(MTU_STEP) {
        if let Some(latency_micros) = check_connectivity(ip_address, mtu_size) {
            return Some(PingResult {
                mtu: mtu_size,
                latency_micros,
            }); // Return the successful MTU size
        }
    }
    None // Return None if all MTU sizes fail
}

#[allow(clippy::cast_precision_loss)]
async fn get_rows_for_html_graph(
    results: &IpResults,
    ip_addresses: &[IpAddr],
    metric_type: &MetricType,
) -> String {
    let mut rows = vec![];

    // Collect all unique timestamps
    let mut timestamps_set = BTreeSet::new();
    for ip in ip_addresses {
        let timestamps = results.get(ip).unwrap().lock().await;
        for &(timestamp, _) in timestamps.iter() {
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
                .await
                .iter()
                .find(|&&(ts, _)| ts == timestamp)
                .map_or(0.0, |(_, ping_result)| match metric_type {
                    MetricType::Mtu => ping_result.mtu as f64,
                    MetricType::Latency => ping_result.latency_micros as f64,
                }); // If no data for that timestamp, use 0
            row.push_str(&format!("{mtu_size}"));
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

    let min_mtu_size: usize = env::var("MIN_MTU_SIZE")
        .expect("MIN_MTU_SIZE must be set in .env")
        .parse()
        .expect("Invalid MIN_MTU_SIZE");
    let max_mtu_size: usize = env::var("MAX_MTU_SIZE")
        .expect("MAX_MTU_SIZE must be set in .env")
        .parse()
        .expect("Invalid MAX_MTU_SIZE");

    let interval_millis: u64 = env::var("INTERVAL_MILLIS")
        .expect("INTERVAL_MILLIS must be set in .env")
        .parse()
        .expect("Invalid INTERVAL_MILLIS");

    let port: u16 = env::var("PORT")
        .expect("PORT must be set in .env")
        .parse()
        .expect("Invalid PORT");

    // Clone ip_addresses before moving it into the async closure
    let ip_addresses_clone = ip_addresses.clone();

    let results: IpResults = ip_addresses
        .iter()
        .map(|ip| (*ip, Arc::new(Mutex::new(VecDeque::new()))))
        .collect();

    let results_clone = results.clone();

    tokio::spawn(async move {
        println!("start time {}", Local::now());

        loop {
            let current_minute = Local::now()
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap();

            print!("{MOVE_CURSOR_UP}{}A", &ip_addresses_clone.len() + 1);
            for ip_address in &ip_addresses_clone {
                // Check for successful MTU size
                let ping_result =
                    check_connectivity_with_mtu(ip_address, min_mtu_size, max_mtu_size).unwrap_or(
                        PingResult {
                            mtu: 0,
                            latency_micros: 1_000_000,
                        },
                    );
                let mut results_lock = results_clone.get(ip_address).unwrap().lock().await;
                // Check if we already have an entry for the current minute
                if let Some((last_time, last_result)) = results_lock.pop_back() {
                    // If it's the same minute and latency is higher, update it
                    if last_time == current_minute {
                        results_lock.push_back((
                            current_minute,
                            PingResult {
                                mtu: last_result.mtu.min(ping_result.mtu),
                                latency_micros: last_result
                                    .latency_micros
                                    .max(ping_result.latency_micros),
                            },
                        ));
                    } else {
                        results_lock.push_back((last_time, last_result));
                        results_lock.push_back((current_minute, ping_result.clone()));
                    }
                } else {
                    // Add to results if no entry exists
                    results_lock.push_back((current_minute, ping_result.clone()));
                }
                print!(
                    "{NEW_CLEAR_LINE}{ip_address}: MTU {} latency {} micros",
                    ping_result.mtu, ping_result.latency_micros
                );
            }

            println!();

            // Remove old results older than one week
            let one_week_ago = current_minute - Duration::days(7);
            for ip_address in &ip_addresses_clone {
                let results_for_ip = results_clone.get(ip_address).unwrap();
                let mut results_lock = results_for_ip.lock().await;
                while results_lock
                    .front()
                    .map_or(false, |(timestamp, _)| *timestamp < one_week_ago)
                {
                    results_lock.pop_front();
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(interval_millis)).await;
        }
    });

    // Serve the HTML version that graphs the MTU size of the most recent successful pings
    let report_route = warp::path::end()
        .and_then(move || {
            let results_clone = results.clone();
            let ip_addresses = ip_addresses.clone(); // Re-use the original ip_addresses
            async move {
                let mut html = String::from("<html>
      <head>
        <script type='text/javascript' src='https://www.gstatic.com/charts/loader.js'></script>
        <script type='text/javascript'>
          google.charts.load('current', {'packages':['annotationchart']});
          google.charts.setOnLoadCallback(drawChart);
          function drawChart() {");

                let rows1 = get_rows_for_html_graph(&results_clone, &ip_addresses, &MetricType::Latency).await;

                // Prepare the column definitions
                let mut columns1 = String::from("data1.addColumn('date', 'Date');\n");
                for ip_address in &ip_addresses {
                    columns1.push_str(&format!(
                        "data1.addColumn('number', '{ip_address}');\n",

                    ));
                }
                html = format!(r#"{html}
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
            "#);

                if min_mtu_size < max_mtu_size {
                    let rows2 = get_rows_for_html_graph(&results_clone, &ip_addresses, &MetricType::Mtu).await;
                    let mut columns2 = String::from("data2.addColumn('date', 'Date');\n");
                    for ip_address in &ip_addresses {
                        columns2.push_str(&format!(
                            "data2.addColumn('number', '{ip_address}');\n",
                        ));
                    }

                    html = format!(r#"{html}
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
            "#);
                }

                html = format!(
                    r#"{html}
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

    println!("Report also available via HTTP port {port}");
    warp::serve(report_route).run(([0, 0, 0, 0], port)).await;
}
