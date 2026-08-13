use std::io::{self, Stdout};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub(super) type AppTerminal = Terminal<CrosstermBackend<Stdout>>;

pub(super) struct TerminalSession {
    terminal: AppTerminal,
    keyboard_enhancement: bool,
}

impl TerminalSession {
    pub(super) fn enter() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        let keyboard_enhancement = execute!(
            stdout,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )
        .is_ok();
        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let mut stdout = io::stdout();
                if keyboard_enhancement {
                    let _ = execute!(stdout, PopKeyboardEnhancementFlags);
                }
                let _ = execute!(stdout, Show, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                return Err(error.into());
            }
        };
        Ok(Self {
            terminal,
            keyboard_enhancement,
        })
    }

    pub(super) fn terminal(&mut self) -> &mut AppTerminal {
        &mut self.terminal
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if self.keyboard_enhancement {
            let _ = execute!(self.terminal.backend_mut(), PopKeyboardEnhancementFlags);
        }
        let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}
