//! Terminal session handling: taking over the screen, giving it back, and
//! restoring it even on a panic.

use std::io::{self, Write};

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        cursor,
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};

/// Backend used to draw on stdout.
pub(crate) type Backend = CrosstermBackend<io::Stdout>;

/// The terminal the interface draws on.
pub(crate) struct Session {
    /// Terminal used to render frames.
    pub(crate) terminal: Terminal<Backend>,
    active: bool,
}

impl Session {
    /// Takes over the terminal: raw mode, alternate screen, mouse capture,
    /// hidden cursor.
    pub(crate) fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let taken = execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            cursor::Hide
        );
        let terminal = taken.and_then(|()| Terminal::new(CrosstermBackend::new(io::stdout())));
        match terminal {
            Ok(terminal) => {
                Ok(Self {
                    terminal,
                    active: true,
                })
            }
            Err(err) => {
                // Raw mode and the screen were already partly taken
                // over, so they go back before the failure escapes.
                let _ = restore();
                Err(err)
            }
        }
    }

    /// Returns the terminal to its original state. Safe to call twice.
    pub(crate) fn leave(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        let restored = restore();
        // A failed restore keeps the session active, so Drop retries
        // instead of the failure ending the attempts.
        if restored.is_ok() {
            self.active = false;
        }
        restored
    }

    /// Returns the terminal to its original state so another program can use it.
    pub(crate) fn suspend(&mut self) -> io::Result<()> {
        self.leave()
    }

    /// Takes the terminal back after [`Session::suspend`] and repaints it.
    pub(crate) fn resume(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let taken = execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            cursor::Hide
        );
        let repainted = taken.and_then(|()| self.terminal.clear());
        match repainted {
            Ok(()) => {
                self.active = true;
                Ok(())
            }
            Err(err) => {
                // The terminal is half-taken and only this session could
                // restore it, so it goes back before the failure
                // escapes; like leave(), a failed restore stays active
                // for Drop to retry.
                if restore().is_err() {
                    self.active = true;
                }
                Err(err)
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}

/// Restores the terminal from inside the panic handler before the regular
/// panic message is printed.
pub(crate) fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        previous(info);
    }));
}

/// Prints a notice and waits for the user to press Enter after a suspended
/// program has finished, so its output stays readable.
pub(crate) fn wait_for_enter() {
    print!("\nPress Enter to return to soar");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
}

fn restore() -> io::Result<()> {
    let result = execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        cursor::Show
    );
    disable_raw_mode()?;
    result
}
