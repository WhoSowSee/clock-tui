use std::{cell::RefCell, cmp::min, process::Command};

use crate::app::modes::pause::Pause;
use crate::app::TimerContinueMode;
use crate::clock_text::font::bricks::BricksFont;
use crate::clock_text::ClockText;
use chrono::{DateTime, Duration, Local};
use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

use super::{format_duration, play_beep_tone, play_sound, render_centered, DurationFormat};

pub struct Timer {
    pub size: u16,
    pub style: Style,
    pub repeat: bool,
    pub durations: Vec<Duration>,
    pub titles: Vec<String>,
    pub execute: Vec<String>,
    pub bell: bool,
    pub sound: Option<String>,
    continue_mode: Option<TimerContinueMode>,
    auto_quit: bool,
    format: DurationFormat,
    passed: Duration,
    started_at: Option<DateTime<Local>>,
    execute_result: RefCell<Option<String>>,
    last_bell_at: RefCell<Option<DateTime<Local>>>,
    sound_fired: RefCell<bool>,
}

impl Timer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        size: u16,
        style: Style,
        durations: Vec<Duration>,
        titles: Vec<String>,
        repeat: bool,
        format: DurationFormat,
        paused: bool,
        auto_quit: bool,
        continue_mode: Option<TimerContinueMode>,
        execute: Vec<String>,
        bell: bool,
        sound: Option<String>,
    ) -> Self {
        Self {
            size,
            style,
            durations,
            titles,
            repeat,
            execute,
            bell,
            sound,
            continue_mode,
            auto_quit,
            format,
            passed: Duration::zero(),
            started_at: (!paused).then(Local::now),
            execute_result: RefCell::new(None),
            last_bell_at: RefCell::new(None),
            sound_fired: RefCell::new(false),
        }
    }

    fn total_passed(&self) -> Duration {
        if let Some(started_at) = self.started_at {
            self.passed + (Local::now() - started_at)
        } else {
            self.passed
        }
    }

    pub(crate) fn remaining_time(&self) -> (Duration, usize) {
        if self.durations.is_empty() {
            return (Duration::zero(), 0);
        }

        let total_passed = self.total_passed();

        let mut idx = 0;
        let mut next_checkpoint = self.durations[idx];
        while next_checkpoint < total_passed {
            if idx >= self.durations.len() - 1 && !self.repeat {
                break;
            }
            idx = (idx + 1) % self.durations.len();
            next_checkpoint += self.durations[idx];
        }

        (next_checkpoint - total_passed, idx)
    }

    fn ensure_completion_handled(&self, remaining_time: Duration) {
        if remaining_time > Duration::zero() {
            return;
        }

        if self.bell {
            let now = Local::now();
            let should_ring = match *self.last_bell_at.borrow() {
                None => true,
                Some(last) => (now - last) >= Duration::milliseconds(1000),
            };
            if should_ring {
                *self.last_bell_at.borrow_mut() = Some(now);
                let _ = std::io::Write::write(&mut std::io::stdout(), b"\x07");
                let _ = std::io::Write::flush(&mut std::io::stdout());
                play_beep_tone();
            }
        }

        if !*self.sound_fired.borrow() {
            *self.sound_fired.borrow_mut() = true;
            if let Some(ref path) = self.sound {
                play_sound(path);
            }
        }

        if self.execute_result.borrow().is_some() {
            return;
        }
        if self.execute.is_empty() {
            *self.execute_result.borrow_mut() = Some(String::new());
            return;
        }

        let result = execute(&self.execute);
        *self.execute_result.borrow_mut() = Some(result);
    }

    fn display_time(&self, remaining_time: Duration) -> Duration {
        if remaining_time <= Duration::zero() {
            if self.continue_mode.is_some() {
                -remaining_time
            } else {
                Duration::zero()
            }
        } else {
            remaining_time
        }
    }

    fn is_clock_up(&self, remaining_time: Duration) -> bool {
        self.continue_mode.is_some() && remaining_time < Duration::zero()
    }

    fn is_blink_hidden_phase(&self, remaining_time: Duration) -> bool {
        if !self.is_clock_up(remaining_time)
            || !matches!(self.continue_mode, Some(TimerContinueMode::Blink))
        {
            return false;
        }

        Local::now().timestamp_millis().rem_euclid(1000) >= 500
    }

    fn blink_time(time_str: &str) -> String {
        time_str
            .chars()
            .map(|c| if c.is_whitespace() { c } else { ' ' })
            .collect()
    }

    fn footer_text(&self, remaining_time: Duration) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();

        if matches!(self.continue_mode, Some(TimerContinueMode::Text))
            && self.is_clock_up(remaining_time)
        {
            parts.push("TIME AFTER TIMER END".to_string());
        }

        if let Some(result) = self
            .execute_result
            .borrow()
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned()
        {
            parts.push(result);
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" | "))
        }
    }

    pub(crate) fn is_finished(&self) -> bool {
        if !self.auto_quit {
            return false;
        }

        let (remaining_time, _) = self.remaining_time();
        if remaining_time <= Duration::zero() {
            self.ensure_completion_handled(remaining_time);
            return true;
        }

        false
    }
}

fn execute(execute: &[String]) -> String {
    let cmd_str = execute.join(" ");
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", &cmd_str]).output()
    } else {
        Command::new("sh").args(["-c", &cmd_str]).output()
    };
    match output {
        Ok(output) => {
            if !output.status.success() {
                format!("[ERROR] {}", String::from_utf8_lossy(&output.stderr))
            } else {
                format!("[SUCCEED] {}", String::from_utf8_lossy(&output.stdout))
            }
        }
        Err(e) => {
            format!("[FAILED] {}", e)
        }
    }
}

impl Widget for &Timer {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (remaining_time, idx) = self.remaining_time();
        self.ensure_completion_handled(remaining_time);
        let display_time = self.display_time(remaining_time);

        let mut time_str = format_duration(display_time, self.format);
        if self.is_blink_hidden_phase(remaining_time) {
            time_str = Timer::blink_time(&time_str);
        }

        let header = if self.titles.is_empty() {
            None
        } else {
            Some(self.titles[min(idx, self.titles.len() - 1)].clone())
        };

        let font = BricksFont::new(self.size);
        let text = ClockText::new(time_str.to_string(), &font, self.style);

        let footer = if self.is_paused() {
            Some("PAUSED (press <SPACE> to resume)".to_string())
        } else {
            self.footer_text(remaining_time)
        };

        render_centered(area, buf, &text, header, footer);
    }
}

impl Pause for Timer {
    fn is_paused(&self) -> bool {
        self.started_at.is_none()
    }

    fn pause(&mut self) {
        if let Some(started_at) = self.started_at {
            self.passed += Local::now() - started_at;
            self.started_at = None;
        }
    }

    fn resume(&mut self) {
        if self.started_at.is_none() {
            self.started_at = Some(Local::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Timer;
    use crate::app::{modes::DurationFormat, TimerContinueMode};
    use chrono::Duration;
    use ratatui::style::Style;

    fn new_timer(auto_quit: bool, continue_mode: Option<TimerContinueMode>) -> Timer {
        Timer::new(
            1,
            Style::default(),
            vec![Duration::seconds(5)],
            vec![],
            false,
            DurationFormat::HourMinSec,
            true,
            auto_quit,
            continue_mode,
            vec![],
            false,
            None,
        )
    }

    #[test]
    fn is_finished_is_true_at_zero_with_auto_quit() {
        let mut timer = new_timer(true, None);
        timer.passed = Duration::seconds(5);

        assert!(timer.is_finished());
        assert!(timer.execute_result.borrow().is_some());
    }

    #[test]
    fn is_finished_is_false_without_auto_quit() {
        let mut timer = new_timer(false, None);
        timer.passed = Duration::seconds(7);

        assert!(!timer.is_finished());
    }

    #[test]
    fn display_time_stops_at_zero_by_default() {
        let timer = new_timer(false, None);
        assert_eq!(timer.display_time(Duration::seconds(-3)), Duration::zero());
    }

    #[test]
    fn display_time_counts_up_with_continue() {
        let timer = new_timer(false, Some(TimerContinueMode::Blink));
        assert_eq!(
            timer.display_time(Duration::seconds(-3)),
            Duration::seconds(3)
        );
    }

    #[test]
    fn remaining_time_handles_empty_durations() {
        let timer = Timer::new(
            1,
            Style::default(),
            vec![],
            vec![],
            false,
            DurationFormat::HourMinSec,
            true,
            false,
            None,
            vec![],
            false,
            None,
        );

        assert_eq!(timer.remaining_time(), (Duration::zero(), 0));
    }

    #[test]
    fn footer_shows_clock_up_message_in_continue_text_mode() {
        let timer = new_timer(false, Some(TimerContinueMode::Text));
        assert_eq!(
            timer.footer_text(Duration::seconds(-1)),
            Some("TIME AFTER TIMER END".to_string())
        );
    }

    #[test]
    fn footer_hides_clock_up_message_in_default_blink_mode() {
        let timer = new_timer(false, Some(TimerContinueMode::Blink));
        assert_eq!(timer.footer_text(Duration::seconds(-1)), None);
    }

    #[test]
    fn blink_time_hides_numbers_and_separators() {
        assert_eq!(Timer::blink_time("12:34.5"), "       ");
    }
}
