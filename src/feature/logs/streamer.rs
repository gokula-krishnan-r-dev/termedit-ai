//! Async log streamer — tails a file and emits `LogEntry` batches over an mpsc channel.
//!
//! Architecture:
//!   - tokio task owns the file handle and the `notify` watcher.
//!   - On file-change events (or fallback poll) it reads newly appended bytes,
//!     splits into lines, parses each, and sends a `Vec<LogEntry>` batch.
//!   - A `StreamControl` enum lets the TUI pause/resume/stop streaming.

use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::sync::mpsc;

use super::config::LogsConfig;
use super::parser::{parse_line, LogEntry};

// ─── Control messages ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum StreamControl {
    Pause,
    Resume,
    Stop,
}

// ─── Streamer ────────────────────────────────────────────────────────────────

/// Spawn the async tail task.
///
/// Returns `(control_tx, entries_rx)` where:
/// - `control_tx` lets the UI send `Pause` / `Resume` / `Stop`.
/// - `entries_rx` receives `Vec<LogEntry>` batches parsed from new lines.
pub fn spawn(
    path: PathBuf,
    from_start: bool,
    cfg: LogsConfig,
) -> (mpsc::Sender<StreamControl>, mpsc::Receiver<Vec<LogEntry>>) {
    let (ctrl_tx, ctrl_rx) = mpsc::channel::<StreamControl>(16);
    let (batch_tx, batch_rx) = mpsc::channel::<Vec<LogEntry>>(256);

    tokio::spawn(async move {
        if let Err(e) = tail_loop(&path, from_start, cfg, ctrl_rx, batch_tx).await {
            eprintln!("[logs/streamer] error: {}", e);
        }
    });

    (ctrl_tx, batch_rx)
}

// ─── Core tail loop ───────────────────────────────────────────────────────────

async fn tail_loop(
    path: &Path,
    from_start: bool,
    cfg: LogsConfig,
    mut ctrl_rx: mpsc::Receiver<StreamControl>,
    batch_tx: mpsc::Sender<Vec<LogEntry>>,
) -> anyhow::Result<()> {
    let file = File::open(path).await?;
    let mut reader = BufReader::new(file);

    // If tailing, seek to end first.
    if !from_start {
        let len = reader.get_ref().metadata().await?.len();
        if len > 0 {
            reader.seek(std::io::SeekFrom::Start(len)).await?;
        }
    }

    let mut line_no: u64 = 0;
    let mut paused = false;

    // Try to set up a filesystem watcher via notify.
    // We use a simple poll+check approach for cross-platform compatibility
    // (notify's async API differs across versions).
    let poll_interval = Duration::from_millis(cfg.tail_poll_ms);

    loop {
        // Drain control messages (non-blocking).
        loop {
            match ctrl_rx.try_recv() {
                Ok(StreamControl::Pause)  => { paused = true; }
                Ok(StreamControl::Resume) => { paused = false; }
                Ok(StreamControl::Stop)   => { return Ok(()); }
                Err(_) => break,
            }
        }

        if !paused {
            let mut batch: Vec<LogEntry> = Vec::new();

            // Read all currently available lines.
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF — wait for more data
                    Ok(_) => {
                        let raw = line.trim_end_matches('\n').trim_end_matches('\r');
                        if !raw.is_empty() {
                            line_no += 1;
                            batch.push(parse_line(raw, line_no));
                        }
                    }
                    Err(e) => {
                        eprintln!("[logs/streamer] read error: {}", e);
                        break;
                    }
                }
            }

            if !batch.is_empty() {
                // Best-effort send; if the receiver is gone, stop.
                if batch_tx.send(batch).await.is_err() {
                    return Ok(());
                }
            }
        }

        // Sleep or wait for fs event.
        tokio::select! {
            // Respect control messages during sleep.
            msg = ctrl_rx.recv() => {
                match msg {
                    Some(StreamControl::Pause)  => { paused = true; }
                    Some(StreamControl::Resume) => { paused = false; }
                    Some(StreamControl::Stop) | None => { return Ok(()); }
                }
            }
            _ = tokio::time::sleep(poll_interval) => {}
        }
    }
}

// ─── Tokio runtime helper ─────────────────────────────────────────────────────

/// Run the given async block on a dedicated tokio runtime.
/// Used by the synchronous `run()` entry-point.
pub fn build_runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| anyhow::anyhow!("tokio runtime: {}", e))
}
