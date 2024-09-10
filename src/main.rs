use chrono::{DateTime, Datelike, Duration, Local, Timelike};
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use warp::Filter;

const NEW_CLEAR_LINE: &str = "\n\x1b[K";
const MOVE_CURSOR_UP: &str = "\r\x1b[";
const MIN_MTU_SIZE: usize = 1448;
const MAX_MTU_SIZE: usize = 1500;
const MTU_STEP: usize = 4;

const PING_OPTIONS: ping_rs::PingOptions = ping_rs::PingOptions {
    ttl: 128,
    dont_fragment: true,
};

fn check_connectivity(ip_address: &IpAddr, mtu_size: usize) -> bool {
    let timeout = std::time::Duration::from_secs(1);
    let data: Vec<u8> = vec![0; mtu_size];
    let result = ping_rs::send_ping(ip_address, timeout, &data, Some(&PING_OPTIONS));
    result.is_ok()
}

fn check_connectivity_with_mtu(ip_address: &IpAddr) -> Option<usize> {
    for mtu_size in (MIN_MTU_SIZE..=MAX_MTU_SIZE).rev().step_by(MTU_STEP) {
        if check_connectivity(ip_address, mtu_size) {
            return Some(mtu_size); // Return the successful MTU size
        }
    }
    None // Return None if all MTU sizes fail
}

fn get_graph_of_successes(
    results: &VecDeque<(DateTime<Local>, usize)>,
    ip_address: &IpAddr,
) -> String {
    let mut graph = String::new();
    for &(_, mtu_size) in results.iter() {
        let symbol = match mtu_size {
            MAX_MTU_SIZE => "█", // Biggest size is the fullest block
            1496 => "▉",
            1492 => "▊",
            1488 => "▋",
            1480 => "▌",
            1476 => "▍",
            1472 => "▎",
            0 => "░",            // No success is represented by an empty symbol
            _ => "▏",
        };
        graph.push_str(symbol);
    }
    format!("{}: {}", ip_address, graph)
}

fn get_rows_for_html_graph(
    results: &HashMap<IpAddr, Arc<Mutex<VecDeque<(DateTime<Local>, usize)>>>>,
    ip_addresses: &[IpAddr],
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
                .map(|&(_, mtu)| mtu as f64)
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
    use std::collections::HashMap;

    let ip_addresses = vec![
        IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
        IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
    ];

    // Clone ip_addresses before moving it into the async closure
    let ip_addresses_clone = ip_addresses.clone();

    let results: HashMap<IpAddr, Arc<Mutex<VecDeque<(DateTime<Local>, usize)>>>> = ip_addresses
        .iter()
        .map(|ip| (*ip, Arc::new(Mutex::new(VecDeque::new()))))
        .collect();

    let results_clone = results.clone();

    tokio::spawn(async move {
        let check_interval = std::time::Duration::from_secs(5);
        println!("start time {}", Local::now());

        loop {
            let now = Local::now();
            for ip_address in &ip_addresses_clone {
                // Check for successful MTU size
                if let Some(successful_mtu) = check_connectivity_with_mtu(ip_address) {
                    // Add the timestamp and successful MTU size to results
                    results_clone
                        .get(ip_address)
                        .unwrap()
                        .lock()
                        .unwrap()
                        .push_back((now, successful_mtu));
                } else {
                    // If all MTU sizes fail, store 0 as the MTU size
                    results_clone
                        .get(ip_address)
                        .unwrap()
                        .lock()
                        .unwrap()
                        .push_back((now, 0));
                }
            }

            print!("{MOVE_CURSOR_UP}3A");

            // Generate the console graph only for successful pings
            for ip_address in &ip_addresses_clone {
                let graph = get_graph_of_successes(
                    &results_clone.get(ip_address).unwrap().lock().unwrap(),
                    ip_address,
                );
                print!("{NEW_CLEAR_LINE}{graph}");
            }

            println!();
            tokio::time::sleep(check_interval).await;

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
                let rows = get_rows_for_html_graph(&results_clone, &ip_addresses);

                // Prepare the column definitions
                let mut columns = String::from("data.addColumn('date', 'Date');\n");
                for ip_address in &ip_addresses {
                    columns.push_str(&format!(
                        "data.addColumn('number', '{}');\n",
                        ip_address
                    ));
                }

                let html = format!(
                    r#"<html>
      <head>
        <script type='text/javascript' src='https://www.gstatic.com/charts/loader.js'></script>
        <script type='text/javascript'>
          google.charts.load('current', {{'packages':['annotatedtimeline']}});
          google.charts.setOnLoadCallback(drawChart);
          function drawChart() {{
            var data = new google.visualization.DataTable();
            {columns}
            data.addRows([
                {rows}
            ]);

            var chart = new google.visualization.AnnotatedTimeLine(document.getElementById('chart_div'));
            chart.draw(data, {{
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
        <div id='chart_div' style='width: 900px; height: 500px;'></div>
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
