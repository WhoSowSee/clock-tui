use std::error::Error;
use std::io::{self, Write};
use std::time::Duration as StdDuration;

use chrono::Duration as ChronoDuration;
use clap::Parser;
use clock_tui::app::keymap::layout_aware;
use clock_tui::app::{App, Mode};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

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

                let modifiers = key.modifiers;
                let key = layout_aware(key.code);
                match key {
                    KeyCode::Char('q') if modifiers.is_empty() => break,
                    KeyCode::Char('c') if modifiers.is_empty() => {
                        app.set_mode_if_inactive(Mode::Clock {
                            timezone: None,
                            no_date: false,
                            no_seconds: false,
                            millis: false,
                        });
                    }
                    KeyCode::Char('w') if modifiers.is_empty() => {
                        app.set_mode_if_inactive(Mode::Stopwatch);
                    }
                    KeyCode::Char('t') if modifiers.is_empty() => {
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
                    _ => app.on_key_with_modifiers(key, modifiers),
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
