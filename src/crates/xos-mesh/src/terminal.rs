//! Line-buffered stdin for `xos.input` while mesh runs.
//!
//! Uses normal cooked console / TTY line editing (`read_line`), not raw mode or per-key console
//! reads. That matches typical CLI tools and avoids behavior that security products often flag
//! (low-level keyboard capture).

use super::state::INPUT_INTERRUPT_REQUESTED;
use std::io::{self, BufRead, IsTerminal, Write};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

/// Serialize mesh app stdout so the stdin prompt thread and `print_above` do not interleave
/// (which breaks cursor position, especially on Windows Terminal / ConPTY).
static MESH_STDOUT_LOCK: Mutex<()> = Mutex::new(());

/// Returned from [`LineEditor::read_line`] on Ctrl+C; mapped to `KeyboardInterrupt` in `xos.input`.
pub const INPUT_INTERRUPT: &str = "xos:input_interrupt";

fn poll_os_interrupt() -> Result<(), String> {
    if INPUT_INTERRUPT_REQUESTED.swap(false, Ordering::SeqCst) {
        return Err(INPUT_INTERRUPT.to_string());
    }
    Ok(())
}

struct CookedStdin {
    rx: mpsc::Receiver<String>,
    _join: thread::JoinHandle<()>,
}

impl CookedStdin {
    fn spawn(prompt: Arc<Mutex<String>>) -> Self {
        let (tx, rx) = mpsc::channel::<String>();
        let p = Arc::clone(&prompt);
        let join = thread::spawn(move || Self::thread_main(p, tx));
        Self { rx, _join: join }
    }

    fn thread_main(prompt: Arc<Mutex<String>>, tx: mpsc::Sender<String>) {
        loop {
            let pr = match prompt.lock() {
                Ok(g) => g.clone(),
                Err(_) => break,
            };
            {
                let _lock = MESH_STDOUT_LOCK.lock().unwrap();
                print!("{}", pr);
                let _ = io::stdout().flush();
            }
            let mut line = String::new();
            match io::stdin().lock().read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if tx.send(line.trim_end().to_string()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }
}

pub struct LineEditor {
    /// Shared with the stdin helper thread for `set_prompt`.
    prompt: Arc<Mutex<String>>,
    /// Lazily started on first `read_line` so Python can print a banner before the first prompt.
    cooked: Option<CookedStdin>,
}

impl LineEditor {
    pub fn new() -> Self {
        Self {
            prompt: Arc::new(Mutex::new(">>> ".to_string())),
            cooked: None,
        }
    }

    /// Prepares for input; does not touch raw mode or install keyboard hooks.
    pub fn enter(&mut self) -> Result<(), String> {
        Ok(())
    }

    pub fn set_prompt(&mut self, prompt: String) {
        if let Ok(mut g) = self.prompt.lock() {
            *g = prompt;
        }
    }

    fn prompt_str(&self) -> String {
        self.prompt.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn read_line(&mut self, wait: bool) -> Result<Option<String>, String> {
        poll_os_interrupt()?;

        if io::stdout().is_terminal() && io::stdin().is_terminal() {
            if self.cooked.is_none() {
                self.cooked = Some(CookedStdin::spawn(Arc::clone(&self.prompt)));
            }
            if let Some(ref c) = self.cooked {
                return if wait {
                    Ok(Some(
                        c.rx.recv()
                            .map_err(|_| "stdin channel closed".to_string())?,
                    ))
                } else {
                    Ok(c.rx.try_recv().ok())
                };
            }
        }

        if wait {
            self.read_line_simple_blocking()
        } else {
            Ok(None)
        }
    }

    fn read_line_simple_blocking(&mut self) -> Result<Option<String>, String> {
        let pr = self.prompt_str();
        {
            let _lock = MESH_STDOUT_LOCK.lock().unwrap();
            print!("{}", pr);
            let _ = io::stdout().flush();
        }
        let mut s = String::new();
        let n = io::stdin()
            .lock()
            .read_line(&mut s)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(s.trim_end().to_string()))
    }

    /// Print a message above the current input line (best-effort without raw-mode redraw).
    ///
    /// Re-prints the current prompt after the message so the caret stays on a known line; the stdin
    /// thread may still have printed an earlier prompt — without a full-screen redraw this cannot be
    /// perfect, but locking + prompt repeat fixes the worst cursor jumps on Windows.
    pub fn print_above(&mut self, text: &str) {
        let trim = text.trim_end_matches('\n');
        let pr = self.prompt_str();
        let _lock = MESH_STDOUT_LOCK.lock().unwrap();
        print!("\n{}\n{}", trim, pr);
        let _ = io::stdout().flush();
    }

    pub fn leave(&mut self) {
        self.cooked = None;
    }
}

impl Drop for LineEditor {
    fn drop(&mut self) {
        self.leave();
    }
}
