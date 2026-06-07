# Age-Based Log Retention Implementation

This documents the changes needed to implement age-aware log retention in CLIENT:
- Keep ALL logs from the last hour (prevents UI freeze from recent large outputs)
- Apply max history limit (500) only to logs older than 1 hour
- Prevents unbounded memory growth while enabling large recent outputs

## Changes to: `client-app/src/app.rs`

### 1. Update imports (line 3)

**OLD:**
```rust
use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
```

**NEW:**
```rust
use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH, Duration},
};
```

### 2. Update LogLine struct (around line 108)

**OLD:**
```rust
/// A single telemetry line stored in the Logs tab ring buffer.
pub struct LogLine {
    pub time: String,
    pub source: String,
    pub level: String,
    pub message: String,
}
```

**NEW:**
```rust
/// A single telemetry line stored in the Logs tab ring buffer.
pub struct LogLine {
    pub time: String,
    pub source: String,
    pub level: String,
    pub message: String,
    /// Timestamp when this entry was created (used for age-based retention).
    pub created_at: SystemTime,
}
```

### 3. Update push_log method (around line 380)

**OLD:**
```rust
/// Pushes a real log entry into the ring buffer (used by background tasks).
pub fn push_log(&mut self, source: &str, level: &str, message: &str) {
    self.logs.push(LogLine {
        time: current_time_str(),
        source: source.to_string(),
        level: level.to_string(),
        message: message.to_string(),
    });
    if self.logs.len() > 500 {
        self.logs.remove(0);
    }
}
```

**NEW:**
```rust
/// Pushes a real log entry into the ring buffer (used by background tasks).
/// 
/// Ring buffer retention policy:
/// - Keep ALL logs from the last hour (recent large outputs don't cause truncation)
/// - For logs older than 1 hour, apply a max history limit of 500
/// - This prevents UI freeze from recent output while preventing unbounded memory growth
pub fn push_log(&mut self, source: &str, level: &str, message: &str) {
    let now = SystemTime::now();
    let one_hour_ago = now - Duration::from_secs(3600);
    const MAX_OLD_LOGS: usize = 500;
    
    self.logs.push(LogLine {
        time: current_time_str(),
        source: source.to_string(),
        level: level.to_string(),
        message: message.to_string(),
        created_at: now,
    });
    
    // Count logs older than 1 hour
    let old_logs_count = self.logs.iter()
        .filter(|log| log.created_at < one_hour_ago)
        .count();
    
    // If we have more than MAX_OLD_LOGS that are old, trim from the front
    if old_logs_count > MAX_OLD_LOGS {
        let excess = old_logs_count - MAX_OLD_LOGS;
        let mut removed = 0;
        while removed < excess && !self.logs.is_empty() {
            if self.logs[0].created_at < one_hour_ago {
                self.logs.remove(0);
                removed += 1;
            } else {
                break; // Reached recent logs, stop removing
            }
        }
    }
}
```

## Why This Works

1. **Recent large outputs stay:** If someone runs a command that produces 1000 lines of output, all of it stays visible because it all has `created_at = now`
2. **Old outputs get capped:** Only logs older than 1 hour count toward the 500-log limit
3. **No UI freeze:** The output buffer never grows unbounded; recent outputs don't trigger aggressive trimming
4. **Memory is bounded:** Even with 24/7 operation, max memory = ~1 hour of recent logs + 500 older logs worth of strings

## Example Timeline

```
12:00 — 100 recent logs added (1 hour old threshold = 12:00 + 1hr = 13:00)
12:30 — 50 more logs added (mixed: recent + old)
13:05 — 200 more logs added
        * Now we have logs from 12:00-13:05
        * Logs from 12:00-12:05 are now "old" (>1hr)
        * push_log() runs: count old_logs = ~5, which is < 500 → no trim
        * All output visible, UI not frozen
        
14:10 — After 2+ hours of operation, 500+ old logs accumulated
        * New logs from 14:10 arrive
        * push_log() counts: old_logs = 550, excess = 50
        * Removes oldest 50 entries (from 12:00-12:50 range)
        * Recent output (13:10-14:10) still visible
        * Total buffer size stays reasonable (~500 old + N recent)
```

## Testing

After applying changes, verify with:

```bash
cd client-app
cargo check
```

Should compile with no errors (warnings about dead code are pre-existing).

To test the behavior manually:
1. Run CLIENT and navigate to Logs tab
2. Stream a large output (e.g., 500+ line sync log)
3. Observe that UI remains responsive (no truncation)
4. Wait 1 hour, stream more logs
5. Older logs are trimmed, newer output stays visible
