use std::io::{self, stdout};
use std::sync::Once;

use crossterm::cursor::Show;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::{DefaultTerminal, Terminal};

static PANIC_HOOK: Once = Once::new();

pub struct TerminalSession {
    terminal: DefaultTerminal,
    active: bool,
    mouse_capture: bool,
}

impl TerminalSession {
    pub fn enter() -> io::Result<Self> {
        install_panic_hook();
        Ok(Self {
            terminal: enter_terminal()?,
            active: true,
            mouse_capture: false,
        })
    }

    pub fn terminal(&mut self) -> &mut DefaultTerminal {
        &mut self.terminal
    }

    pub fn suspend(&mut self) -> io::Result<()> {
        if self.active {
            restore_terminal(self.mouse_capture)?;
            self.active = false;
        }
        Ok(())
    }

    pub fn resume(&mut self) -> io::Result<()> {
        if !self.active {
            self.terminal = enter_terminal()?;
            self.active = true;
            if self.mouse_capture
                && let Err(error) = execute!(stdout(), EnableMouseCapture)
            {
                let _ = restore_terminal(false);
                self.active = false;
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn enable_mouse_capture(&mut self) -> io::Result<()> {
        if !self.mouse_capture {
            execute!(stdout(), EnableMouseCapture)?;
            self.mouse_capture = true;
        }
        Ok(())
    }

    pub fn disable_mouse_capture(&mut self) -> io::Result<()> {
        if self.mouse_capture {
            execute!(stdout(), DisableMouseCapture)?;
            self.mouse_capture = false;
        }
        Ok(())
    }

    pub fn with_suspended<T>(
        &mut self,
        operation: impl FnOnce() -> io::Result<T>,
    ) -> io::Result<T> {
        self.suspend()?;
        let operation_result = operation();
        let resume_result = self.resume();
        match (operation_result, resume_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), Ok(())) | (_, Err(error)) => Err(error),
        }
    }

    pub fn restore(&mut self) -> io::Result<()> {
        if self.active {
            let result = restore_terminal(self.mouse_capture);
            self.active = false;
            result
        } else {
            Ok(())
        }
    }

    #[cfg(unix)]
    pub fn suspend_process(&mut self) -> io::Result<()> {
        self.suspend()?;
        // SIGSTOP remains effective for PTY children whose process group is orphaned, while
        // interactive shells still report and resume it as a normal stopped job.
        let signal_result = unsafe { libc::raise(libc::SIGSTOP) };
        if signal_result != 0 {
            let error = io::Error::last_os_error();
            let _ = self.resume();
            return Err(error);
        }
        self.resume()
    }

    #[cfg(windows)]
    pub fn suspend_process(&mut self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "suspend is not supported by this console",
        ))
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn enter_terminal() -> io::Result<DefaultTerminal> {
    enable_raw_mode()?;
    if let Err(error) = execute!(stdout(), EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(error);
    }
    let backend = CrosstermBackend::new(stdout());
    match Terminal::new(backend) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let _ = restore_terminal(false);
            Err(error)
        }
    }
}

fn restore_terminal(mouse_capture: bool) -> io::Result<()> {
    let mouse_result = if mouse_capture {
        execute!(stdout(), DisableMouseCapture)
    } else {
        Ok(())
    };
    let raw_result = disable_raw_mode();
    let screen_result = execute!(stdout(), LeaveAlternateScreen, Show);
    mouse_result.and(raw_result).and(screen_result)
}

fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = restore_terminal(true);
            previous(info);
        }));
    });
}
