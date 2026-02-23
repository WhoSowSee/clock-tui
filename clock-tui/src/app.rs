use chrono::DateTime;
use chrono::Duration;
use chrono::Local;
use chrono::NaiveDate;
use chrono::NaiveDateTime;
use chrono::NaiveTime;
use chrono::TimeZone;
use chrono_tz::Tz;
use clap::Subcommand;
use crossterm::event::KeyCode;
use ratatui::{
    style::{Color, Style},
    Frame,
};
use regex::Regex;

use self::modes::Clock;
use self::modes::Countdown;
use self::modes::DurationFormat;
use self::modes::Pause;
use self::modes::Stopwatch;
use self::modes::Timer;

pub mod modes;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TimerContinueMode {
    Blink,
    Text,
}

#[derive(Debug, Subcommand)]
pub enum Mode {
    /// The clock mode displays the current time, the default mode.
    Clock {
        /// Custome timezone, for example "America/New_York", use local timezone if not specificed
        #[clap(short = 'z', long, value_parser=parse_timezone)]
        timezone: Option<Tz>,
        /// Do not show date
        #[clap(short = 'D', long, action)]
        no_date: bool,
        /// Do not show seconds
        #[clap(short = 'S', long, action)]
        no_seconds: bool,
        /// Show milliseconds
        #[clap(short, long, action)]
        millis: bool,
    },
    /// The timer mode displays the remaining time until the timer is finished.
    Timer {
        /// Initial duration for timer, value can be 10s, 1m, 7m30s, 3h52m12s, etc.
        /// Also accept mulitple duration value and run the timers sequentially, eg. 25m 5m
        #[clap(short, long="duration", value_parser = parse_duration, num_args = 1.., default_value = "5m")]
        durations: Vec<Duration>,

        /// Set the title for the timer, also accept mulitple titles for each durations correspondingly
        #[clap(short, long = "title", num_args = 0..)]
        titles: Vec<String>,

        /// Restart the timer when timer is over
        #[clap(long, short, action)]
        repeat: bool,

        /// Hide milliseconds
        #[clap(long = "no-millis", short = 'M', action)]
        no_millis: bool,

        /// Start the timer paused
        #[clap(long = "paused", short = 'P', action)]
        paused: bool,

        /// Continue counting up after reaching zero
        #[clap(long = "continue", short = 'C', value_name = "MODE", value_enum)]
        continue_mode: Option<TimerContinueMode>,

        /// Auto quit when time is up
        #[clap(long = "quit", short = 'Q', action)]
        auto_quit: bool,

        /// Command to run when the timer ends
        #[clap(long, short, num_args = 1.., allow_hyphen_values = true)]
        execute: Vec<String>,
    },
    /// The stopwatch mode displays the elapsed time since it was started.
    Stopwatch,
    /// The countdown timer mode shows the duration to a specific time
    Countdown {
        /// The target time to countdown to, eg. "2023-01-01", "20:00", "2022-12-25 20:00:00" or "2022-12-25T20:00:00-04:00"
        #[clap(long, short, value_parser = parse_datetime)]
        time: DateTime<Local>,

        /// Title or description for countdown show in header
        #[clap(long, short = 'T')]
        title: Option<String>,

        /// Continue to countdown after pass the target time
        #[clap(long = "continue", short = 'c', action)]
        continue_on_zero: bool,

        /// Reverse the countdown, a.k.a. countup
        #[clap(long, short, action)]
        reverse: bool,

        /// Show milliseconds
        #[clap(short, long, action)]
        millis: bool,
    },
}

use crate::config::{Config, TimerConfig};

#[derive(clap::Parser, Default)]
#[clap(name = "tclock", about = "A clock app in terminal", long_about = None)]
pub struct App {
    #[clap(subcommand)]
    pub mode: Option<Mode>,
    /// Foreground color of the clock, possible values are:
    ///     a) Any one of: Black, Red, Green, Yellow, Blue, Magenta, Cyan, Gray, DarkGray, LightRed, LightGreen, LightYellow, LightBlue, LightMagenta, LightCyan, White.
    ///     b) Hexadecimal color code: #RRGGBB.
    #[clap(short, long, value_parser = parse_color)]
    pub color: Option<Color>,
    /// Size of the clock, should be a positive integer (>=1).
    #[clap(short, long, value_parser)]
    pub size: Option<u16>,

    #[clap(skip)]
    clock: Option<Clock>,
    #[clap(skip)]
    timer: Option<Timer>,
    #[clap(skip)]
    stopwatch: Option<Stopwatch>,
    #[clap(skip)]
    countdown: Option<Countdown>,
}

impl App {
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = Some(mode);
        self.init_app();
    }

    pub fn set_mode_if_inactive(&mut self, mode: Mode) {
        if !self.is_mode_active(&mode) {
            self.set_mode(mode);
        }
    }

    pub fn is_mode_active(&self, mode: &Mode) -> bool {
        match mode {
            Mode::Clock { .. } => self.clock.is_some(),
            Mode::Timer { .. } => self.timer.is_some(),
            Mode::Stopwatch => self.stopwatch.is_some(),
            Mode::Countdown { .. } => self.countdown.is_some(),
        }
    }

    pub fn init_app(&mut self) {
        // Keep active widget state mutually exclusive across mode switches.
        self.clock = None;
        self.timer = None;
        self.stopwatch = None;
        self.countdown = None;

        // Load config
        let config = Config::load();
        let default_config = config.as_ref().map(|c| &c.default);

        // default mode
        if self.mode.is_none() {
            self.mode = default_config.map(|c| match c.mode.as_str() {
                "timer" => {
                    let timer_config = config.as_ref().map(|c| &c.timer);
                    Mode::Timer {
                        durations: timer_config
                            .map(|c| {
                                c.durations
                                    .iter()
                                    .filter_map(|d| parse_duration(d).ok())
                                    .collect()
                            })
                            .unwrap_or_else(|| vec![Duration::minutes(25), Duration::minutes(5)]),
                        titles: timer_config.map(|c| c.titles.clone()).unwrap_or_default(),
                        repeat: timer_config.map(|c| c.repeat).unwrap_or(false),
                        no_millis: !timer_config.map(|c| c.show_millis).unwrap_or(true),
                        paused: timer_config.map(|c| c.start_paused).unwrap_or(false),
                        continue_mode: timer_config.and_then(timer_continue_mode_from_config),
                        auto_quit: timer_config.map(|c| c.auto_quit).unwrap_or(false),
                        execute: timer_config.map(|c| c.execute.clone()).unwrap_or_default(),
                    }
                }
                "stopwatch" => Mode::Stopwatch,
                "countdown" => {
                    let countdown_config = config.as_ref().map(|c| &c.countdown);
                    Mode::Countdown {
                        time: countdown_config
                            .and_then(|c| c.time.as_ref())
                            .and_then(|t| parse_datetime(t).ok())
                            .unwrap_or_else(|| Local::now()),
                        title: countdown_config.map(|c| c.title.clone()).unwrap_or(None),
                        continue_on_zero: countdown_config
                            .map(|c| c.continue_on_zero)
                            .unwrap_or(false),
                        reverse: countdown_config.map(|c| c.reverse).unwrap_or(false),
                        millis: countdown_config.map(|c| c.show_millis).unwrap_or(false),
                    }
                }
                _ => {
                    let clock_config = config.as_ref().map(|c| &c.clock);
                    Mode::Clock {
                        no_date: !clock_config.map(|c| c.show_date).unwrap_or(true),
                        millis: clock_config.map(|c| c.show_millis).unwrap_or(false),
                        no_seconds: !clock_config.map(|c| c.show_seconds).unwrap_or(true),
                        timezone: clock_config.and_then(|c| c.timezone),
                    }
                }
            });
        }

        // set default color and size
        if self.color.is_none() {
            self.color = default_config
                .map(|c| parse_color(&c.color).unwrap_or(Color::Green))
                .or(Some(Color::Green));
        }
        if self.size.is_none() {
            self.size = default_config.map(|c| c.size).or(Some(1));
        }

        let style = Style::default().fg(self.color.unwrap_or(Color::Green));
        let size = self.size.unwrap_or(1);

        // initialize the clock mode
        match self.mode.as_ref().unwrap_or(&Mode::Clock {
            no_date: false,
            millis: false,
            no_seconds: false,
            timezone: None,
        }) {
            Mode::Clock {
                no_date,
                no_seconds,
                millis,
                timezone,
            } => {
                let clock_config = config.as_ref().map(|c| &c.clock);
                self.clock = Some(Clock {
                    size,
                    style,
                    show_date: !no_date && clock_config.map(|c| c.show_date).unwrap_or(true),
                    show_millis: *millis || clock_config.map(|c| c.show_millis).unwrap_or(false),
                    show_secs: !no_seconds && clock_config.map(|c| c.show_seconds).unwrap_or(true),
                    timezone: timezone.or_else(|| clock_config.and_then(|c| c.timezone)),
                });
            }
            Mode::Timer {
                durations,
                titles,
                repeat,
                no_millis,
                paused,
                continue_mode,
                auto_quit,
                execute,
            } => {
                let timer_config = config.as_ref().map(|c| &c.timer);
                let config_continue_mode = timer_config.and_then(timer_continue_mode_from_config);
                let continue_mode = (*continue_mode).or(config_continue_mode);
                let format = if *no_millis {
                    DurationFormat::HourMinSec
                } else {
                    DurationFormat::HourMinSecDeci
                };
                self.timer = Some(Timer::new(
                    size,
                    style,
                    durations.to_owned(),
                    titles.to_owned(),
                    *repeat || timer_config.map(|c| c.repeat).unwrap_or(false),
                    format,
                    *paused || timer_config.map(|c| c.start_paused).unwrap_or(false),
                    *auto_quit || timer_config.map(|c| c.auto_quit).unwrap_or(false),
                    continue_mode,
                    execute.to_owned(),
                ));
            }
            Mode::Stopwatch => {
                self.stopwatch = Some(Stopwatch::new(size, style));
            }
            Mode::Countdown {
                time,
                title,
                continue_on_zero,
                reverse,
                millis,
            } => {
                let countdown_config = config.as_ref().map(|c| &c.countdown);
                self.countdown = Some(Countdown {
                    size,
                    style,
                    time: *time,
                    title: title.to_owned(),
                    continue_on_zero: *continue_on_zero
                        || countdown_config
                            .map(|c| c.continue_on_zero)
                            .unwrap_or(false),
                    reverse: *reverse || countdown_config.map(|c| c.reverse).unwrap_or(false),
                    format: if *millis || countdown_config.map(|c| c.show_millis).unwrap_or(false) {
                        DurationFormat::HourMinSecDeci
                    } else {
                        DurationFormat::HourMinSec
                    },
                })
            }
        }
    }

    pub fn ui(&self, f: &mut Frame) {
        if let Some(ref w) = self.clock {
            f.render_widget(w, f.size());
        } else if let Some(ref w) = self.timer {
            f.render_widget(w, f.size());
        } else if let Some(ref w) = self.stopwatch {
            f.render_widget(w, f.size());
        } else if let Some(ref w) = self.countdown {
            f.render_widget(w, f.size());
        }
    }

    pub fn on_key(&mut self, key: KeyCode) {
        if let Some(_w) = self.clock.as_mut() {
        } else if let Some(w) = self.timer.as_mut() {
            handle_key(w, key);
        } else if let Some(w) = self.stopwatch.as_mut() {
            handle_key(w, key);
        }
    }

    pub fn is_ended(&self) -> bool {
        if let Some(ref w) = self.timer {
            return w.is_finished();
        }
        false
    }

    pub fn on_exit(&self) {
        if let Some(ref w) = self.stopwatch {
            println!("Stopwatch time: {}", w.get_display_time());
        }
    }
}

fn handle_key<T: Pause>(widget: &mut T, key: KeyCode) {
    if let KeyCode::Char(' ') = key {
        widget.toggle_paused()
    }
}

fn parse_timer_continue_mode_str(s: &str) -> Option<TimerContinueMode> {
    match s.to_ascii_lowercase().as_str() {
        "blink" => Some(TimerContinueMode::Blink),
        "text" => Some(TimerContinueMode::Text),
        _ => None,
    }
}

fn timer_continue_mode_from_config(timer: &TimerConfig) -> Option<TimerContinueMode> {
    if let Some(mode) = timer.continue_mode.as_deref() {
        if let Some(parsed) = parse_timer_continue_mode_str(mode) {
            return Some(parsed);
        }
        eprintln!(
            "Invalid timer.continue_mode '{}' in config, expected 'blink' or 'text'",
            mode
        );
    }

    if timer.continue_on_zero {
        return Some(if timer.continue_text {
            TimerContinueMode::Text
        } else {
            TimerContinueMode::Blink
        });
    }

    None
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Duration is empty".to_string());
    }

    let reg = Regex::new(r"(?i)(\d+)([smhd])").unwrap();
    let mut total = Duration::zero();
    let mut consumed = 0usize;

    for cap in reg.captures_iter(s) {
        let m = cap.get(0).unwrap();
        if m.start() != consumed {
            return Err(format!("{} is not a valid duration", s));
        }
        consumed = m.end();

        let num = cap
            .get(1)
            .unwrap()
            .as_str()
            .parse::<i64>()
            .map_err(|_| format!("Invalid number in duration: {}", s))?;
        let unit = cap.get(2).unwrap().as_str().to_ascii_lowercase();

        let part = match unit.as_str() {
            "s" => Duration::seconds(num),
            "m" => Duration::minutes(num),
            "h" => Duration::hours(num),
            "d" => Duration::days(num),
            _ => return Err(format!("Invalid duration: {}", s)),
        };
        total = total + part;
    }

    if consumed == s.len() && consumed > 0 {
        Ok(total)
    } else {
        Err(format!("{} is not a valid duration", s))
    }
}

fn parse_color(s: &str) -> Result<Color, String> {
    let s = s.to_lowercase();
    let reg = Regex::new(r"^#([0-9a-f]{6})$").unwrap();
    match s.as_str() {
        "black" => Ok(Color::Black),
        "red" => Ok(Color::Red),
        "green" => Ok(Color::Green),
        "yellow" => Ok(Color::Yellow),
        "blue" => Ok(Color::Blue),
        "magenta" => Ok(Color::Magenta),
        "cyan" => Ok(Color::Cyan),
        "gray" => Ok(Color::Gray),
        "darkgray" => Ok(Color::DarkGray),
        "lightred" => Ok(Color::LightRed),
        "lightgreen" => Ok(Color::LightGreen),
        "lightyellow" => Ok(Color::LightYellow),
        "lightblue" => Ok(Color::LightBlue),
        "lightmagenta" => Ok(Color::LightMagenta),
        "lightcyan" => Ok(Color::LightCyan),
        "white" => Ok(Color::White),
        s => {
            let cap = reg
                .captures(s)
                .ok_or_else(|| format!("Invalid color: {}", s))?;
            let hex = cap.get(1).unwrap().as_str();
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
            let b = u8::from_str_radix(&hex[4..], 16).unwrap();
            Ok(Color::Rgb(r, g, b))
        }
    }
}

fn parse_datetime(s: &str) -> Result<DateTime<Local>, String> {
    let s = s.trim();
    let today = Local::now().date_naive();

    let time = NaiveTime::parse_from_str(s, "%H:%M");
    if let Ok(time) = time {
        let time = NaiveDateTime::new(today, time);
        return Ok(Local.from_local_datetime(&time).unwrap());
    }

    let time = NaiveTime::parse_from_str(s, "%H:%M:%S");
    if let Ok(time) = time {
        let time = NaiveDateTime::new(today, time);
        return Ok(Local.from_local_datetime(&time).unwrap());
    }

    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d");
    if let Ok(date) = date {
        let time = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        return Ok(Local.from_local_datetime(&time).unwrap());
    }

    let date_time = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S");
    if let Ok(date_time) = date_time {
        return Ok(Local.from_local_datetime(&date_time).unwrap());
    }

    let rfc_time = DateTime::parse_from_rfc3339(s);
    if let Ok(rfc_time) = rfc_time {
        return Ok(rfc_time.with_timezone(&Local));
    }

    Err("Invalid time format".to_string())
}

fn parse_timezone(s: &str) -> Result<Tz, String> {
    s.parse()
}

#[cfg(test)]
mod tests {
    use super::{parse_duration, App, Mode};
    use chrono::Duration;

    #[test]
    fn parse_duration_supports_single_unit() {
        assert_eq!(parse_duration("450s").unwrap(), Duration::seconds(450));
        assert_eq!(parse_duration("5m").unwrap(), Duration::minutes(5));
    }

    #[test]
    fn parse_duration_supports_combined_units() {
        assert_eq!(
            parse_duration("7m30s").unwrap(),
            Duration::minutes(7) + Duration::seconds(30)
        );
        assert_eq!(
            parse_duration("3h52m12s").unwrap(),
            Duration::hours(3) + Duration::minutes(52) + Duration::seconds(12)
        );
    }

    #[test]
    fn parse_duration_supports_uppercase_units() {
        assert_eq!(
            parse_duration("1H2M3S").unwrap(),
            Duration::hours(1) + Duration::minutes(2) + Duration::seconds(3)
        );
    }

    #[test]
    fn parse_duration_rejects_invalid_values() {
        assert!(parse_duration("450").is_err());
        assert!(parse_duration("7m-30s").is_err());
        assert!(parse_duration("1x").is_err());
        assert!(parse_duration("m10").is_err());
    }

    #[test]
    fn set_mode_if_inactive_does_not_restart_same_mode() {
        let mut app = App::default();
        app.set_mode(Mode::Timer {
            durations: vec![Duration::minutes(1)],
            titles: vec![],
            repeat: false,
            no_millis: false,
            paused: false,
            continue_mode: None,
            auto_quit: false,
            execute: vec![],
        });

        app.set_mode_if_inactive(Mode::Timer {
            durations: vec![Duration::minutes(5)],
            titles: vec![],
            repeat: false,
            no_millis: false,
            paused: false,
            continue_mode: None,
            auto_quit: false,
            execute: vec![],
        });

        match app.mode {
            Some(Mode::Timer { durations, .. }) => {
                assert_eq!(durations, vec![Duration::minutes(1)]);
            }
            _ => panic!("expected timer mode"),
        }
    }

    #[test]
    fn mode_switching_keeps_only_one_active_widget() {
        let mut app = App::default();
        let clock_mode = Mode::Clock {
            timezone: None,
            no_date: false,
            no_seconds: false,
            millis: false,
        };

        app.set_mode(clock_mode);
        assert!(app.is_mode_active(&Mode::Clock {
            timezone: None,
            no_date: false,
            no_seconds: false,
            millis: false,
        }));

        app.set_mode(Mode::Stopwatch);
        assert!(app.is_mode_active(&Mode::Stopwatch));
        assert!(!app.is_mode_active(&Mode::Clock {
            timezone: None,
            no_date: false,
            no_seconds: false,
            millis: false,
        }));

        app.set_mode(Mode::Timer {
            durations: vec![Duration::minutes(1)],
            titles: vec![],
            repeat: false,
            no_millis: false,
            paused: false,
            continue_mode: None,
            auto_quit: false,
            execute: vec![],
        });
        assert!(app.is_mode_active(&Mode::Timer {
            durations: vec![Duration::minutes(10)],
            titles: vec![],
            repeat: false,
            no_millis: false,
            paused: false,
            continue_mode: None,
            auto_quit: false,
            execute: vec![],
        }));
        assert!(!app.is_mode_active(&Mode::Stopwatch));
    }
}
