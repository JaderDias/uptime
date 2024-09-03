use chrono::{DateTime, Datelike, Duration, Local, Timelike};
use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use warp::Filter;

#[derive(Clone)]
struct ConnectivityCheck {
    timestamp: DateTime<Local>,
    success: bool,
}

const CLEAR_LINE: &'static str = "\x1b[K";
const MOVE_CURSOR_UP: &'static str = "\r\x1b[";
const REPORT_LINES: usize = 2;

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

#[allow(clippy::cast_precision_loss)]
fn calculate_percentage(failures: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (failures as f64 / total as f64) * 100.0
    }
}

fn generate_report(results: &Arc<Mutex<VecDeque<ConnectivityCheck>>>) -> Vec<String> {
    let now = Local::now();
    let mut output = Vec::new();
    let runtime = { now - results.lock().unwrap().front().unwrap().timestamp };

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

    let results_clone: Vec<ConnectivityCheck> =
        { results.lock().unwrap().iter().cloned().collect() };
    for check in results_clone {
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
            output.push(format!(
                "failed last {}:\t{:.0} %\t{}/{}",
                label,
                calculate_percentage(failed_counts[i], total_counts[i]),
                failed_counts[i],
                total_counts[i]
            ));
        }
    }

    if runtime < intervals.last().expect("missing element").0 {
        output.push(format!(
            "total failed:\t\t{:.0} %\t{}/{}",
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

fn get_graph(results: &VecDeque<ConnectivityCheck>) -> String {
    let mut graph = String::new();
    let mut results_iter = results.iter();
    let mut check: Option<&ConnectivityCheck> = results_iter.next_back();
    loop {
        let mut total = 0;
        for _ in 0..9 {
            if let Some(some_check) = check {
                if some_check.success {
                    total += 1;
                }
            } else {
                break;
            }

            check = results_iter.next_back();
        }
        let symbol = match total {
            0 => "░",
            1 => "▏",
            2 => "▎",
            3 => "▍",
            4 => "▌",
            5 => "▋",
            6 => "▊",
            7 => "▉",
            _ => "█",
        };
        graph.push_str(symbol);
        if let None = check {
            return graph;
        }
    }
}

fn get_rows(results: &VecDeque<ConnectivityCheck>) -> String {
    let mut graph = vec![];
    let mut i = 0;
    while i < results.len() {
        let mut successes = 0;
        let mut failures = 0;
        let timestamp = results[i].timestamp;
        for (j, check) in results.iter().enumerate().skip(i) {
            if check.timestamp.minute() != timestamp.minute() {
                break;
            }
            if check.success {
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

    tokio::spawn(async move {
        let check_interval = std::time::Duration::from_secs(5);
        println!("start time {}", Local::now());
        let mut report_lines = 0;

        loop {
            let now = Local::now();
            for ip_address in &ip_addresses {
                let success = check_connectivity(ip_address);
                results_clone.lock().unwrap().push_back(ConnectivityCheck {
                    timestamp: now,
                    success,
                });
            }

            let report = generate_report(&results_clone);
            if report_lines > 0 {
                println!("{MOVE_CURSOR_UP}{report_lines}A{}", report.join(&format!("\n{CLEAR_LINE}")));
            } else {
                println!("{}", report.join("\n"));
            }
            report_lines = report.len() + REPORT_LINES;
            let graph = { get_graph(&results_clone.lock().unwrap()) };
            println!("{CLEAR_LINE}<< most recent\n{graph}");
                tokio::time::sleep(check_interval).await;

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
        }
    });

    let report_route = warp::path::end()
        .and_then(move || {
            let results_clone = Arc::clone(&results);
            async move {
                let report = generate_report(&results_clone).join("<br/>");
                let rows = get_rows(&results_clone.lock().unwrap());
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
