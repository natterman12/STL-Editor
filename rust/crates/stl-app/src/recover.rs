//! Recover Unsaved Work: the dialog 1.x shows at start-up (and from Help) when an earlier session
//! didn't close cleanly (`web/js/app.js` `checkRecovery`).
//!
//! Where the copies live is a [`RecoverStore`]: desktop reads the autosave folders
//! (`stl_platform::autosave`), the web edition reads its browser storage. Listing and loading
//! run off the UI thread; `show` only draws and polls.
//!
//! The app drives it:
//! - at start-up, once: [`Recover::start`];
//! - Help > Recover Unsaved Work (`help.recover`): [`Recover::open_manual`];
//! - every frame: [`Recover::show`] (it draws nothing when closed) and act on what it returns.

use crate::platform::Pending;
use crate::theme::Tokens;
use crate::ui::dialogs::button;
use egui::{Align, Area, CornerRadius, Frame, Id, Key, Layout, Margin, Modifiers, Order, RichText, Stroke, Vec2};

/// 1.x toast after a project was recovered.
pub const RECOVERED_TOAST: &str = "Recovered. Save it to keep it.";
/// 1.x toast when Help > Recover Unsaved Work finds nothing.
pub const NOTHING_TO_RECOVER: &str = "There is no unsaved work to recover.";
/// 1.x confirm when the open project has unsaved changes.
pub const REPLACE_CONFIRM: &str = "Replace the current unsaved work with the recovered project?";
/// 1.x busy note while a project is rebuilt.
pub const RECOVERING_MSG: &str = "Recovering…";

/// One copy that can be recovered.
#[derive(Clone, Debug, PartialEq)]
pub struct RecoverEntry {
    pub id: String,
    pub title: String,
    pub path: Option<String>,
    /// Seconds since 1970 when the copy was written.
    pub saved: f64,
    pub steps: usize,
}

/// A recovered project, ready for `Command::Open`.
#[derive(Clone, Debug)]
pub struct Recovered {
    /// `<title>.stlx`; the engine names the document from it.
    pub name: String,
    /// The project file. It carries `recovered: true`, so the engine opens it as unsaved work and
    /// writes a new autosave copy straight away.
    pub bytes: Vec<u8>,
    /// Where the project was saved last, if it ever was (desktop).
    pub path: Option<String>,
}

pub enum RecoverAction {
    /// Send `Command::Open { name, bytes }`, then mark the project unsaved (the app's clean
    /// marker must differ from the project as opened), set its remembered path to `path`, and toast
    /// [`RECOVERED_TOAST`].
    Open(Recovered),
    /// Show this as a toast (`warn`: as a warning).
    Message { text: String, warn: bool },
}

/// Where the copies are kept.
pub trait RecoverStore {
    /// The copies of earlier sessions (never this window's own, nor a running window's), oldest first.
    fn list(&self) -> Pending<Vec<RecoverEntry>>;
    /// The project file for a copy. The copy is discarded once its bytes are in hand: opening it
    /// writes a new one.
    fn load(&self, id: &str) -> Pending<Result<Vec<u8>, String>>;
    fn discard(&self, id: &str);
}

enum State {
    Closed,
    /// Waiting for the list. `manual`: the user asked, so "nothing" is answered.
    Listing { pending: Pending<Vec<RecoverEntry>>, manual: bool },
    Open { entries: Vec<RecoverEntry> },
    /// Recover was pressed while the open project has unsaved changes.
    Confirm { entries: Vec<RecoverEntry>, id: String },
    Loading { pending: Pending<Result<Vec<u8>, String>>, entry: RecoverEntry },
}

pub struct Recover {
    store: Box<dyn RecoverStore>,
    web: bool,
    state: State,
}

impl Recover {
    pub fn new(store: Box<dyn RecoverStore>, web: bool) -> Recover {
        Recover { store, web, state: State::Closed }
    }

    /// The desktop store: autosave folders under `paths::data_dir()/autosave`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn desktop() -> Recover {
        Recover::new(Box::new(DesktopStore { root: stl_platform::paths::data_dir().join("autosave") }), false)
    }

    /// The browser store: IndexedDB database `stle2`.
    #[cfg(target_arch = "wasm32")]
    pub fn web() -> Recover {
        Recover::new(Box::new(crate::web::store::WebStore), true)
    }

    /// Start-up offer: lists what an earlier session left and opens the dialog if there is any.
    pub fn start(&mut self) {
        if matches!(self.state, State::Closed) {
            self.state = State::Listing { pending: self.store.list(), manual: false };
        }
    }

    /// Help > Recover Unsaved Work: the same, and a toast when there is nothing.
    pub fn open_manual(&mut self) {
        if matches!(self.state, State::Closed) {
            self.state = State::Listing { pending: self.store.list(), manual: true };
        }
    }

    pub fn is_open(&self) -> bool {
        !matches!(self.state, State::Closed | State::Listing { .. })
    }

    /// Draw the dialog and poll the store. `dirty`: the open project has unsaved changes.
    pub fn show(&mut self, ctx: &egui::Context, tk: &Tokens, dirty: bool) -> Option<RecoverAction> {
        match &mut self.state {
            State::Closed => return None,
            State::Listing { pending, manual } => {
                let Some(entries) = pending.take() else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                    return None;
                };
                let manual = *manual;
                if entries.is_empty() {
                    self.state = State::Closed;
                    return manual.then(|| RecoverAction::Message { text: NOTHING_TO_RECOVER.into(), warn: false });
                }
                self.state = State::Open { entries };
                ctx.request_repaint();
                return None;
            }
            State::Loading { pending, entry } => {
                let Some(result) = pending.take() else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                    return None;
                };
                let entry = entry.clone();
                self.state = State::Closed;
                return Some(match result {
                    Ok(bytes) => RecoverAction::Open(Recovered { name: format!("{}.stlx", entry.title), bytes, path: entry.path }),
                    Err(text) => RecoverAction::Message { text, warn: true },
                });
            }
            _ => {}
        }
        self.draw(ctx, tk, dirty)
    }

    fn draw(&mut self, ctx: &egui::Context, tk: &Tokens, dirty: bool) -> Option<RecoverAction> {
        let screen = ctx.content_rect();
        Area::new(Id::unique("recover-scrim")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
            ui.painter().rect_filled(r, 0.0, tk.scrim);
        });
        let esc = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let web = self.web;
        let mut next: Option<State> = None;
        let mut discard: Option<String> = None;
        let mut recover: Option<String> = None;
        let mut later = esc;
        let mut confirm_ok = false;
        let mut confirm_cancel = false;
        let entries: Vec<RecoverEntry> = match &self.state {
            State::Open { entries } | State::Confirm { entries, .. } => entries.clone(),
            _ => return None,
        };
        let confirming = matches!(self.state, State::Confirm { .. });
        Area::new(Id::unique("recover-card")).order(Order::Tooltip).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            Frame::new()
                .fill(tk.panel)
                .stroke(Stroke::new(1.0, tk.line))
                .corner_radius(CornerRadius::same(9))
                .shadow(tk.shadow_lg)
                .inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 14 })
                .show(ui, |ui| {
                    ui.set_min_width(300.0);
                    ui.set_max_width(480.0);
                    let bold = egui::FontId::new(15.0, egui::FontFamily::Name("bold".into()));
                    if confirming {
                        ui.label(RichText::new("STL Editor").font(bold).color(tk.strong));
                        ui.add_space(8.0);
                        ui.label(RichText::new(REPLACE_CONFIRM).size(12.0).color(tk.text));
                        ui.add_space(14.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            confirm_ok = button(ui, tk, "OK", true).clicked();
                            confirm_cancel = button(ui, tk, "Cancel", false).clicked();
                        });
                        return;
                    }
                    ui.label(RichText::new("Recover unsaved work?").font(bold).color(tk.strong));
                    ui.add_space(8.0);
                    ui.label(RichText::new(intro(web)).size(12.0).color(tk.text));
                    ui.add_space(4.0);
                    for e in &entries {
                        Frame::new().stroke(Stroke::new(1.0, tk.line)).corner_radius(CornerRadius::same(tk.radius as u8)).inner_margin(Margin::symmetric(10, 8)).outer_margin(Margin::symmetric(0, 3)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    let name = ui.label(RichText::new(&e.title).size(12.5).strong().color(tk.strong));
                                    if let Some(p) = &e.path {
                                        name.on_hover_text(p);
                                    }
                                    ui.label(RichText::new(detail(e)).size(11.5).color(tk.muted));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    if button(ui, tk, "Recover", true).clicked() {
                                        recover = Some(e.id.clone());
                                    }
                                    if button(ui, tk, "Discard", false).clicked() {
                                        discard = Some(e.id.clone());
                                    }
                                });
                            });
                        });
                    }
                    ui.add_space(14.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, tk, "Later", false).clicked() {
                            later = true;
                        }
                    });
                });
        });

        if confirming {
            if confirm_ok {
                if let State::Confirm { id, .. } = &self.state {
                    let id = id.clone();
                    return self.begin_load(&id, &entries);
                }
            }
            if confirm_cancel || esc {
                next = Some(State::Open { entries });
            }
        } else if let Some(id) = discard {
            self.store.discard(&id);
            let left: Vec<RecoverEntry> = entries.into_iter().filter(|e| e.id != id).collect();
            next = Some(if left.is_empty() { State::Closed } else { State::Open { entries: left } });
        } else if let Some(id) = recover {
            if dirty {
                next = Some(State::Confirm { entries, id });
            } else {
                return self.begin_load(&id, &entries);
            }
        } else if later {
            next = Some(State::Closed);
        }
        if let Some(s) = next {
            self.state = s;
        }
        None
    }

    fn begin_load(&mut self, id: &str, entries: &[RecoverEntry]) -> Option<RecoverAction> {
        let entry = entries.iter().find(|e| e.id == id)?.clone();
        self.state = State::Loading { pending: self.store.load(id), entry };
        None
    }

    /// True while a copy is being read (the app can show [`RECOVERING_MSG`]).
    pub fn is_loading(&self) -> bool {
        matches!(self.state, State::Loading { .. })
    }
}

/// 1.x's sentence under the title.
fn intro(web: bool) -> &'static str {
    if web {
        "This work wasn’t saved before STL Editor was last closed. A copy was kept in this browser:"
    } else {
        "STL Editor didn’t close normally last time. This work was saved automatically:"
    }
}

/// 1.x's second line: `3 steps · autosaved <when>`.
fn detail(e: &RecoverEntry) -> String {
    format!("{} step{} · autosaved {}", e.steps, if e.steps == 1 { "" } else { "s" }, when(e.saved))
}

/// `2026-11-14 10:32` in the computer's time zone (1.x used the browser's locale format).
pub fn when(saved: f64) -> String {
    if saved <= 0.0 {
        return String::new();
    }
    let t = saved + local_offset_secs(saved) as f64;
    let secs = t.floor() as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // civil date from days since 1970-01-01
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

/// Seconds to add to UTC for the computer's local time at `utc`.
#[cfg(target_arch = "wasm32")]
fn local_offset_secs(utc: f64) -> i64 {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(utc * 1000.0));
    -(d.get_timezone_offset() * 60.0) as i64
}

/// The C library's `localtime`: the date and time as seen in the local time zone.
#[cfg(not(target_arch = "wasm32"))]
fn local_offset_secs(utc: f64) -> i64 {
    #[repr(C)]
    struct Tm {
        sec: i32,
        min: i32,
        hour: i32,
        mday: i32,
        mon: i32,
        year: i32,
        wday: i32,
        yday: i32,
        isdst: i32,
        // glibc and macOS add `gmtoff` and `zone` after these; room for both
        rest: [i64; 2],
    }
    unsafe extern "C" {
        #[cfg(windows)]
        fn _localtime64_s(out: *mut Tm, t: *const i64) -> i32;
        #[cfg(not(windows))]
        fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let t = utc.floor() as i64;
    let mut tm = Tm { sec: 0, min: 0, hour: 0, mday: 0, mon: 0, year: 0, wday: 0, yday: 0, isdst: 0, rest: [0; 2] };
    // SAFETY: both functions fill `tm` (at least the nine ints, at most 56 bytes) and read `t`.
    let ok = unsafe {
        #[cfg(windows)]
        {
            _localtime64_s(&mut tm, &t) == 0
        }
        #[cfg(not(windows))]
        {
            !localtime_r(&t, &mut tm).is_null()
        }
    };
    if !ok {
        return 0;
    }
    // local time of day minus UTC time of day, folded into -12 h ..= +14 h
    let local = i64::from(tm.hour) * 3600 + i64::from(tm.min) * 60 + i64::from(tm.sec);
    let mut off = local - t.rem_euclid(86400);
    if off > 14 * 3600 {
        off -= 86400;
    } else if off < -12 * 3600 {
        off += 86400;
    }
    off
}

/// The autosave folders of an earlier desktop session.
#[cfg(not(target_arch = "wasm32"))]
pub struct DesktopStore {
    pub root: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl RecoverStore for DesktopStore {
    fn list(&self) -> Pending<Vec<RecoverEntry>> {
        let (root, out) = (self.root.clone(), Pending::new());
        let answer = out.clone();
        std::thread::spawn(move || {
            let list = stl_platform::autosave::recoverable(&root, None)
                .into_iter()
                .map(|s| RecoverEntry { id: s.id, title: s.title, path: s.path, saved: s.saved, steps: s.steps })
                .collect();
            answer.set(list);
        });
        out
    }

    fn load(&self, id: &str) -> Pending<Result<Vec<u8>, String>> {
        let (root, id, out) = (self.root.clone(), id.to_string(), Pending::new());
        let answer = out.clone();
        std::thread::spawn(move || {
            let r = project_bytes(&root, &id);
            if r.is_ok() {
                stl_platform::autosave::discard(&root, &id);
            }
            answer.set(r);
        });
        out
    }

    fn discard(&self, id: &str) {
        let (root, id) = (self.root.clone(), id.to_string());
        std::thread::spawn(move || stl_platform::autosave::discard(&root, &id));
    }
}

/// The autosave folder as the project file the engine opens (nothing is evaluated here).
#[cfg(not(target_arch = "wasm32"))]
pub fn project_bytes(root: &std::path::Path, id: &str) -> Result<Vec<u8>, String> {
    let (json, sources) = stl_platform::autosave::load(root, id).map_err(|e| format!("Couldn't recover that copy: {e}"))?;
    let parts: Vec<(String, String, String, Vec<u8>)> = sources.into_iter().map(|s| (s.id, s.name, s.ext, s.data)).collect();
    stl_engine::stlx::parts_to_bytes(&json, &parts).map_err(|e| e.0)
}
