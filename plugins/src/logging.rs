use crate::codec::JsonCodec;
use futures::SinkExt;
use serde::Serialize;
use std::sync::Arc;
use tokio::io::AsyncWrite;
use tokio::sync::Mutex;
use tokio_util::codec::FramedWrite;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
struct LogEntry {
    level: LogLevel,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl From<tracing::Level> for LogLevel {
    fn from(lvl: tracing::Level) -> Self {
        match lvl {
            tracing::Level::ERROR => LogLevel::Error,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::DEBUG | _ => LogLevel::Debug,
        }
    }
}

/// A simple logger that just wraps log entries in a JSON-RPC
/// notification and delivers it to `lightningd`.
struct PluginMakeWriter {
    // An unbounded mpsc channel we can use to talk to the
    // flusher. This avoids having circular locking dependencies if we
    // happen to emit a log record while holding the lock on the
    // plugin connection.
    sender: tokio::sync::mpsc::UnboundedSender<LogEntry>,
}

struct PluginWriter {
    // An unbounded mpsc channel we can use to talk to the
    // flusher. This avoids having circular locking dependencies if we
    // happen to emit a log record while holding the lock on the
    // plugin connection.
    sender: tokio::sync::mpsc::UnboundedSender<LogEntry>,
    buf: Vec<u8>,
    level: LogLevel,
}

impl std::io::Write for PluginWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::Write::flush(&mut self.buf)
    }
}

impl Drop for PluginWriter {
    fn drop(&mut self) {
        let mut message = String::from_utf8(std::mem::take(&mut self.buf))
            .expect("tracing subscriber always writes UTF-8 data");

        let len = message.trim_end().len();
        message.truncate(len);
        self.sender
            .send(LogEntry {
                level: self.level.clone(),
                message,
            })
            .unwrap();
    }
}

use tracing::Metadata;
impl<'a> MakeWriter<'a> for PluginMakeWriter {
    type Writer = PluginWriter;

    // this should never be called if make_writer_for is implemented?
    fn make_writer(&'a self) -> Self::Writer {
        PluginWriter {
            sender: self.sender.clone(),
            buf: Vec::new(),
            level: LogLevel::Info,
        }
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        PluginWriter {
            sender: self.sender.clone(),
            buf: Vec::new(),
            level: (*meta.level()).into(),
        }
    }
}

/// Initialize the logger starting a flusher to the passed in sink.
pub async fn init<O>(
    out: Arc<Mutex<FramedWrite<O, JsonCodec>>>,
) -> Result<(), tracing_subscriber::util::TryInitError>
where
    O: AsyncWrite + Send + Unpin + 'static,
{
    let out = out.clone();

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<LogEntry>();
    tokio::spawn(async move {
        while let Some(i) = receiver.recv().await {
            // We continue draining the queue, even if we get some
            // errors when forwarding. Forwarding could break due to
            // an interrupted connection or stdout being closed, but
            // keeping the messages in the queue is a memory leak.
            let payload = json!({
                "jsonrpc": "2.0",
                "method": "log",
                "params": i
            });

            let _ = out.lock().await.send(payload).await;
        }
    });
    tracing_subscriber::fmt()
        // time and level is logged already.
        .without_time()
        .with_level(false)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_env("CLN_PLUGIN_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(PluginMakeWriter { sender })
        .finish()
        .try_init()
}
