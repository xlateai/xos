//! Line editor + "print above the prompt" for synchronous polling loops (no Python threads/async).

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, IsTerminal, Write};
use std::time::Duration;

/// `read_line(false)` waits this long for the first key event before returning `None` (non-busy poll).
const INPUT_POLL_IDLE: Duration = Duration::from_millis(32);

pub struct LineEditor {
    prompt: String,
    buffer: String,
    raw_enabled: bool,
}

impl LineEditor {
    pub fn new() -> Self {
        Self {
            prompt: ">>> ".to_string(),
            buffer: String::new(),
            raw_enabled: false,
        }
    }

    pub fn enter(&mut self) -> Result<(), String> {
        let stdout = io::stdout();
        if !stdout.is_terminal() {
            return Ok(());
        }
        enable_raw_mode().map_err(|e| e.to_string())?;
        self.raw_enabled = true;
        self.redraw_bottom()?;
        Ok(())
    }

    pub fn set_prompt(&mut self, prompt: String) {
        self.prompt = prompt;
    }

    fn redraw_bottom(&mut self) -> Result<(), String> {
        print!("\r\x1b[K{}{}", self.prompt, self.buffer);
        io::stdout().flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Print a message above the current input line, then redraw prompt + partial input.
    pub fn print_above(&mut self, text: &str) {
        let trim = text.trim_end_matches('\n');
        print!("\r\x1b[K\n{}\n", trim);
        let _ = io::stdout().flush();
        let _ = self.redraw_bottom();
    }

    pub fn read_line(&mut self, wait: bool) -> Result<Option<String>, String> {
        if !io::stdout().is_terminal() {
            if wait {
                return self.read_line_simple_blocking();
            }
            return Ok(None);
        }
        if !self.raw_enabled {
            if wait {
                return self.read_line_simple_blocking();
            }
            return Ok(None);
        }

        if wait {
            loop {
                let ev = event::read().map_err(|e| e.to_string())?;
                if let Some(line) = self.handle_event(ev)? {
                    return Ok(Some(line));
                }
            }
        } else {
            // First wait (up to POLL_IDLE) for input; then drain any further ready events without blocking.
            if event::poll(INPUT_POLL_IDLE).map_err(|e| e.to_string())? {
                loop {
                    let ev = event::read().map_err(|e| e.to_string())?;
                    if let Some(line) = self.handle_event(ev)? {
                        return Ok(Some(line));
                    }
                    if !event::poll(Duration::ZERO).map_err(|e| e.to_string())? {
                        break;
                    }
                }
            }
            Ok(None)
        }
    }

    fn handle_event(&mut self, ev: Event) -> Result<Option<String>, String> {
        if let Event::Key(key) = ev {
            if key.kind == KeyEventKind::Release {
                return Ok(None);
            }
            match key.code {
                KeyCode::Enter => {
                    let line = std::mem::take(&mut self.buffer);
                    print!("\r\n");
                    let _ = io::stdout().flush();
                    self.redraw_bottom()?;
                    return Ok(Some(line));
                }
                KeyCode::Char(c) => {
                    self.buffer.push(c);
                    self.redraw_bottom()?;
                }
                KeyCode::Backspace => {
                    self.buffer.pop();
                    self.redraw_bottom()?;
                }
                KeyCode::Esc => {
                    self.buffer.clear();
                    self.redraw_bottom()?;
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn read_line_simple_blocking(&mut self) -> Result<Option<String>, String> {
        use std::io::BufRead;
        let mut s = String::new();
        let n = io::stdin()
            .lock()
            .read_line(&mut s)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(s.trim_end_matches('\n').to_string()))
    }

    pub fn leave(&mut self) {
        if self.raw_enabled {
            let _ = disable_raw_mode();
            self.raw_enabled = false;
        }
    }
}

impl Drop for LineEditor {
    fn drop(&mut self) {
        self.leave();
    }
}
