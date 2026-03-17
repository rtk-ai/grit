use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomEvent {
    pub event_type: EventType,
    pub agent: String,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    Claimed,
    Released,
    AgentDone,
}

pub struct Room {
    socket_path: PathBuf,
}

impl Room {
    pub fn new(grit_dir: &Path) -> Self {
        Self {
            socket_path: grit_dir.join("room.sock"),
        }
    }

    /// Send an event to the notification server.
    /// The server reads the JSON line and broadcasts it to all watchers.
    pub fn notify(&self, event: &RoomEvent) {
        if !self.socket_path.exists() {
            return;
        }
        if let Ok(mut stream) = UnixStream::connect(&self.socket_path) {
            let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(2)));
            let json = serde_json::to_string(event).unwrap_or_default();
            let _ = writeln!(stream, "{}", json);
            let _ = stream.flush();
        }
    }
}

/// Notification server that listens on a Unix socket and broadcasts events.
///
/// Protocol (newline-delimited JSON over Unix socket):
/// - A connection that sends data within 200ms is a **producer** (e.g. `grit claim`).
///   The server reads one JSON line and broadcasts it to all watchers.
/// - A connection that sends nothing within 200ms is a **watcher** (e.g. `grit watch`).
///   It stays open and receives newline-delimited JSON events.
pub struct NotificationServer {
    socket_path: PathBuf,
}

impl NotificationServer {
    pub fn new(grit_dir: &Path) -> Self {
        Self {
            socket_path: grit_dir.join("room.sock"),
        }
    }

    /// Start the notification listener in a background thread.
    /// Returns immediately. The server runs until the process exits.
    pub fn start(&self) -> anyhow::Result<()> {
        // Remove stale socket
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        let watchers: Arc<Mutex<Vec<UnixStream>>> = Arc::new(Mutex::new(Vec::new()));

        let watchers_ref = watchers.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let watchers = watchers_ref.clone();
                        thread::spawn(move || {
                            handle_connection(stream, watchers);
                        });
                    }
                    Err(e) => {
                        eprintln!("Socket accept error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}

/// Determine if a new connection is a producer or watcher, then act accordingly.
fn handle_connection(stream: UnixStream, watchers: Arc<Mutex<Vec<UnixStream>>>) {
    // Set a short read timeout to distinguish producers from watchers
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(200)));

    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(reader_stream);
    let mut line = String::new();

    match reader.read_line(&mut line) {
        Ok(n) if n > 0 => {
            // This is a producer -- broadcast the message to all watchers
            let line = line.trim().to_string();
            if !line.is_empty() {
                broadcast_to_watchers(&watchers, &line);
            }
            // Producer connection ends here (dropped on function return)
        }
        _ => {
            // No data within timeout -- this is a watcher.
            // Clear read timeout and park the stream in the watchers list.
            // Limit max watchers to prevent resource exhaustion (DoS).
            const MAX_WATCHERS: usize = 128;
            let _ = stream.set_read_timeout(None);
            if let Ok(mut wl) = watchers.lock() {
                if wl.len() < MAX_WATCHERS {
                    wl.push(stream);
                }
            }
        }
    }
}

/// Send a message to every connected watcher, pruning dead connections.
fn broadcast_to_watchers(watchers: &Arc<Mutex<Vec<UnixStream>>>, message: &str) {
    let mut wl = match watchers.lock() {
        Ok(wl) => wl,
        Err(_) => return,
    };

    let mut dead = Vec::new();
    for (i, watcher) in wl.iter_mut().enumerate() {
        let ok = writeln!(watcher, "{}", message).is_ok() && watcher.flush().is_ok();
        if !ok {
            dead.push(i);
        }
    }

    // Remove dead watchers in reverse index order
    for i in dead.into_iter().rev() {
        wl.remove(i);
    }
}
