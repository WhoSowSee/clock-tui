use std::error::Error;
use std::io::{self, Write};
use std::time::Duration as StdDuration;

use chrono::Duration as ChronoDuration;
use clap::Parser;
use clock_tui::app::keymap::layout_aware;
use clock_tui::app::{App, Mode};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

fn has_supported_modifiers(key: &KeyEvent) -> bool {
    key.modifiers.is_empty()
        || (key.modifiers == KeyModifiers::SHIFT
            && matches!(key.code, KeyCode::Char('+') | KeyCode::Char('=')))
}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse command line arguments
    // Must be done first so `--help` isn't printed to the alternate screen.
    let mut app = App::parse();

    app.validate_sound_on_error_exit();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(&mut stdout);
    let mut terminal = Terminal::new(backend)?;

    // Load config and initialize app
    app.init_app();

    loop {
        if app.is_ended() {
            break;
        }
        terminal.draw(|f| app.ui(f))?;

        if event::poll(StdDuration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('c'))
                {
                    break;
                }

                if !has_supported_modifiers(&key) {
                    continue;
                }

                let key = layout_aware(key.code);
                match key {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') => {
                        app.set_mode_if_inactive(Mode::Clock {
                            timezone: None,
                            no_date: false,
                            no_seconds: false,
                            millis: false,
                        });
                    }
                    KeyCode::Char('w') => {
                        app.set_mode_if_inactive(Mode::Stopwatch);
                    }
                    KeyCode::Char('t') => {
                        app.set_mode_if_inactive(Mode::Timer {
                            durations: vec![ChronoDuration::minutes(5)],
                            titles: vec![],
                            repeat: false,
                            no_millis: false,
                            paused: false,
                            continue_mode: None,
                            auto_quit: false,
                            execute: vec![],
                            bell: false,
                            sound: None,
                        });
                    }
                    _ => app.on_key(key),
                }
            }
        }
    }

    // restore terminal
    terminal.show_cursor()?;
    drop(terminal);
    disable_raw_mode()?;
    stdout.execute(LeaveAlternateScreen)?;

    // Perform logic such as printing the stopwatch time.
    // Must be done after leaving alternate screen.
    app.on_exit();
    io::stdout().flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn shifted_plus_has_supported_modifiers() {
        for code in [KeyCode::Char('+'), KeyCode::Char('=')] {
            let key = KeyEvent::new(code, KeyModifiers::SHIFT);
            assert!(super::has_supported_modifiers(&key));
        }

        let control_plus = KeyEvent::new(KeyCode::Char('+'), KeyModifiers::CONTROL);
        assert!(!super::has_supported_modifiers(&control_plus));
    }
}
