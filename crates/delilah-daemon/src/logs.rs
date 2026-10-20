//! The daemon's recent log lines, kept in memory for the app's log viewer (Help ›
//! Ver logs): what the daemon printed, without colours, newest last.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

use axum::extract::Query;
use serde::Deserialize;
use tracing_subscriber::fmt::MakeWriter;

const KEPT: usize = 3000;

#[derive(Clone, Default)]
pub struct LogBuffer {
    lines: Arc<Mutex<VecDeque<String>>>,
}

static BUFFER: OnceLock<LogBuffer> = OnceLock::new();

/// The buffer the daemon's logging writes to (see `main.rs`).
pub fn buffer() -> LogBuffer {
    BUFFER.get_or_init(LogBuffer::default).clone()
}

impl LogBuffer {
    pub fn tail(&self, n: usize) -> Vec<String> {
        let lines = self.lines.lock().unwrap();
        lines
            .iter()
            .skip(lines.len().saturating_sub(n))
            .cloned()
            .collect()
    }
}

impl Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        let mut lines = self.lines.lock().unwrap();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            if lines.len() == KEPT {
                lines.pop_front();
            }
            lines.push_back(line.to_string());
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for LogBuffer {
    type Writer = LogBuffer;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[derive(Debug, Deserialize)]
pub struct TailQuery {
    pub lines: Option<usize>,
}

/// `GET /api/v1/logs?lines=…`: the last lines (500 by default) as plain text.
pub async fn tail(Query(q): Query<TailQuery>) -> String {
    let mut out = buffer().tail(q.lines.unwrap_or(500).min(KEPT)).join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_lines() {
        let mut buf = LogBuffer::default();
        buf.write_all(b"one\ntwo\n").unwrap();
        buf.write_all(b"three\n").unwrap();
        assert_eq!(buf.tail(2), ["two", "three"]);
    }
}
