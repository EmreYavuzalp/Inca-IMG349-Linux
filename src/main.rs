//! img349ctl: configure the Inca IMG-349 gaming mouse on Linux.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand, ValueEnum};

use img349ctl::config::{self, Config};
use img349ctl::device::{self, Mouse};
use img349ctl::protocol::{self as p, LedMode};

#[derive(Parser)]
#[command(version, about = "Configure the Inca IMG-349 gaming mouse.")]
struct Cli {
    /// hidraw node [default: auto-detect]
    #[arg(long, global = true, value_name = "PATH")]
    device: Option<PathBuf>,
    /// Print the reports instead of sending them
    #[arg(short = 'n', long, global = true)]
    dry_run: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    A,
    B,
}

impl Mode {
    fn index(self) -> u8 {
        self as u8
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Show the detected mouse and its settings
    Status,
    /// List what buttons can be bound to
    Actions,
    /// Save the mouse's whole memory to a file, as a backup
    Backup { file: PathBuf },
    /// Set a DPI level's value
    Dpi {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=6))]
        level: u8,
        /// 200 400 600 800 1000 1200 1600 2000 2400 3200 4800 6400
        value: u16,
    },
    /// Switch to a DPI level
    Level {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=6))]
        level: u8,
    },
    /// Set a DPI level's LED colour
    Color {
        /// DPI level 1-6, or "all"
        #[arg(value_parser = parse_level_or_all)]
        level: u8,
        /// RRGGBB or a name (red, orange, cyan, ...)
        color: String,
    },
    /// One colour at every DPI level: DPI colour mode, so the light still breathes
    Single {
        /// RRGGBB or a name (red, orange, cyan, ...)
        color: String,
    },
    /// LED mode, speed and brightness
    Led {
        mode: Option<LedMode>,
        /// 0 is fastest, 7 slowest
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=7))]
        speed: Option<u8>,
        /// Half brightness
        #[arg(long, conflicts_with = "full")]
        half: bool,
        /// Full brightness
        #[arg(long)]
        full: bool,
    },
    /// Polling rate in Hz: 125, 250, 500 or 1000. The mouse restarts for a moment.
    Rate { hz: u16 },
    /// Bind a button (1-7, numbered as in the vendor tool)
    Button {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=7))]
        button: u8,
        /// See "img349ctl actions"
        action: String,
        /// Which mode's table to change
        #[arg(long, value_enum, default_value = "a")]
        mode: Mode,
    },
    /// Switch between mode A and mode B
    Mode {
        #[arg(value_enum)]
        mode: Mode,
    },
    /// Put all settings back as the mouse came
    Reset,
}

/// "1".."6", or "all" (returned as 0).
fn parse_level_or_all(s: &str) -> Result<u8, String> {
    match s {
        "all" => Ok(0),
        _ => match s.parse::<u8>() {
            Ok(n) if (1..=6).contains(&n) => Ok(n),
            _ => Err("level must be 1-6 or \"all\"".into()),
        },
    }
}

fn show(c: &Config) {
    println!("DPI levels (* current):");
    for i in 0..p::NUM_LEVELS {
        let mark = if i == c.level { '*' } else { ' ' };
        let [r, g, b] = c.colors[i];
        println!(" {mark}{}: {:>4} dpi  #{r:02x}{g:02x}{b:02x}", i + 1, c.dpi_value(i));
    }
    println!("Mode: {}   Polling rate: {} Hz", if c.mode == 0 { "A" } else { "B" }, c.rate_hz());
    let [r, g, b] = c.colors[0];
    let led = if c.is_single_colour() { format!("single colour #{r:02x}{g:02x}{b:02x}") } else { c.led_mode.name().to_string() };
    println!("LED: {led}, speed {}, {} brightness", c.led_speed, if c.half_bright { "half" } else { "full" });
    println!("Buttons:            mode A              mode B");
    for b in 0..config::BUTTONS {
        println!("  {}: {:<24} {}", b + 1, p::action_name(c.button(0, b)), p::action_name(c.button(1, b)));
    }
}

fn actions() {
    println!("Fixed functions, all carried out by the mouse itself:");
    let names: Vec<&str> = p::ACTIONS.iter().map(|(n, _)| *n).collect();
    println!("  {}", names.join(" "));
    println!("  keys:CHORD    one key with modifiers, e.g. keys:ctrl+shift+t");
    println!("  raw:XXXXXXXX  the 4 stored bytes in hex");
}

fn modify(cmd: Cmd, mut c: Config) -> Result<Config> {
    match cmd {
        Cmd::Reset => return Ok(Config::factory(c.mode)),
        Cmd::Dpi { level, value } => c.dpi[level as usize - 1] = p::dpi_code(value)?,
        Cmd::Level { level } => c.level = level as usize - 1,
        Cmd::Color { level, color } => {
            let rgb = config::parse_color(&color)?;
            match level {
                0 => c.colors = [rgb; p::NUM_LEVELS],
                n => c.colors[n as usize - 1] = rgb,
            }
        }
        Cmd::Single { color } => c.set_single_colour(config::parse_color(&color)?),
        Cmd::Led { mode, speed, half, full } => {
            if mode.is_none() && speed.is_none() && !half && !full {
                bail!("give a mode, --speed, --half or --full");
            }
            if let Some(m) = mode {
                c.led_mode = m;
            }
            if let Some(s) = speed {
                c.led_speed = s;
            }
            if half || full {
                c.half_bright = half;
            }
        }
        Cmd::Rate { hz } => c.set_rate(hz)?,
        Cmd::Button { button, action, mode } => c.set_button(mode.index() as usize, button as usize - 1, p::parse_action(&action)?)?,
        Cmd::Mode { mode } => c.mode = mode.index(),
        Cmd::Status | Cmd::Actions | Cmd::Backup { .. } => {}
    }
    Ok(c)
}

fn run(cli: Cli) -> Result<()> {
    if matches!(cli.cmd, Cmd::Actions) {
        actions();
        return Ok(());
    }
    if matches!(cli.cmd, Cmd::Status) && device::find().is_empty() && cli.device.is_none() {
        println!("Mouse: not found");
        return Ok(());
    }
    let mouse = Mouse::open(cli.device.as_deref())?;
    match cli.cmd {
        Cmd::Status => {
            let found: Vec<String> = device::find().iter().map(|p| p.display().to_string()).collect();
            println!("Mouse: {}", found.join(", "));
            show(&Config::read(&mouse)?);
        }
        Cmd::Backup { file } => {
            let memory = mouse.read(0, p::MEMORY_SIZE)?;
            std::fs::write(&file, memory)?;
            println!("Saved {} bytes to {}", p::MEMORY_SIZE, file.display());
        }
        cmd => {
            let old = Config::read(&mouse)?;
            let new = modify(cmd, old.clone())?;
            let reports = new.changes_from(&old);
            if cli.dry_run {
                for r in &reports {
                    println!("{}", r.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "));
                }
                return Ok(());
            }
            mouse.send_all(&reports)?;
            // After a restart the mouse is away for a moment and checks itself on the way back.
            if new.rate_byte == old.rate_byte && !new.verify(&mouse) {
                bail!("the mouse did not take the change; run \"img349ctl status\" to see what it has");
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    // Exit quietly when stdout is closed early (e.g. `img349ctl status | head`).
    // SAFETY: called before any other thread exists.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("img349ctl: {e:#}");
            ExitCode::FAILURE
        }
    }
}
