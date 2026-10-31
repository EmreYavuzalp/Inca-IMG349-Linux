//! The mouse's settings: read them from its memory and work out what to write back.

use anyhow::{bail, Result};

use crate::device::Mouse;
use crate::factory;
use crate::protocol::{self as p, LedMode, Report, NUM_BUTTONS, NUM_LEVELS, TABLE_STRIDE};

/// Everything from the USB ID to the end of the button tables.
const MAIN_LEN: usize = p::BUTTONS_ADDR as usize + p::BUTTONS_LEN;
const VERIFY_TRIES: u32 = 8;
const VERIFY_GAP_MS: u64 = 250;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Stored byte, with the polling rate in bits 4-5
    pub rate_byte: u8,
    pub dpi: [u8; NUM_LEVELS],
    /// 0-based current DPI level
    pub level: usize,
    /// R, G, B of each level; the mouse stores them as G, R, B
    pub colors: [[u8; 3]; NUM_LEVELS],
    pub mode: u8,
    pub led_mode: LedMode,
    pub led_speed: u8,
    pub half_bright: bool,
    /// Mode A, mode B and a third table, with check bytes
    pub tables: [u8; p::BUTTONS_LEN],
}

impl Config {
    /// Reads the settings. Fails on any memory that doesn't look like what was seen on the IMG-349,
    /// because the same addresses would be overwritten later.
    pub fn read(mouse: &Mouse) -> Result<Self> {
        let m = mouse.read(0, MAIN_LEN)?;
        if m[p::ID_ADDR as usize..4] != p::ID {
            bail!("unexpected memory contents: this does not look like an IMG-349");
        }
        let led = mouse.read(p::LED_BLOCK_ADDR, p::LED_BLOCK_LEN)?;
        if led[..8] != p::LED_BLOCK_HEAD {
            bail!("unexpected LED settings area; not writing to this mouse");
        }
        let Some(led_mode) = LedMode::from_byte(m[p::LED_MODE_ADDR as usize]) else {
            bail!("unknown LED mode {:#04x}", m[p::LED_MODE_ADDR as usize]);
        };
        let level = m[p::LEVEL_ADDR as usize] as usize;
        if level >= NUM_LEVELS {
            bail!("unknown DPI level {level}");
        }
        let dpi = p::unpack_dpi(&m[p::DPI_ADDR as usize..]);
        if dpi.iter().any(|&c| c as usize >= p::DPI_VALUES.len()) {
            bail!("unknown DPI code in {dpi:02x?}");
        }
        let tables: [u8; p::BUTTONS_LEN] = m[p::BUTTONS_ADDR as usize..].try_into().unwrap();
        for t in 0..3 {
            let table = &tables[t * TABLE_STRIDE..][..TABLE_STRIDE];
            if p::table_check(&table[..p::TABLE_LEN]) != table[p::TABLE_LEN] {
                bail!("button table {t} has a bad check byte; not writing to this mouse");
            }
        }
        Ok(Config {
            rate_byte: m[p::RATE_ADDR as usize],
            dpi,
            level,
            colors: colors(&m[p::COLORS_ADDR as usize..]),
            mode: m[p::MODE_ADDR as usize],
            led_mode,
            led_speed: led[8] & p::LED_SPEED_MAX,
            half_bright: led[9] == p::BRIGHTNESS_HALF,
            tables,
        })
    }

    /// As the mouse came. `mode` stays as it is, the settings are the same in both.
    pub fn factory(mode: u8) -> Self {
        Config {
            rate_byte: factory::RATE_BYTE,
            dpi: p::unpack_dpi(&factory::DPI),
            level: factory::LEVEL as usize,
            colors: colors(&factory::COLORS),
            mode,
            led_mode: LedMode::from_byte(factory::LED_MODE).unwrap(),
            led_speed: factory::LED_SPEED,
            half_bright: factory::LED_BRIGHTNESS == p::BRIGHTNESS_HALF,
            tables: factory::BUTTONS,
        }
    }

    /// Whether the mouse holds this configuration. It stores changes with a delay, so this tries
    /// for a moment, and only returns once the change is really there: the next read would
    /// otherwise see the old values. The DPI buttons can change the level at any time, so in the
    /// end a different level alone doesn't count as a failure.
    pub fn verify(&self, mouse: &Mouse) -> bool {
        let mut last = None;
        for i in 0..VERIFY_TRIES {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(VERIFY_GAP_MS));
            }
            last = Config::read(mouse).ok();
            if last.as_ref() == Some(self) {
                return true;
            }
        }
        last.is_some_and(|got| Config { level: self.level, ..got } == *self)
    }

    pub fn uniform_colors(&self) -> bool {
        self.colors.iter().all(|c| *c == self.colors[0])
    }

    /// One colour at every DPI level. This is DPI colour mode, so it still breathes: the firmware
    /// has no mode for a steady light.
    pub fn is_single_colour(&self) -> bool {
        self.led_mode == LedMode::Dpi && self.uniform_colors()
    }

    pub fn set_single_colour(&mut self, rgb: [u8; 3]) {
        self.led_mode = LedMode::Dpi;
        self.colors = [rgb; NUM_LEVELS];
    }

    pub fn rate_hz(&self) -> u16 {
        p::RATES[(self.rate_byte >> 4 & 3) as usize]
    }

    pub fn set_rate(&mut self, hz: u16) -> Result<()> {
        let Some(code) = p::RATES.iter().position(|&r| r == hz) else {
            bail!("polling rate must be 125, 250, 500 or 1000");
        };
        self.rate_byte = self.rate_byte & 0xCF | (code as u8) << 4;
        Ok(())
    }

    pub fn dpi_value(&self, level: usize) -> u16 {
        p::DPI_VALUES[self.dpi[level] as usize]
    }

    fn slot_range(table: usize, button: usize) -> std::ops::Range<usize> {
        let start = table * TABLE_STRIDE + p::BUTTON_SLOT[button] * 4;
        start..start + 4
    }

    /// Function of a button (0-based) in mode table 0 (A) or 1 (B).
    pub fn button(&self, table: usize, button: usize) -> [u8; 4] {
        self.tables[Self::slot_range(table, button)].try_into().unwrap()
    }

    /// The vendor tool doesn't let the left button of mode A change, so that the mouse always
    /// has a way to click. The same rule applies here.
    pub fn set_button(&mut self, table: usize, button: usize, action: [u8; 4]) -> Result<()> {
        if table == 0 && button == 0 && action != self.button(0, 0) {
            bail!("the left button of mode A cannot be changed");
        }
        self.tables[Self::slot_range(table, button)].copy_from_slice(&action);
        let t = &mut self.tables[table * TABLE_STRIDE..][..TABLE_STRIDE];
        t[p::TABLE_LEN] = p::table_check(&t[..p::TABLE_LEN]);
        Ok(())
    }

    /// The reports that turn `old` into this configuration, changing only what differs.
    /// The polling rate only takes effect with a restart, so that comes last.
    pub fn changes_from(&self, old: &Config) -> Vec<Report> {
        let mut out = Vec::new();
        for level in 0..NUM_LEVELS {
            if self.colors[level] != old.colors[level] {
                let [r, g, b] = self.colors[level];
                out.push(p::write_report(p::COLORS_ADDR + 3 * level as u16, &[g, r, b]));
            }
        }
        let (new_dpi, old_dpi) = (p::pack_dpi(&self.dpi), p::pack_dpi(&old.dpi));
        for i in 0..new_dpi.len() {
            if new_dpi[i] != old_dpi[i] {
                out.push(p::write_report(p::DPI_ADDR + i as u16, &[new_dpi[i]]));
            }
        }
        // Re-selecting the level makes the mouse use the new values.
        if new_dpi != old_dpi {
            out.push(p::write_report(p::LEVEL_ADDR, &[0]));
        }
        if new_dpi != old_dpi || self.level != old.level {
            out.push(p::write_report(p::LEVEL_ADDR, &[self.level as u8]));
        }
        if self.led_mode != old.led_mode {
            out.push(p::write_report(p::LED_MODE_ADDR, &[self.led_mode.byte()]));
        }
        if self.led_speed != old.led_speed || self.half_bright != old.half_bright {
            let brightness = if self.half_bright { p::BRIGHTNESS_HALF } else { p::BRIGHTNESS_FULL };
            out.extend(p::block_reports(p::LED_BLOCK_ADDR, &p::led_block(self.led_speed, brightness)));
            out.extend(p::live_led_reports(self.led_speed, brightness));
        }
        if self.tables != old.tables {
            out.extend(p::block_reports(p::BUTTONS_ADDR, &self.tables));
            out.extend(p::buttons_reload_reports(self.mode));
        }
        if self.mode != old.mode {
            out.extend(p::mode_reports(self.mode));
        }
        if self.rate_byte != old.rate_byte {
            out.push(p::write_report(p::RATE_ADDR, &[self.rate_byte]));
            out.push(p::restart_report());
        }
        out
    }
}

fn colors(bytes: &[u8]) -> [[u8; 3]; NUM_LEVELS] {
    std::array::from_fn(|i| [bytes[i * 3 + 1], bytes[i * 3], bytes[i * 3 + 2]])
}

pub fn parse_color(text: &str) -> Result<[u8; 3]> {
    const NAMES: &[(&str, u32)] = &[
        ("red", 0xFF0000), ("green", 0x00FF00), ("blue", 0x0000FF), ("yellow", 0xFFFF00),
        ("cyan", 0x00FFFF), ("magenta", 0xFF00FF), ("pink", 0xFF00FF), ("white", 0xFFFFFF),
        ("orange", 0xFF8000), ("purple", 0x8000FF), ("black", 0x000000),
    ];
    let t = text.trim().to_lowercase();
    let t = t.trim_start_matches('#');
    let value = match NAMES.iter().find(|(n, _)| *n == t) {
        Some((_, c)) => *c,
        None => {
            let hex: String = if t.len() == 3 { t.chars().flat_map(|c| [c, c]).collect() } else { t.to_string() };
            match (hex.len(), u32::from_str_radix(&hex, 16)) {
                (6, Ok(c)) => c,
                _ => bail!("bad colour {text:?}: use a name or RRGGBB"),
            }
        }
    };
    Ok([(value >> 16) as u8, (value >> 8) as u8, value as u8])
}

/// Number of buttons, for callers that only use this module.
pub const BUTTONS: usize = NUM_BUTTONS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::parse_action;

    fn hex(r: &Report) -> String {
        r.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn factory_tables_have_valid_checks() {
        for t in 0..3 {
            let table = &factory::BUTTONS[t * TABLE_STRIDE..][..TABLE_STRIDE];
            assert_eq!(p::table_check(&table[..p::TABLE_LEN]), table[p::TABLE_LEN], "table {t}");
        }
    }

    #[test]
    fn check_byte_after_rebinding() {
        // Button 4 of mode A set to Volume Up gives check byte a9.
        let mut c = Config::factory(0);
        c.set_button(0, 3, parse_action("volume-up").unwrap()).unwrap();
        assert_eq!(c.tables[0xD0 - 0xC0..0xD4 - 0xC0], [0x03, 0x00, 0xE9, 0x00]);
        assert_eq!(c.tables[0xF0 - 0xC0], 0xA9);
    }

    #[test]
    fn table_checks() {
        // Button 4 of mode A, with the rest of the table as it came.
        for (name, check) in [("play-pause", 0x8D), ("next", 0xE5), ("volume-up", 0xA9), ("mute", 0xB6), ("web-home", 0x75), ("off", 0x5D), ("aim", 0x56)] {
            let mut c = Config::factory(0);
            c.set_button(0, 3, parse_action(name).unwrap()).unwrap();
            assert_eq!(c.tables[0x30], check, "{name}");
        }
    }

    #[test]
    fn buttons_map_to_slots() {
        // Address of each button's slot.
        let addr = [0xC0, 0xCC, 0xD8, 0xD0, 0xC4, 0xD4, 0xE0];
        let addr_b = [0xF1, 0xFD, 0x109, 0x101, 0xF5, 0x105, 0x111];
        for b in 0..NUM_BUTTONS {
            assert_eq!(Config::slot_range(0, b).start + 0xC0, addr[b]);
            assert_eq!(Config::slot_range(1, b).start + 0xC0, addr_b[b]);
        }
    }

    #[test]
    fn left_button_of_mode_a_is_locked() {
        let mut c = Config::factory(0);
        assert!(c.set_button(0, 0, parse_action("off").unwrap()).is_err());
        assert!(c.set_button(1, 0, parse_action("left").unwrap()).is_ok());
    }

    #[test]
    fn single_changes_match_expected_reports() {
        let old = Config::factory(0);

        let mut c = old.clone();
        c.set_rate(500).unwrap();
        let r = c.changes_from(&old);
        assert_eq!(hex(&r[0]), "40 05 00 04 af 00 00 01");
        assert_eq!(hex(&r[1]), "40 0e 00 00 00 00 00 00");

        let mut c = old.clone();
        c.level = 2;
        assert_eq!(hex(&c.changes_from(&old)[0]), "40 05 00 a6 02 00 00 01");

        let mut c = old.clone();
        c.led_mode = LedMode::Off;
        assert_eq!(hex(&c.changes_from(&old)[0]), "40 05 00 a5 87 00 00 01");

        // DPI 1 at the lowest step, re-selecting the level afterwards.
        let mut c = old.clone();
        c.dpi[0] = 0;
        let r = c.changes_from(&old);
        assert_eq!(hex(&r[0]), "40 05 00 2c 60 00 00 01");
        assert_eq!(hex(&r[1]), "40 05 00 a6 00 00 00 01");
        assert_eq!(hex(&r[2]), "40 05 00 a6 01 00 00 01");

        let mut c = old.clone();
        c.colors[0] = [0, 0, 0];
        assert_eq!(hex(&c.changes_from(&old)[0]), "40 05 00 57 00 00 00 03");

        assert!(old.changes_from(&old).is_empty());
    }

    #[test]
    fn colors_are_stored_green_first() {
        // As seen on the mouse: level 1 stores ff 00 00 and lights up green.
        let old = Config::factory(0);
        assert_eq!(old.colors[0], [0, 0xFF, 0]);
        assert_eq!(old.colors[1], [0xFF, 0, 0]);
        assert_eq!(old.colors[2], [0, 0, 0xFF]);
        assert_eq!(old.colors[4], [0xFF, 0, 0xFF]);
        assert_eq!(old.colors[5], [0, 0xFF, 0xFF]);
        let mut c = old.clone();
        c.colors[0] = [0x11, 0x22, 0x33];
        assert_eq!(hex(&c.changes_from(&old)[0]), "40 05 00 57 22 11 33 03");
    }

    #[test]
    fn single_colour_is_dpi_mode_with_one_colour() {
        let mut c = Config::factory(0);
        assert!(!c.is_single_colour());
        c.set_single_colour([1, 2, 3]);
        assert!(c.is_single_colour());
        assert_eq!(c.led_mode, LedMode::Dpi);
        assert_eq!(c.colors, [[1, 2, 3]; NUM_LEVELS]);
        c.colors[3] = [4, 5, 6];
        assert!(!c.is_single_colour());
    }

    #[test]
    fn led_block_reports() {
        let old = Config::factory(0);
        let mut c = old.clone();
        c.led_speed = 7;
        let r = c.changes_from(&old);
        assert_eq!(hex(&r[0]), "40 07 03 f0 00 10 00 00");
        assert_eq!(hex(&r[1]), "41 04 00 02 e0 00 1a 5f");
        assert_eq!(&hex(&r[2])[..11], "41 63 07 83");
        assert_eq!(&hex(&r[3])[..8], "41 00 d0");
        assert_eq!(hex(&r[4]), "40 01 07 c4 07 00 00 00");
        assert_eq!(hex(&r[5]), "40 01 07 c5 83 00 00 00");
        for (speed, check) in [0xD9, 0xDE, 0xDF, 0xDC, 0xDD, 0xD2, 0xD3, 0xD0].into_iter().enumerate() {
            assert_eq!(p::led_block(speed as u8, p::BRIGHTNESS_FULL)[15], check);
        }
        assert_eq!(p::led_block(2, p::BRIGHTNESS_HALF)[15], 0x5F);
    }

    #[test]
    fn actions_round_trip() {
        for (name, code) in p::ACTIONS {
            assert_eq!(p::action_name(*code), *name);
            assert_eq!(parse_action(name).unwrap(), *code);
        }
        assert_eq!(parse_action("keys:ctrl+shift+t").unwrap(), [0x02, 0x03, 0x17, 0]);
        assert_eq!(p::action_name([0x02, 0x03, 0x17, 0]), "keys:ctrl+shift+t");
        assert_eq!(parse_action(&p::action_name([0x0A, 1, 2, 3])).unwrap(), [0x0A, 1, 2, 3]);
        assert!(parse_action("keys:ctrl").is_err());
        assert!(parse_action("keys:a+b").is_err());
    }

    #[test]
    fn dpi_packing() {
        assert_eq!(p::unpack_dpi(&factory::DPI), [3, 6, 8, 9, 0xA, 0xB]);
        assert_eq!(p::pack_dpi(&[3, 6, 8, 9, 0xA, 0xB]), factory::DPI);
    }
}
