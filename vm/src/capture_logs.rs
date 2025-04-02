use tracing_subscriber::{fmt, layer::SubscriberExt, registry::Registry};
use tracing::dispatcher::with_default;

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

struct SharedWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Captures all logs emitted inside the given closure and returns them as a string
pub fn capture_logs<F, R>(func: F) -> (R, String)
where
    F: FnOnce() -> R,
{
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = {
        let buf = Arc::clone(&buffer);
        move || SharedWriter { buffer: Arc::clone(&buf) }
    };

    let layer = fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .without_time();

    let subscriber = Registry::default().with(layer);

    let dispatch = tracing::Dispatch::new(subscriber);
    let result = with_default(&dispatch, || func());

    let logs = buffer.lock().unwrap();
    let log_str = String::from_utf8(logs.clone()).unwrap_or_default();

    (result, log_str)
}