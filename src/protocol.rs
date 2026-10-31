//! Wire protocol of the Inca IMG-349 (Sunplus SPCP6651B, 1bcf:08b8) gaming mouse.
//!
//! Everything goes through 8-byte feature reports on the vendor collection:
//!
//! ```text
//! 40 09 <addr hi> <addr lo> <len hi> <len lo>   read memory, then GET report 41 chunks
//! 40 05 <addr hi> <addr lo> <b1> <b2> <b3> <n>  write n (1-3) bytes
//! 40 07 <addr hi> <addr lo> <len hi> <len lo>   write a block, then SET report 41 chunks
//! 40 01 <addr hi> <addr lo> <byte>              set a live value (not stored)
//! 40 06 00 a4 00 1c                             sent after a block
//! 40 0e                                         restart the mouse (it leaves USB for a moment)
//! ```
//!
//! The memory also holds the mouse's firmware, so only the addresses below are written.

use anyhow::{bail, Result};

pub const REPORT_LEN: usize = 8;
pub type Report = [u8; REPORT_LEN];
pub const CMD_REPORT: u8 = 0x40;
pub const DATA_REPORT: u8 = 0x41;
/// Data bytes in one report 41.
pub const CHUNK: usize = 7;

const CMD_SET_LIVE: u8 = 0x01;
const CMD_WRITE: u8 = 0x05;
const CMD_AFTER_BLOCK: u8 = 0x06;
const CMD_BLOCK: u8 = 0x07;
pub const CMD_READ: u8 = 0x09;
const CMD_RESTART: u8 = 0x0E;

pub const MEMORY_SIZE: usize = 0x800;
pub const ID_ADDR: u16 = 0x00;
pub const ID: [u8; 4] = [0x1B, 0xCF, 0x08, 0xB8];
pub const RATE_ADDR: u16 = 0x04;
pub const DPI_ADDR: u16 = 0x2C;
pub const COLORS_ADDR: u16 = 0x57;
pub const MODE_ADDR: u16 = 0xA4;
pub const LED_MODE_ADDR: u16 = 0xA5;
pub const LEVEL_ADDR: u16 = 0xA6;
pub const BUTTONS_ADDR: u16 = 0xC0;
pub const BUTTONS_LEN: usize = 0x93;
pub const LED_BLOCK_ADDR: u16 = 0x3F0;
pub const LED_BLOCK_LEN: usize = 16;
/// The start of the LED block never changes; anything else means an unknown mouse.
pub const LED_BLOCK_HEAD: [u8; 8] = [0x04, 0x00, 0x02, 0xE0, 0x00, 0x1A, 0x5F, 0x63];
const LIVE_LED_SPEED: u16 = 0x7C4;
const LIVE_LED_BRIGHTNESS: u16 = 0x7C5;

pub const NUM_LEVELS: usize = 6;
pub const NUM_BUTTONS: usize = 7;
pub const NUM_SLOTS: usize = 12;
pub const TABLE_LEN: usize = NUM_SLOTS * 4;
/// A table and its check byte.
pub const TABLE_STRIDE: usize = TABLE_LEN + 1;

/// Table slot of buttons 1-7, as numbered on the vendor's picture of the mouse.
/// Slots 2 and 7 hold other functions and are left alone.
pub const BUTTON_SLOT: [usize; NUM_BUTTONS] = [0, 3, 6, 4, 1, 5, 8];

pub const DPI_VALUES: [u16; 12] = [200, 400, 600, 800, 1000, 1200, 1600, 2000, 2400, 3200, 4800, 6400];
pub const RATES: [u16; 4] = [125, 250, 500, 1000];

pub const LED_SPEED_MAX: u8 = 7;
pub const BRIGHTNESS_FULL: u8 = 0x83;
pub const BRIGHTNESS_HALF: u8 = 0x03;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum LedMode {
    /// Colour of the current DPI level
    Dpi,
    Colorful,
    Rainbow,
    Floe,
    Waltz,
    Seasons,
    Off,
}

impl LedMode {
    pub const ALL: [LedMode; 7] = [LedMode::Dpi, LedMode::Colorful, LedMode::Rainbow, LedMode::Floe, LedMode::Waltz, LedMode::Seasons, LedMode::Off];

    pub fn byte(self) -> u8 {
        0x81 + LedMode::ALL.iter().position(|m| *m == self).unwrap() as u8
    }

    pub fn from_byte(b: u8) -> Option<Self> {
        LedMode::ALL.get(b.wrapping_sub(0x81) as usize).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            LedMode::Dpi => "dpi",
            LedMode::Colorful => "colorful",
            LedMode::Rainbow => "rainbow",
            LedMode::Floe => "floe",
            LedMode::Waltz => "waltz",
            LedMode::Seasons => "seasons",
            LedMode::Off => "off",
        }
    }
}

/// Functions with fixed codes. Slot layout: <type> <a> <b> <c>.
pub const ACTIONS: &[(&str, [u8; 4])] = &[
    ("left", [0x01, 0x01, 0, 0]),
    ("right", [0x01, 0x02, 0, 0]),
    ("middle", [0x01, 0x04, 0, 0]),
    ("back", [0x01, 0x08, 0, 0x02]),
    ("forward", [0x01, 0x10, 0, 0]),
    ("dpi-loop", [0x09, 0x00, 0, 0]),
    ("dpi+", [0x09, 0x01, 0, 0]),
    ("dpi-", [0x09, 0x02, 0, 0]),
    ("fire1", [0x06, 0x92, 0, 0x3C]),
    ("fire2", [0x06, 0x14, 0, 0x3C]),
    ("fire3", [0x06, 0x17, 0, 0x3C]),
    ("aim", [0x05, 0, 0, 0]),
    ("led-mode", [0x0E, 0, 0, 0]),
    ("led-toggle", [0x08, 0, 0, 0]),
    ("rate", [0x0C, 0, 0, 0]),
    ("office-mode", [0x07, 0x80, 0, 0]),
    ("gaming-mode", [0x07, 0x81, 0, 0]),
    ("power-down", [0x04, 0x01, 0, 0]),
    ("sleep", [0x04, 0x02, 0, 0]),
    ("wake", [0x04, 0x03, 0, 0]),
    ("off", [0x00, 0, 0, 0]),
    ("play-pause", [0x03, 0x00, 0xCD, 0]),
    ("stop", [0x03, 0x00, 0xB7, 0]),
    ("previous", [0x03, 0x00, 0xB6, 0]),
    ("next", [0x03, 0x00, 0xB5, 0]),
    ("volume-up", [0x03, 0x00, 0xE9, 0]),
    ("volume-down", [0x03, 0x00, 0xEA, 0]),
    ("mute", [0x03, 0x00, 0xE2, 0]),
    ("web-home", [0x03, 0x02, 0x23, 0]),
    ("web-search", [0x03, 0x02, 0x21, 0]),
    ("web-back", [0x03, 0x02, 0x24, 0]),
    ("web-forward", [0x03, 0x02, 0x25, 0]),
    ("web-stop", [0x03, 0x02, 0x26, 0]),
    ("web-refresh", [0x03, 0x02, 0x27, 0]),
    ("web-favorites", [0x03, 0x02, 0x2A, 0]),
    ("my-computer", [0x03, 0x01, 0x94, 0]),
    ("calculator", [0x03, 0x01, 0x92, 0]),
    ("email", [0x03, 0x01, 0x8A, 0]),
    ("show-desktop", [0x02, 0x08, 0x07, 0]),
];
const TYPE_KEYBOARD: u8 = 0x02;

const MODIFIERS: &[(&str, u8)] = &[
    ("ctrl", 0x01), ("shift", 0x02), ("alt", 0x04), ("super", 0x08),
    ("rctrl", 0x10), ("rshift", 0x20), ("altgr", 0x40), ("rsuper", 0x80),
];
const KEYS: &[(&str, u8)] = &[
    ("enter", 0x28), ("esc", 0x29), ("backspace", 0x2A), ("tab", 0x2B), ("space", 0x2C),
    ("minus", 0x2D), ("equal", 0x2E), ("leftbracket", 0x2F), ("rightbracket", 0x30),
    ("backslash", 0x31), ("semicolon", 0x33), ("apostrophe", 0x34), ("grave", 0x35),
    ("comma", 0x36), ("dot", 0x37), ("slash", 0x38), ("capslock", 0x39),
    ("printscreen", 0x46), ("scrolllock", 0x47), ("pause", 0x48), ("insert", 0x49),
    ("home", 0x4A), ("pageup", 0x4B), ("delete", 0x4C), ("end", 0x4D), ("pagedown", 0x4E),
    ("right", 0x4F), ("left", 0x50), ("down", 0x51), ("up", 0x52), ("menu", 0x65),
];

fn key_usage(name: &str) -> Option<u8> {
    if let Some((_, u)) = KEYS.iter().find(|(n, _)| *n == name) {
        return Some(*u);
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return match c {
            'a'..='z' => Some(0x04 + (c as u8 - b'a')),
            '1'..='9' => Some(0x1E + (c as u8 - b'1')),
            '0' => Some(0x27),
            _ => None,
        };
    }
    let n: u8 = name.strip_prefix('f')?.parse().ok()?;
    (1..=12).contains(&n).then(|| 0x3A + n - 1)
}

fn key_name(usage: u8) -> Option<String> {
    if let Some((n, _)) = KEYS.iter().find(|(_, u)| *u == usage) {
        return Some(n.to_string());
    }
    match usage {
        0x04..=0x1D => Some(((b'a' + usage - 0x04) as char).to_string()),
        0x1E..=0x26 => Some(((b'1' + usage - 0x1E) as char).to_string()),
        0x27 => Some("0".to_string()),
        0x3A..=0x45 => Some(format!("f{}", usage - 0x3A + 1)),
        _ => None,
    }
}

/// Text for a button slot. Functions this tool has no name for are shown as `raw:`.
pub fn action_name(slot: [u8; 4]) -> String {
    if let Some((n, _)) = ACTIONS.iter().find(|(_, a)| *a == slot) {
        return n.to_string();
    }
    if slot[0] == TYPE_KEYBOARD && slot[3] == 0 {
        if let Some(key) = key_name(slot[2]) {
            let mut parts: Vec<String> = MODIFIERS.iter().filter(|(_, m)| slot[1] & m != 0).map(|(n, _)| n.to_string()).collect();
            parts.push(key);
            return format!("keys:{}", parts.join("+"));
        }
    }
    format!("raw:{:02x}{:02x}{:02x}{:02x}", slot[0], slot[1], slot[2], slot[3])
}

pub fn parse_action(text: &str) -> Result<[u8; 4]> {
    let t = text.trim().to_lowercase();
    if let Some((_, a)) = ACTIONS.iter().find(|(n, _)| *n == t) {
        return Ok(*a);
    }
    if let Some(chord) = t.strip_prefix("keys:") {
        let (mut mods, mut key) = (0u8, None);
        for part in chord.split('+').filter(|p| !p.is_empty()) {
            if let Some((_, m)) = MODIFIERS.iter().find(|(n, _)| *n == part) {
                mods |= m;
            } else if key.is_some() {
                bail!("a button can send only one key besides modifiers: {text:?}");
            } else {
                key = Some(key_usage(part).ok_or_else(|| anyhow::anyhow!("unknown key {part:?}"))?);
            }
        }
        let Some(key) = key else { bail!("{text:?} has no key, only modifiers") };
        return Ok([TYPE_KEYBOARD, mods, key, 0]);
    }
    if let Some(hex) = t.strip_prefix("raw:") {
        if hex.len() == 8 {
            if let Ok(v) = u32::from_str_radix(hex, 16) {
                return Ok(v.to_be_bytes());
            }
        }
        bail!("raw: needs 8 hex digits");
    }
    bail!("unknown action {text:?} (see \"img349ctl actions\")")
}

pub fn dpi_code(dpi: u16) -> Result<u8> {
    match DPI_VALUES.iter().position(|&d| d == dpi) {
        Some(i) => Ok(i as u8),
        None => {
            let all: Vec<String> = DPI_VALUES.iter().map(u16::to_string).collect();
            bail!("unsupported DPI {dpi}; choose one of {}", all.join(", "))
        }
    }
}

fn sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |s, b| s.wrapping_add(*b))
}

/// Check byte of a button table.
pub fn table_check(table: &[u8]) -> u8 {
    sum(table) ^ 0x5A
}

/// Check byte of the LED block.
fn led_check(block: &[u8]) -> u8 {
    sum(block).wrapping_add(0x0E) ^ 0x8A
}

/// The DPI steps are packed in pairs of nibbles, the lower level in the low nibble.
pub fn pack_dpi(codes: &[u8; NUM_LEVELS]) -> [u8; 3] {
    [codes[0] | codes[1] << 4, codes[2] | codes[3] << 4, codes[4] | codes[5] << 4]
}

pub fn unpack_dpi(packed: &[u8]) -> [u8; NUM_LEVELS] {
    [packed[0] & 15, packed[0] >> 4, packed[1] & 15, packed[1] >> 4, packed[2] & 15, packed[2] >> 4]
}

/// The whole LED block for a speed and brightness.
pub fn led_block(speed: u8, brightness: u8) -> [u8; LED_BLOCK_LEN] {
    let mut b = [0u8; LED_BLOCK_LEN];
    b[..8].copy_from_slice(&LED_BLOCK_HEAD);
    b[8] = speed;
    b[9] = brightness;
    b[15] = led_check(&b[..15]);
    b
}

fn cmd(c: u8, addr: u16, a: u8, b: u8, d: u8, e: u8) -> Report {
    [CMD_REPORT, c, (addr >> 8) as u8, addr as u8, a, b, d, e]
}

pub fn read_report(addr: u16, len: u16) -> Report {
    cmd(CMD_READ, addr, (len >> 8) as u8, len as u8, 0, 0)
}

/// Write 1-3 bytes.
pub fn write_report(addr: u16, data: &[u8]) -> Report {
    let mut r = cmd(CMD_WRITE, addr, 0, 0, 0, data.len() as u8);
    r[4..4 + data.len()].copy_from_slice(data);
    r
}

pub fn live_report(addr: u16, value: u8) -> Report {
    cmd(CMD_SET_LIVE, addr, value, 0, 0, 0)
}

pub fn live_led_reports(speed: u8, brightness: u8) -> [Report; 2] {
    [live_report(LIVE_LED_SPEED, speed), live_report(LIVE_LED_BRIGHTNESS, brightness)]
}

pub fn block_reports(addr: u16, data: &[u8]) -> Vec<Report> {
    let len = data.len() as u16;
    let mut out = vec![cmd(CMD_BLOCK, addr, (len >> 8) as u8, len as u8, 0, 0)];
    for chunk in data.chunks(CHUNK) {
        let mut r = [0u8; REPORT_LEN];
        r[0] = DATA_REPORT;
        r[1..1 + chunk.len()].copy_from_slice(chunk);
        out.push(r);
    }
    out
}

/// Sent after changing the button tables, so that they take effect.
pub fn buttons_reload_reports(mode: u8) -> [Report; 3] {
    [cmd(CMD_AFTER_BLOCK, MODE_ADDR, 0, 0x1C, 0, 0), write_report(MODE_ADDR, &[0x0F]), write_report(MODE_ADDR, &[mode])]
}

/// Sent when switching between mode A and mode B.
pub fn mode_reports(mode: u8) -> [Report; 5] {
    let (a8, aa) = if mode == 0 { (0x00, 0x00) } else { (0x0F, 0x18) };
    [
        write_report(0xA7, &[0x00]),
        write_report(0xA8, &[a8]),
        write_report(0xA9, &[0x00]),
        write_report(0xAA, &[aa]),
        write_report(MODE_ADDR, &[mode]),
    ]
}

pub fn restart_report() -> Report {
    cmd(CMD_RESTART, 0, 0, 0, 0, 0)
}

/// Pause the mouse needs after a report.
pub fn gap_ms(report: &Report) -> u64 {
    match (report[0], report[1]) {
        (CMD_REPORT, CMD_WRITE) | (CMD_REPORT, CMD_SET_LIVE) => 130,
        (CMD_REPORT, CMD_AFTER_BLOCK) => 130,
        _ => 5,
    }
}
