use chrono::{DateTime, Datelike, Duration, Local, Timelike};
use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use warp::Filter;

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

async fn generate_report(
    results: Arc<Mutex<VecDeque<ConnectivityCheck>>>,
    separator: &str,
) -> String {
    let now = Local::now();
    let mut output = String::new();
    let runtime = now - results.lock().unwrap().front().unwrap().timestamp;

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

    for check in results.lock().unwrap().iter() {
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
            output.push_str(&format!(
                "failed last {}:\t{:.0} %\t{}/{}{separator}",
                label,
                calculate_percentage(failed_counts[i], total_counts[i]),
                failed_counts[i],
                total_counts[i]
            ));
        }
    }

    if runtime < intervals.last().expect("missing element").0 {
        output.push_str(&format!(
            "total failed:\t{:.0} %\t{}/{}{separator}",
            calculate_percentage(
                *failed_counts.last().expect("missing element"),
                *total_counts.last().expect("missing element")
            ),
            failed_counts.last().expect("missing element"),
            total_counts.last().expect("missing element")
        ));
    }

    output
}

fn print_combined_graph(results: &VecDeque<ConnectivityCheck>) -> String {
    let mut graph = String::new();
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
        graph.push_str(symbol);
        i += 4;
    }
    graph
}

fn get_rows(results: &VecDeque<ConnectivityCheck>) -> String {
    let mut graph = vec![];
    let mut i = 0;
    while i < results.len() {
        let mut successes = 0;
        let mut failures = 0;
        let timestamp = results[i].timestamp;
        for j in i..results.len() {
            if results[j].timestamp.minute() != timestamp.minute() {
                break;
            }
            if results[j].success {
                successes += 1;
            } else {
                failures += 1;
            }
            i = j;
        }

        graph.push(format!(
            "[new Date({}, {}, {}, {}, {}), {successes}, {failures}]",
            timestamp.year(),
            timestamp.month0(),
            timestamp.day(),
            timestamp.hour(),
            timestamp.minute()
        ));
        i += 1;
    }
    graph.join(",")
}

#[tokio::main]
async fn main() {
    let ip_addresses = vec![
        IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
        IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
    ];

    let results: Arc<Mutex<VecDeque<ConnectivityCheck>>> = Arc::new(Mutex::new(VecDeque::new()));
    let results_clone = Arc::clone(&results);
    let results_clone2 = Arc::clone(&results);

    tokio::spawn(async move {
        let ten_seconds = std::time::Duration::from_secs(10);
        println!("start time {}", Local::now());

        loop {
            let now = Local::now();
            for ip_address in &ip_addresses {
                let success = check_connectivity(ip_address);
                results_clone.lock().unwrap().push_back(ConnectivityCheck {
                    timestamp: now,
                    success,
                });
            }

            println!(
                "{}",
                generate_report(results_clone.clone(), "\n").await.as_str()
            );
            println!(
                "Combined Graph:\n{}",
                &print_combined_graph(&results_clone2.lock().unwrap())
            );

            // Remove old results
            let one_week_ago = now - Duration::days(7);
            while results_clone
                .lock()
                .unwrap()
                .front()
                .map_or(false, |check| check.timestamp < one_week_ago)
            {
                results_clone.lock().unwrap().pop_front();
            }

            tokio::time::sleep(ten_seconds).await;
        }
    });

    let report_route = warp::path::end()
        .and_then(move || {
            let results_clone = Arc::clone(&results);
            let results_clone2 = Arc::clone(&results);
            async move {
                let report = generate_report(results_clone, "<br/>").await;
                let rows = get_rows(&results_clone2.lock().unwrap());
                let html = format!(r#"<html>
  <head>
    <script type='text/javascript' src='https://www.gstatic.com/charts/loader.js'></script>
    <script type='text/javascript'>
      google.charts.load('current', {{'packages':['annotatedtimeline']}});
      google.charts.setOnLoadCallback(drawChart);
      function drawChart() {{
        var data = new google.visualization.DataTable();
        data.addColumn('date', 'Date');
        data.addColumn('number', 'Sucesses');
        data.addColumn('number', 'Failures');
        data.addRows([
            {rows}
        ]);

        var chart = new google.visualization.AnnotatedTimeLine(document.getElementById('chart_div'));
        chart.draw(data, {{displayAnnotations: true}});
      }}
    </script>
  </head>

  <body>
    {report}
    <div id='chart_div' style='width: 700px; height: 240px;'></div>
  </body>
</html>
"#);
                Ok::<_, warp::Rejection>(warp::reply::html(html))
            }
        });

    println!("report also available via HTTP port 8080");
    warp::serve(report_route).run(([0, 0, 0, 0], 8080)).await;
}
