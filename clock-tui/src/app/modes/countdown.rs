use std::cell::RefCell;

use crate::clock_text::font::bricks::BricksFont;
use crate::clock_text::ClockText;
use chrono::{DateTime, Duration, Local};
use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

use super::{format_duration, play_beep_tone, play_sound, render_centered, DurationFormat};

pub struct Countdown {
    pub size: u16,
    pub style: Style,
    pub time: DateTime<Local>,
    pub title: Option<String>,
    pub continue_on_zero: bool,
    pub(crate) reverse: bool,
    pub(crate) format: DurationFormat,
    pub bell: bool,
    pub sound: Option<String>,
    initial_remaining: Duration,
    last_bell_at: RefCell<Option<DateTime<Local>>>,
    sound_fired: RefCell<bool>,
}

impl Countdown {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        size: u16,
        style: Style,
        time: DateTime<Local>,
        title: Option<String>,
        continue_on_zero: bool,
        reverse: bool,
        format: DurationFormat,
        bell: bool,
        sound: Option<String>,
    ) -> Self {
        let now = Local::now();
        let initial_remaining = time.signed_duration_since(now);
        Self {
            size,
            style,
            time,
            title,
            continue_on_zero,
            reverse,
            format,
            bell,
            sound,
            initial_remaining: if initial_remaining < Duration::zero() {
                Duration::zero()
            } else {
                initial_remaining
            },
            last_bell_at: RefCell::new(None),
            sound_fired: RefCell::new(false),
        }
    }

    pub(crate) fn remaining_time(&self) -> Duration {
        self.time.signed_duration_since(Local::now())
    }

    fn ensure_completion_handled(&self, remaining_time: Duration) {
        if remaining_time > Duration::zero() {
            return;
        }

        // Repeated bell
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
    }
}

impl Widget for &Countdown {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let remaining_time = self.remaining_time();
        self.ensure_completion_handled(remaining_time);

        let display_time = if self.reverse {
            let elapsed = self.initial_remaining - remaining_time;
            if elapsed < Duration::zero() {
                Duration::zero()
            } else if !self.continue_on_zero && remaining_time < Duration::zero() {
                self.initial_remaining
            } else {
                elapsed
            }
        } else if remaining_time < Duration::zero() && !self.continue_on_zero {
            if remaining_time.num_milliseconds().abs() % 1000 < 500 {
                let font = BricksFont::new(self.size);
                let text = ClockText::new(String::new(), &font, self.style);
                render_centered(area, buf, &text, self.title.to_owned(), None);
                return;
            } else {
                Duration::zero()
            }
        } else {
            remaining_time
        };

        let time_str = format_duration(display_time, self.format);
        let font = BricksFont::new(self.size);
        let text = ClockText::new(time_str, &font, self.style);
        render_centered(area, buf, &text, self.title.to_owned(), None);
    }
}
