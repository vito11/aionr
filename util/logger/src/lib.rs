/*******************************************************************************
 * Copyright (c) 2015-2018 Parity Technologies (UK) Ltd.
 * Copyright (c) 2018-2019 Aion foundation.
 *
 *     This file is part of the aion network project.
 *
 *     The aion network project is free software: you can redistribute it
 *     and/or modify it under the terms of the GNU General Public License
 *     as published by the Free Software Foundation, either version 3 of
 *     the License, or any later version.
 *
 *     The aion network project is distributed in the hope that it will
 *     be useful, but WITHOUT ANY WARRANTY; without even the implied
 *     warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 *     See the GNU General Public License for more details.
 *
 *     You should have received a copy of the GNU General Public License
 *     along with the aion network project source files.
 *     If not, see <https://www.gnu.org/licenses/>.
 *
 ******************************************************************************/

//! Logger for Aion executables
#![warn(unused_extern_crates)]

extern crate arrayvec;
extern crate log as rlog;
extern crate isatty;
extern crate regex;
extern crate env_logger;
extern crate time;
#[macro_use]
extern crate lazy_static;
extern crate parking_lot;
extern crate ansi_term;

mod rotating;

use std::{env, thread, fs};
use std::sync::{Weak, Arc};
use std::io::Write;
use isatty::{stderr_isatty, stdout_isatty};
use env_logger::LogBuilder;
use regex::Regex;
use ansi_term::Colour;
use parking_lot::Mutex;

pub use rotating::{RotatingLogger, init_log};

#[derive(Debug, PartialEq, Clone)]
pub struct LogConfig {
    pub targets: Option<String>,
    pub level: String,
    pub color: bool,
    pub file: Option<String>,
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            targets: None,
            level: "info".into(),
            color: !cfg!(windows),
            file: None,
        }
    }
}

lazy_static! {
    static ref ROTATING_LOGGER: Mutex<Weak<RotatingLogger>> = Mutex::new(Default::default());
}

/// Sets up the logger
pub fn setup_log(config: &LogConfig) -> Result<Arc<RotatingLogger>, String> {
    use rlog::*;

    let mut levels = String::new();
    let mut builder = LogBuilder::new();
    // Disable info logging by default for some modules:
    builder.filter(Some("ws"), LogLevelFilter::Warn);
    builder.filter(Some("reqwest"), LogLevelFilter::Warn);
    builder.filter(Some("hyper"), LogLevelFilter::Warn);
    builder.filter(Some("rustls"), LogLevelFilter::Warn);
    // Enable info for others.
    builder.filter(
        None,
        config
            .level
            .as_str()
            .parse()
            .map_err(|_| format!("config level parse error"))?,
    );

    if let Ok(lvl) = env::var("RUST_LOG") {
        levels.push_str(&lvl);
        levels.push_str(",");
        builder.parse(&lvl);
    }
    // parse config mode
    match config.targets {
        None => {
            //doing noting
        }
        Some(ref targets) => {
            builder.parse(targets);
        }
    }

    let isatty = stderr_isatty();
    let enable_color = config.color && isatty;
    let logs = Arc::new(RotatingLogger::new(levels));
    let logger = logs.clone();
    let mut open_options = fs::OpenOptions::new();

    let maybe_file = match config.file.as_ref() {
        Some(f) => {
            Some(
                open_options
                    .append(true)
                    .create(true)
                    .open(f)
                    .map_err(|_| format!("Cannot write to log file given: {}", f))?,
            )
        }
        None => None,
    };

    let format = move |record: &LogRecord| {
        let timestamp =
            time::strftime("%Y-%m-%d %H:%M:%S", &time::now()).expect("set timestamp error");

        let with_color = if max_log_level() <= LogLevelFilter::Info {
            format!(
                "{} {}",
                Colour::Black.bold().paint(timestamp),
                record.args()
            )
        } else {
            let name = thread::current().name().map_or_else(Default::default, |x| {
                format!("{}", Colour::Blue.bold().paint(x))
            });
            format!(
                "{} {} {} {} line:{}  {}",
                Colour::Black.bold().paint(timestamp),
                name,
                record.level(),
                record.target(),
                record.location().__line,
                record.args()
            )
        };

        let removed_color = kill_color(with_color.as_ref());

        let ret = match enable_color {
            true => with_color,
            false => removed_color.clone(),
        };

        if let Some(mut file) = maybe_file.as_ref() {
            // ignore errors - there's nothing we can do
            let _ = file.write_all(removed_color.as_bytes());
            let _ = file.write_all(b"\n");
        }
        logger.append(removed_color);
        if !isatty && record.level() <= LogLevel::Debug && stdout_isatty() {
            // duplicate INFO/WARN output to console
            println!("{}", ret);
        }

        ret
    };

    builder.format(format);
    builder
        .init()
        .and_then(|_| {
            *ROTATING_LOGGER.lock() = Arc::downgrade(&logs);
            Ok(logs)
        })
        // couldn't create new logger - try to fall back on previous logger.
        .or_else(|err| {
            match ROTATING_LOGGER.lock().upgrade() {
                Some(l) => Ok(l),
                // no previous logger. fatal.
                None => Err(format!("{:?}", err)),
            }
        })
}

fn kill_color(s: &str) -> String {
    lazy_static! {
        static ref RE: Regex = Regex::new("\x1b\\[[^m]+m").expect("log kill color failed");
    }
    RE.replace_all(s, "").to_string()
}

#[test]
fn should_remove_colour() {
    let before = "test";
    let after = kill_color(&Colour::Red.bold().paint(before));
    assert_eq!(after, "test");
}

#[test]
fn should_remove_multiple_colour() {
    let t = format!(
        "{} {}",
        Colour::Red.bold().paint("test"),
        Colour::White.normal().paint("again")
    );
    let after = kill_color(&t);
    assert_eq!(after, "test again");
}
