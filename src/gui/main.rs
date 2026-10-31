//! img349ctl-gui: graphical settings for the Inca IMG-349 gaming mouse.

mod i18n;

use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};

use i18n::{Strings, ACTION_NAMES};
use img349ctl::config::Config;
use img349ctl::device::{self, Mouse};
use img349ctl::protocol::{self as p, LedMode, ACTIONS, DPI_VALUES, RATES};

const GREEN: Color32 = Color32::from_rgb(60, 170, 90);
const RED: Color32 = Color32::from_rgb(220, 60, 60);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dpi,
    Led,
    Buttons,
    Other,
}

enum Status {
    None,
    Ok(String),
    Err(String),
}

struct App {
    t: &'static Strings,
    turkish: bool,
    /// The settings being edited and the ones the mouse has, once it has been read.
    conf: Option<Config>,
    saved: Option<Config>,
    tab: Tab,
    /// Which mode's button table is shown (0 = A, 1 = B).
    table: usize,
    /// Text fields for custom button functions, one per button.
    custom: [String; p::NUM_BUTTONS],
    status: Status,
    mouse: Option<Mouse>,
    last_poll: Instant,
    /// The mouse restarts after a polling rate change: don't touch it until it is back.
    wait_until: Instant,
}

impl App {
    fn new() -> Self {
        let turkish = i18n::turkish();
        App {
            t: if turkish { &i18n::TR } else { &i18n::EN },
            turkish,
            conf: None,
            saved: None,
            tab: Tab::Dpi,
            table: 0,
            custom: Default::default(),
            status: Status::None,
            mouse: None,
            last_poll: Instant::now() - Duration::from_secs(10),
            wait_until: Instant::now(),
        }
    }

    fn label(&self, id: &str) -> String {
        match ACTION_NAMES.iter().find(|(n, _, _)| *n == id) {
            Some((_, tr, en)) => (if self.turkish { tr } else { en }).to_string(),
            None => id.to_string(),
        }
    }

    fn sync_custom(&mut self) {
        let Some(c) = &self.conf else { return };
        for b in 0..p::NUM_BUTTONS {
            self.custom[b] = p::action_name(c.button(self.table, b));
        }
    }

    /// Finds the mouse and reads it. Later polls only follow the level, which the DPI buttons change.
    fn poll(&mut self) {
        if self.last_poll.elapsed() < Duration::from_millis(700) || Instant::now() < self.wait_until {
            return;
        }
        self.last_poll = Instant::now();
        let level = self.mouse.as_ref().and_then(|m| m.read(p::LEVEL_ADDR, 1).ok());
        match level {
            Some(l) => {
                let level = l[0] as usize;
                if level < p::NUM_LEVELS && self.saved.as_ref().is_some_and(|s| s.level != level) {
                    if let (Some(s), Some(c)) = (&mut self.saved, &mut self.conf) {
                        s.level = level;
                        c.level = level;
                    }
                }
            }
            None => self.connect(),
        }
    }

    fn connect(&mut self) {
        self.mouse = None;
        let result = Mouse::open(None).and_then(|m| Ok((Config::read(&m)?, m)));
        match result {
            Ok((c, m)) => {
                self.mouse = Some(m);
                // A change waiting for Apply stays when the mouse comes back.
                if self.saved.as_ref() != Some(&c) {
                    self.conf = Some(c.clone());
                    self.saved = Some(c);
                    self.sync_custom();
                }
                if matches!(self.status, Status::Err(_)) {
                    self.status = Status::None;
                }
            }
            Err(e) => {
                if !device::find().is_empty() {
                    self.status = Status::Err(format!("{e:#}"));
                }
            }
        }
    }

    fn apply(&mut self) {
        let (Some(conf), Some(saved), Some(mouse)) = (&self.conf, &self.saved, &self.mouse) else { return };
        let restart = conf.rate_byte != saved.rate_byte;
        let reports = conf.changes_from(saved);
        self.status = match mouse.send_all(&reports) {
            Ok(()) if !restart && !conf.verify(mouse) => Status::Err(self.t.not_taken.to_string()),
            Ok(()) => Status::Ok(self.t.applied.to_string()),
            Err(e) => Status::Err(format!("{e:#}")),
        };
        if !matches!(self.status, Status::Err(_)) {
            self.saved = self.conf.clone();
        }
        if restart {
            self.mouse = None;
            self.wait_until = Instant::now() + Duration::from_secs(3);
        }
    }

    fn dpi_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.t;
        let Some(c) = &mut self.conf else { return };
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}:", t.current_dpi)).size(16.0));
            let level = self.saved.as_ref().map_or(c.level, |s| s.level);
            let dpi = self.saved.as_ref().map_or(0, |s| s.dpi_value(level));
            ui.label(RichText::new(format!("{dpi} ({} {})", t.level, level + 1)).size(16.0).strong().color(GREEN));
        });
        ui.add_space(10.0);
        egui::Grid::new("dpi").num_columns(4).spacing([28.0, 10.0]).striped(true).show(ui, |ui| {
            for h in [t.level, t.current, "DPI", t.color] {
                ui.strong(h);
            }
            ui.end_row();
            for i in 0..p::NUM_LEVELS {
                ui.label(format!("{}", i + 1));
                ui.radio_value(&mut c.level, i, "");
                egui::ComboBox::from_id_salt(("dpi", i))
                    .selected_text(c.dpi_value(i).to_string())
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for (code, v) in DPI_VALUES.iter().enumerate() {
                            ui.selectable_value(&mut c.dpi[i], code as u8, v.to_string());
                        }
                    });
                ui.color_edit_button_srgb(&mut c.colors[i]);
                ui.end_row();
            }
        });
        ui.add_space(12.0);
        ui.label(RichText::new(t.color_note).weak());
    }

    fn led_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.t;
        let Some(c) = &mut self.conf else { return };
        ui.strong(t.led_mode);
        ui.add_space(4.0);
        let single = c.is_single_colour();
        ui.horizontal(|ui| {
            let mut rgb = c.colors[c.level];
            if ui.radio(single, t.led_single).clicked() && !single {
                c.set_single_colour(rgb);
            }
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                c.set_single_colour(rgb);
            }
        });
        ui.add_space(6.0);
        egui::Grid::new("led").num_columns(4).spacing([24.0, 8.0]).show(ui, |ui| {
            for (i, mode) in LedMode::ALL.into_iter().enumerate() {
                let selected = c.led_mode == mode && !(single && mode == LedMode::Dpi);
                if ui.radio(selected, t.led_names[i]).clicked() && !selected {
                    // With one colour on every level this would still be the single colour.
                    if mode == LedMode::Dpi && c.uniform_colors() {
                        c.colors = match &self.saved {
                            Some(s) if !s.uniform_colors() => s.colors,
                            _ => Config::factory(c.mode).colors,
                        };
                    }
                    c.led_mode = mode;
                }
                if i % 4 == 3 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(t.led_speed);
            ui.label(RichText::new(t.fast).weak());
            ui.add(egui::Slider::new(&mut c.led_speed, 0..=p::LED_SPEED_MAX));
            ui.label(RichText::new(t.slow).weak());
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(t.brightness);
            ui.radio_value(&mut c.half_bright, false, t.bright_full);
            ui.radio_value(&mut c.half_bright, true, t.bright_half);
        });
    }

    fn buttons_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.t;
        ui.horizontal(|ui| {
            for (i, name) in [t.mode_a, t.mode_b].into_iter().enumerate() {
                if ui.selectable_label(self.table == i, name).clicked() && self.table != i {
                    self.table = i;
                    self.sync_custom();
                }
            }
            ui.add_space(16.0);
            if let Some(c) = &mut self.conf {
                ui.label(t.active_mode);
                ui.radio_value(&mut c.mode, 0, "A");
                ui.radio_value(&mut c.mode, 1, "B");
            }
        });
        ui.add_space(8.0);
        egui::Grid::new("buttons").num_columns(3).spacing([16.0, 10.0]).striped(true).show(ui, |ui| {
            for b in 0..p::NUM_BUTTONS {
                ui.label(format!("{}  {}", b + 1, t.button_names[b]));
                let locked = self.table == 0 && b == 0;
                let Some(current) = self.conf.as_ref().map(|c| c.button(self.table, b)) else { continue };
                let shown = self.label(&p::action_name(current));
                let mut chosen = None;
                ui.add_enabled_ui(!locked, |ui| {
                    egui::ComboBox::from_id_salt(("button", b)).selected_text(shown).width(200.0).height(420.0).show_ui(ui, |ui| {
                        for (group, range) in i18n::ACTION_GROUPS {
                            ui.label(RichText::new(if self.turkish { group.0 } else { group.1 }).small().weak());
                            for (name, code) in &ACTIONS[range.clone()] {
                                if ui.selectable_label(*code == current, self.label(name)).clicked() {
                                    chosen = Some(*code);
                                }
                            }
                            ui.separator();
                        }
                    });
                })
                .response
                .on_disabled_hover_text(t.left_locked);
                if let (Some(code), Some(c)) = (chosen, &mut self.conf) {
                    if c.set_button(self.table, b, code).is_ok() {
                        self.custom[b] = p::action_name(code);
                    }
                }
                let parsed = p::parse_action(&self.custom[b]);
                let edit = egui::TextEdit::singleline(&mut self.custom[b])
                    .desired_width(150.0)
                    .hint_text("keys:ctrl+t")
                    .text_color_opt(parsed.is_err().then_some(RED));
                let resp = ui.add_enabled(!locked, edit).on_hover_text(t.custom_hint);
                if resp.changed() {
                    if let (Ok(code), Some(c)) = (p::parse_action(&self.custom[b]), &mut self.conf) {
                        let _ = c.set_button(self.table, b, code);
                    }
                }
                ui.end_row();
            }
        });
        ui.add_space(8.0);
        ui.label(RichText::new(t.custom_hint).weak());
    }

    fn other_tab(&mut self, ui: &mut egui::Ui) {
        let t = self.t;
        let Some(c) = &mut self.conf else { return };
        ui.horizontal(|ui| {
            ui.label(t.rate);
            let current = c.rate_hz();
            for hz in RATES {
                if ui.radio(current == hz, format!("{hz} Hz")).clicked() {
                    let _ = c.set_rate(hz);
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new(t.rate_note).weak());
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        let t = self.t;
        let dirty = self.conf != self.saved;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let connected = self.mouse.is_some();
            let (color, text) = if connected { (GREEN, t.mouse_found) } else { (RED, t.mouse_missing) };
            status_dot(ui, color);
            ui.colored_label(color, text);
            if let (true, Some(s)) = (connected, &self.saved) {
                ui.label(RichText::new(format!("· {} DPI", s.dpi_value(s.level))).strong());
            }
        });
        ui.horizontal(|ui| {
            match &self.status {
                Status::Err(e) => ui.colored_label(RED, e),
                _ if dirty => ui.label(RichText::new(t.unsaved).italics()),
                Status::Ok(m) => ui.colored_label(GREEN, m),
                Status::None => ui.label(""),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(dirty && self.mouse.is_some(), egui::Button::new(RichText::new(t.apply).strong())).clicked() {
                    self.apply();
                }
                if ui.add_enabled(dirty, egui::Button::new(t.revert)).clicked() {
                    self.conf = self.saved.clone();
                    self.sync_custom();
                    self.status = Status::None;
                }
                if ui.add_enabled(self.conf.is_some(), egui::Button::new(t.defaults)).clicked() {
                    let mode = self.conf.as_ref().map_or(0, |c| c.mode);
                    self.conf = Some(Config::factory(mode));
                    self.sync_custom();
                }
            });
        });
        ui.add_space(6.0);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        ctx.request_repaint_after(Duration::from_millis(700));
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let t = self.t;
                for (tab, name) in [(Tab::Dpi, t.tab_dpi), (Tab::Led, t.tab_led), (Tab::Buttons, t.tab_buttons), (Tab::Other, t.tab_other)] {
                    if ui.selectable_value(&mut self.tab, tab, RichText::new(name).size(15.0)).clicked() {
                        // The tab strip was already drawn with the old selection.
                        ctx.request_repaint();
                    }
                }
            });
            ui.add_space(6.0);
        });
        egui::TopBottomPanel::bottom("bar").show(ctx, |ui| self.bottom_bar(ui));
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);
                if self.conf.is_none() {
                    ui.label(self.t.waiting);
                    return;
                }
                match self.tab {
                    Tab::Dpi => self.dpi_tab(ui),
                    Tab::Led => self.led_tab(ui),
                    Tab::Buttons => self.buttons_tab(ui),
                    Tab::Other => self.other_tab(ui),
                }
            });
        });
    }
}

/// A small filled circle; drawn rather than typed because the bundled fonts lack "●".
fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// Prints log records to stderr when IMG349_LOG is set (error, warn, info, debug or trace),
/// so graphics driver problems can be diagnosed without a debugger.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Re-executes this program with Mesa's software OpenGL; returns only if that is not possible.
fn restart_with_software_gl() {
    if std::env::var_os("LIBGL_ALWAYS_SOFTWARE").is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    log::warn!("only OpenGL ES 2.0 is available; restarting with software rendering");
    let err = Command::new(exe).args(std::env::args_os().skip(1)).env("LIBGL_ALWAYS_SOFTWARE", "1").exec();
    log::warn!("could not restart with software rendering: {err}");
}

fn init_logging() {
    let Ok(level) = std::env::var("IMG349_LOG") else { return };
    let level = level.parse().unwrap_or(log::LevelFilter::Debug);
    if log::set_logger(&StderrLogger).is_ok() {
        log::set_max_level(level);
    }
}

fn main() -> eframe::Result {
    init_logging();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(if i18n::turkish() { i18n::TR.title } else { i18n::EN.title })
            .with_app_id("img349ctl-gui")
            .with_inner_size([640.0, 520.0])
            .with_min_inner_size([560.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "img349ctl-gui",
        options,
        Box::new(|cc| {
            if let Some(gl) = &cc.gl {
                use eframe::glow::HasContext;
                let v = gl.version();
                log::debug!("GL {}.{} embedded={} {}", v.major, v.minor, v.is_embedded, v.vendor_info);
                // Old GPUs (e.g. Intel Ironlake) only offer OpenGL ES 2.0 here, where egui draws a
                // black window. Mesa's software renderer works fine for a small settings window.
                if v.is_embedded && v.major < 3 {
                    restart_with_software_gl();
                }
            }
            Ok(Box::new(App::new()))
        }),
    )
}
