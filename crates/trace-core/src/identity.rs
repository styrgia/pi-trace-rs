use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct RunId(String);
impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}
impl RunId {
    pub fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self(format!("r{stamp:x}-{:x}-{sequence:x}", std::process::id()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
