
pub enum MetricType {
    MTU,
    Latency,
}

#[derive(Clone)]
pub struct PingResult {
    pub mtu: usize,
    pub latency_micros: u128,
}