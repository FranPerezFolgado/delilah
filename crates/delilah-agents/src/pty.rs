//! A process running in a pseudo-terminal, owned by the daemon. Its output is kept as
//! scrollback, appended to a log file and broadcast live to every attached client.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::{broadcast, watch};

/// Bytes of output kept in memory for clients that attach later.
const SCROLLBACK_BYTES: usize = 512 * 1024;
/// Chunks a slow client may fall behind before it starts missing output.
const LIVE_BUFFER_CHUNKS: usize = 1024;

pub type Chunk = Arc<[u8]>;

pub struct PtySession {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    output: Mutex<Output>,
    exited: watch::Receiver<bool>,
    /// Columns and rows: one size for everyone, set by whoever is driving it.
    size: watch::Sender<(u16, u16)>,
}

struct Output {
    scrollback: VecDeque<u8>,
    live: broadcast::Sender<Chunk>,
}

impl PtySession {
    pub fn spawn(
        cmd: CommandBuilder,
        cols: u16,
        rows: u16,
        log: Option<PathBuf>,
    ) -> std::io::Result<Arc<Self>> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(std::io::Error::other)?;
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(std::io::Error::other)?;
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(std::io::Error::other)?;
        let writer = pair.master.take_writer().map_err(std::io::Error::other)?;
        let (exited_tx, exited) = watch::channel(false);
        let session = Arc::new(Self {
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            child: Mutex::new(child),
            output: Mutex::new(Output {
                scrollback: VecDeque::new(),
                live: broadcast::channel(LIVE_BUFFER_CHUNKS).0,
            }),
            exited,
            size: watch::channel((cols, rows)).0,
        });

        let log_file = match log {
            Some(path) => Some(
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)?,
            ),
            None => None,
        };
        let pump = Arc::clone(&session);
        thread::spawn(move || pump.pump_output(reader, log_file, exited_tx));
        Ok(session)
    }

    fn pump_output(
        &self,
        mut reader: Box<dyn Read + Send>,
        mut log: Option<std::fs::File>,
        exited: watch::Sender<bool>,
    ) {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk: Chunk = Arc::from(&buf[..n]);
                    {
                        let mut out = self.output.lock().unwrap();
                        out.scrollback.extend(chunk.iter());
                        let excess = out.scrollback.len().saturating_sub(SCROLLBACK_BYTES);
                        out.scrollback.drain(..excess);
                        // No receivers is fine: nobody is attached right now.
                        let _ = out.live.send(Arc::clone(&chunk));
                    }
                    if let Some(file) = log.as_mut() {
                        let _ = file.write_all(&chunk);
                    }
                }
            }
        }
        let _ = self.child.lock().unwrap().wait();
        let _ = exited.send(true);
    }

    /// The scrollback so far plus a live feed of what comes next, with no gap or overlap
    /// between them.
    pub fn attach(&self) -> (Vec<u8>, broadcast::Receiver<Chunk>) {
        let out = self.output.lock().unwrap();
        (
            out.scrollback.iter().copied().collect(),
            out.live.subscribe(),
        )
    }

    pub fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(bytes)?;
        writer.flush()
    }

    pub fn resize(&self, cols: u16, rows: u16) -> std::io::Result<()> {
        if *self.size.borrow() == (cols, rows) {
            return Ok(());
        }
        self.master
            .lock()
            .unwrap()
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(std::io::Error::other)?;
        self.size.send_replace((cols, rows));
        Ok(())
    }

    pub fn size(&self) -> (u16, u16) {
        *self.size.borrow()
    }

    /// Fires whenever the size changes, so every viewer can draw at the same one.
    pub fn watch_size(&self) -> watch::Receiver<(u16, u16)> {
        self.size.subscribe()
    }

    pub fn kill(&self) -> std::io::Result<()> {
        self.child.lock().unwrap().kill()
    }

    pub fn has_exited(&self) -> bool {
        *self.exited.borrow()
    }

    /// Resolves once the process has exited and all its output has been read.
    pub async fn wait(&self) {
        let mut exited = self.exited.clone();
        let _ = exited.wait_for(|done| *done).await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn bash(script: &str) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("bash");
        cmd.args(["--noprofile", "--norc", "-c", script]);
        cmd
    }

    async fn read_until(rx: &mut broadcast::Receiver<Chunk>, seen: &mut Vec<u8>, needle: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !String::from_utf8_lossy(seen).contains(needle) {
                seen.extend_from_slice(&rx.recv().await.unwrap());
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "never saw {needle:?} in {:?}",
                String::from_utf8_lossy(seen)
            )
        });
    }

    #[tokio::test]
    async fn streams_output_and_takes_input() {
        let session =
            PtySession::spawn(bash("echo ready; read line; echo got:$line"), 80, 24, None).unwrap();
        let (mut seen, mut rx) = session.attach();

        read_until(&mut rx, &mut seen, "ready").await;
        session.write(b"hola\r").unwrap();
        read_until(&mut rx, &mut seen, "got:hola").await;

        tokio::time::timeout(Duration::from_secs(5), session.wait())
            .await
            .unwrap();
        assert!(session.has_exited());
    }

    #[tokio::test]
    async fn late_clients_get_the_scrollback() {
        let dir = std::env::temp_dir().join(format!("delilah-pty-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("terminal.log");

        let session =
            PtySession::spawn(bash("echo first; echo second"), 80, 24, Some(log.clone())).unwrap();
        tokio::time::timeout(Duration::from_secs(5), session.wait())
            .await
            .unwrap();

        let (scrollback, _) = session.attach();
        let text = String::from_utf8_lossy(&scrollback);
        assert!(
            text.contains("first") && text.contains("second"),
            "{text:?}"
        );
        assert!(std::fs::read_to_string(&log).unwrap().contains("second"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn shares_its_size_with_every_viewer() {
        let session = PtySession::spawn(bash("sleep 5"), 80, 24, None).unwrap();
        let mut sizes = session.watch_size();
        session.resize(120, 40).unwrap();
        sizes.changed().await.unwrap();
        assert_eq!(*sizes.borrow(), (120, 40));
        assert_eq!(session.size(), (120, 40));
        session.kill().unwrap();
    }

    #[tokio::test]
    async fn can_be_killed() {
        let session = PtySession::spawn(bash("sleep 30"), 80, 24, None).unwrap();
        session.kill().unwrap();
        tokio::time::timeout(Duration::from_secs(5), session.wait())
            .await
            .unwrap();
    }
}
