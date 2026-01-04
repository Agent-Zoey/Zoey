//! Logging utilities

use once_cell::sync::OnceCell;
use serde::Serialize;
use std::io::Write;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[derive(Clone, Debug, Serialize)]
pub struct LogEvent {
    pub level: String,
    pub target: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub time: String,
}

static LOG_TX: OnceCell<broadcast::Sender<LogEvent>> = OnceCell::new();

pub fn subscribe_logs() -> Option<broadcast::Receiver<LogEvent>> {
    LOG_TX.get().map(|tx| tx.subscribe())
}

struct BroadcastLayer {
    tx: broadcast::Sender<LogEvent>,
}

impl<S> Layer<S> for BroadcastLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        use tracing::field::{Field, Visit};
        struct MsgVisitor {
            msg: String,
        }
        impl Visit for MsgVisitor {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.msg = format!("{:?}", value);
                }
            }
            fn record_str(&mut self, field: &Field, value: &str) {
                if field.name() == "message" {
                    self.msg = value.to_string();
                }
            }
        }
        let mut visitor = MsgVisitor { msg: String::new() };
        event.record(&mut visitor);
        let meta = event.metadata();
        let ev = LogEvent {
            level: meta.level().to_string(),
            target: meta.target().to_string(),
            message: visitor.msg,
            file: meta.file().map(|s| s.to_string()),
            line: meta.line(),
            time: chrono::Utc::now().to_rfc3339(),
        };
        let _ = self.tx.send(ev);
    }
}

/// Logger wrapper for agent-specific logging
#[derive(Clone)]
pub struct Logger {
    namespace: String,
}

impl Logger {
    /// Create a new logger with a namespace
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
        }
    }

    /// Log an info message
    pub fn info(&self, message: &str) {
        info!("[{}] {}", self.namespace, message);
    }

    /// Log a debug message
    pub fn debug(&self, message: &str) {
        debug!("[{}] {}", self.namespace, message);
    }

    /// Log a warning message
    pub fn warn(&self, message: &str) {
        warn!("[{}] {}", self.namespace, message);
    }

    /// Log an error message
    pub fn error(&self, message: &str) {
        error!("[{}] {}", self.namespace, message);
    }

    /// Log a success message (info level with prefix)
    pub fn success(&self, message: &str) {
        info!("[{}] ✓ {}", self.namespace, message);
    }
}

/// Initialize the global logging system
pub fn init_logging() {
    let level = std::env::var("ZOEY_LOG_LEVEL").unwrap_or_else(|_| "trace".to_string());
    let env_filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| level.into());

    let tx = LOG_TX
        .get_or_init(|| {
            let (tx, _rx) = broadcast::channel(1024);
            tx
        })
        .clone();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(tracing_subscriber::fmt::layer().with_writer(LogBufferWriter::default()))
        .with(BroadcastLayer { tx })
        .init();
}

#[derive(Default, Clone)]
struct LogBufferWriter;

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBufferWriter {
    type Writer = LogBuf;
    fn make_writer(&'a self) -> Self::Writer {
        LogBuf::default()
    }
}

#[derive(Default)]
struct LogBuf(Vec<u8>);

impl Write for LogBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_creation() {
        let logger = Logger::new("test");
        assert_eq!(logger.namespace, "test");
    }

    #[test]
    fn test_logger_methods() {
        let logger = Logger::new("test");
        // These won't panic
        logger.info("info message");
        logger.debug("debug message");
        logger.warn("warn message");
        logger.error("error message");
        logger.success("success message");
    }
}
