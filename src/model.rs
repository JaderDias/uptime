use chrono::{DateTime, Local};
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

pub enum MetricType {
    Mtu,
    Latency,
}

#[derive(Clone)]
pub struct PingResult {
    pub mtu: usize,
    pub latency_micros: u128,
}

pub type TimedResult = (DateTime<Local>, PingResult);
pub type IpResults = HashMap<IpAddr, Arc<Mutex<VecDeque<TimedResult>>>>;
