#[derive(Clone, Debug)]
pub struct MPResult<T> {
    pub name: &'static str,
    pub value: T,
    pub duration_ms: i64,
}

#[derive(Clone, Debug)]
pub struct PhaseProfile {
    pub phase: &'static str,
    pub total_ms: i64,
    pub bottlenecks: Vec<(String, i64)>,
}
