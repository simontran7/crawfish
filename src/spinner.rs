use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const TICK: Duration = Duration::from_millis(80);

const BOLD_BLUE: &str = "\x1b[1;34m";
const RESET: &str = "\x1b[0m";
const ERASE_LINE: &str = "\x1b[2K\r";

/// A single-line terminal spinner reading `⠋ Compiling main`, that animates
/// on its own background thread while the caller does synchronous work, then
/// leaves behind a permanent `Compiling main` status line once stopped — so
/// slow stages get live feedback without the transcript looking any
/// different from a fast one.
///
/// A no-op animation when stdout isn't a terminal (piped output, tests, CI):
/// the status line still prints once, just without escape codes or motion.
pub(crate) struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    verb: &'static str,
    message: String,
}

impl Spinner {
    /// Starts animating `verb message` (e.g. `Compiling main`), redrawing
    /// the next spinner frame every [`TICK`], until [`Spinner::stop`] is
    /// called.
    pub(crate) fn start(verb: &'static str, message: impl Into<String>) -> Self {
        let message = message.into();
        let stop = Arc::new(AtomicBool::new(false));
        let handle = if io::stdout().is_terminal() {
            Some(thread::spawn({
                let stop = Arc::clone(&stop);
                let message = message.clone();
                move || {
                    let mut stdout = io::stdout();
                    let mut frame_idx = 0;
                    while !stop.load(Ordering::Relaxed) {
                        let _ = write!(
                            stdout,
                            "{ERASE_LINE}{BOLD_BLUE}{} {verb}{RESET} {message}",
                            FRAMES[frame_idx],
                        );
                        let _ = stdout.flush();
                        frame_idx = (frame_idx + 1) % FRAMES.len();
                        thread::sleep(TICK);
                    }
                    let _ = stdout.write_all(ERASE_LINE.as_bytes());
                    let _ = stdout.flush();
                }
            }))
        } else {
            None
        };

        Self {
            stop,
            handle,
            verb,
            message,
        }
    }

    /// Stops the animation and prints the permanent `verb message` status
    /// line in its place, so it's safe to print (diagnostics, an `--emit`
    /// dump) immediately after. Safe to call more than once; only the first
    /// call does anything.
    pub(crate) fn stop(&mut self) {
        if self.stop.swap(true, Ordering::Relaxed) {
            return;
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        if io::stdout().is_terminal() {
            println!("{BOLD_BLUE}{}{RESET} {}", self.verb, self.message);
        } else {
            println!("{} {}", self.verb, self.message);
        }
    }
}

impl Drop for Spinner {
    /// Safety net for any return path that forgets to call
    /// [`Spinner::stop`] explicitly.
    fn drop(&mut self) {
        self.stop();
    }
}
