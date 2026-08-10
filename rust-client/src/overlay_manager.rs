use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;

use crate::overlay_ipc::{OverlayEvent, OverlayState};
use std::sync::mpsc::{channel, Receiver};
use log::{error, info, warn};

pub struct OverlayManager {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    event_rx: Option<Receiver<OverlayEvent>>,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self {
            child: None,
            stdin: None,
            event_rx: None,
        }
    }

    pub fn start(&mut self) {
        if self.child.is_some() {
            return;
        }

        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                error!("Failed to get current executable path for overlay: {}", e);
                return;
            }
        };

        info!("Starting overlay process...");
        let mut child = match Command::new(exe)
            .arg("--overlay")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // forward stderr for debugging
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                error!("Failed to spawn overlay process: {}", e);
                return;
            }
        };

        let stdin = child.stdin.take().expect("Failed to open stdin for overlay");
        let stdout = child.stdout.take().expect("Failed to open stdout for overlay");

        self.child = Some(child);
        self.stdin = Some(stdin);

        let (tx, rx) = channel();
        self.event_rx = Some(rx);

        // Read stdout for events
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line_str) => {
                        if let Ok(event) = serde_json::from_str::<OverlayEvent>(&line_str) {
                            let _ = tx.send(event);
                        }
                    }
                    Err(_) => break, // Process died
                }
            }
        });
    }

    pub fn stop(&mut self) {
        if let Some(mut stdin) = self.stdin.take() {
            let _ = writeln!(stdin, "HIDE");
            let _ = stdin.flush();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            info!("Overlay process stopped.");
        }
        self.event_rx = None;
    }

    pub fn send_state(&mut self, state: &OverlayState) {
        if let Some(stdin) = &mut self.stdin {
            if let Ok(json) = serde_json::to_string(state) {
                if let Err(e) = writeln!(stdin, "{}", json) {
                    warn!("Failed to write to overlay stdin: {}", e);
                    // Child probably died, clean up
                    self.stop();
                }
            }
        }
    }

    pub fn poll_events(&mut self) -> Vec<OverlayEvent> {
        let mut events = Vec::new();
        if let Some(rx) = &self.event_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        events
    }
}

impl Drop for OverlayManager {
    fn drop(&mut self) {
        self.stop();
    }
}
