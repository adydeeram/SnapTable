use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch. All persisted timestamps use this unit.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}
