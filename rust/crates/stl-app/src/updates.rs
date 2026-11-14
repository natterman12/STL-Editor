//! The 2.0 update check, banner and Update Settings (milestone 5, Task 4): 1.x's `checkUpdates`,
//! `showUpdateBanner` and `updateSettings` (`web/js/app.js`) over `stl_platform::update`.
//!
//! Desktop only; the web edition gets new versions with the page, so its Help menu has neither
//! item ([`help_menu_items`]).
//!
//! The app drives it (desktop):
//! - once, a few seconds after start-up (1.x waits 6 s): [`Updates::start_background`];
//! - Help > Check for Updates… (`help.check_updates`): [`Updates::check_now`];
//! - Help > Update Settings… (`help.update_settings`): [`Updates::open_settings`];
//! - every frame: [`Updates::show`] (draws the banner and the settings dialog when open) and act
//!   on what it returns;
//! - once, after the first frame has been drawn: [`finish_swap_after_first_frame`].
//!
//! The network work runs on background threads; the UI thread only reads and writes the
//! settings (`update_check`, `update_last`, `update_cache`, `update_skip`).

/// The Help menu's update items (id, label), 1.x's texts. None on the web edition.
pub fn help_menu_items(web: bool) -> Vec<(&'static str, &'static str)> {
    if web {
        Vec::new()
    } else {
        vec![("help.check_updates", "Check for Updates…"), ("help.update_settings", "Update Settings…")]
    }
}

/// 1.x's busy note while Check for Updates asks GitHub.
pub const CHECKING: &str = "Checking for updates…";
/// 1.x's toast when Update Settings turns the checks off.
pub const CHECKS_OFF: &str = "Update checks are off.";
/// The banner's note while the new exe downloads (spec 2.7 decision 5: no percentage).
pub const DOWNLOADING: &str = "Downloading update…";
/// 1.x's note once the new version is in place.
pub const RESTARTING: &str = "Restarting STL Editor…";
/// 1.x's banner line for a copy that can update itself (the portable build's).
pub const INSTALL_LINE: &str = "Update now downloads it, swaps it in and restarts STL Editor. Unsaved work is kept for Recover Unsaved Work.";
/// The banner line for a copy that can't (not run from the install folder). 1.x says "Download
/// the new zip, unzip it, and use it instead of this copy."; 2.0 ships a plain exe.
pub const DOWNLOAD_LINE: &str = "Download the new version and use it instead of this copy.";
/// Update now while another 2.0 window is open (spec 2.7 decision 2). New text.
pub const OTHER_WINDOWS: &str = "Close the other STL Editor 2.0 windows first, then press Update now again.";
/// The Update Settings dialog (2.0: on/off; 1.x asked for a repository name).
pub const SETTINGS_TITLE: &str = "Update Settings";
pub const SETTINGS_CHECK: &str = "Check for new versions once a day";
pub const SETTINGS_NOTE: &str = "STL Editor asks GitHub (github.com/natterman12/STL-Editor) for the newest version. Nothing about you or your files is sent.";

/// 1.x's banner title.
pub fn available_title(version: &str) -> String {
    format!("STL Editor {version} is available")
}

/// 1.x's toast when nothing newer is found.
pub fn latest_text(current: &str) -> String {
    format!("You have the latest version ({current}).")
}

/// The release's page (1.x opened the release's `html_url` for What's new and Download).
pub fn release_page(tag: &str) -> String {
    format!("https://github.com/natterman12/STL-Editor/releases/tag/{tag}")
}

/// The banner's buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerButton {
    Skip,
    Later,
    WhatsNew,
    UpdateNow,
    Download,
    Cancel,
    Close,
    DownloadInstead,
}

impl BannerButton {
    pub fn label(self) -> &'static str {
        match self {
            BannerButton::Skip => "Skip this version",
            BannerButton::Later => "Later",
            BannerButton::WhatsNew => "What’s new",
            BannerButton::UpdateNow => "Update now",
            BannerButton::Download => "Download",
            BannerButton::Cancel => "Cancel",
            BannerButton::Close => "Close",
            BannerButton::DownloadInstead => "Download instead",
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn primary(self) -> bool {
        matches!(self, BannerButton::UpdateNow | BannerButton::Download | BannerButton::DownloadInstead)
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const BUTTONS: [BannerButton; 8] = [
    BannerButton::Skip,
    BannerButton::Later,
    BannerButton::WhatsNew,
    BannerButton::UpdateNow,
    BannerButton::Download,
    BannerButton::Cancel,
    BannerButton::Close,
    BannerButton::DownloadInstead,
];

/// What the app does after [`Updates::show`] / [`Updates::poll`] / [`Updates::press`].
#[derive(Clone, Debug, PartialEq)]
pub enum UpdateAction {
    /// Show this as a toast (`warn`: as a warning).
    Toast { text: String, warn: bool },
    /// Open this page in the browser.
    OpenUrl(String),
    /// The new exe is in place at `exe`. Close this window *without ending the autosave
    /// session* (its unsaved work stays for Recover Unsaved Work, as 1.x), and start `exe` once
    /// this process has ended (`App::restart_helper_command(&exe, std::process::id())`). Use
    /// `exe`, not `current_exe()`, which may now name the `.old.exe`.
    Restart { exe: std::path::PathBuf },
}

/// What the banner shows (for drawing and for tests).
#[derive(Clone, Debug, PartialEq)]
pub struct BannerView {
    pub version: String,
    pub title: String,
    pub line: String,
    /// Under the line: why Update now didn't go ahead, or what it is doing.
    pub note: Option<String>,
    /// Left to right.
    pub buttons: Vec<&'static str>,
}

#[cfg(not(target_arch = "wasm32"))]
pub use desktop::*;

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use std::fs::{File, OpenOptions, TryLockError};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use egui::{Align, Align2, Area, CornerRadius, Frame, Id, Key, Layout, Margin, Modifiers, Order, RichText, Stroke, Vec2};
    use serde_json::{json, Value};
    use stl_platform::settings::keys;
    use stl_platform::update::{self, Release};

    use super::*;
    use crate::platform::{Pending, Platform};
    use crate::theme::Tokens;
    use crate::ui::dialogs::button;

    /// 1.x checks again after 20 hours, which is what "once a day" means there.
    const DAY_SECS: f64 = 20.0 * 3600.0;
    /// 2.0 only: when a window started the day's check (unix seconds; `null` when none is running).
    /// Another window starting meanwhile waits for that answer instead of asking too.
    pub const UPDATE_CLAIM: &str = "update_checking";
    /// A claim older than this belongs to a check that never finished (the request times out
    /// after 8 s).
    const CLAIM_SECS: f64 = 60.0;

    enum Phase {
        Offer { note: Option<String> },
        Downloading { pending: Pending<Result<PathBuf, String>> },
        Restarting,
        Failed(String),
    }

    struct Banner {
        rel: Release,
        phase: Phase,
    }

    pub struct Updates {
        api: String,
        current: String,
        /// The running exe, when this copy can update itself (it runs from the install folder).
        running_exe: Option<PathBuf>,
        /// `data_dir()/windows`: one locked file per open 2.0 window.
        windows_dir: PathBuf,
        /// This window's lock there, held for as long as the window (this `Updates`) lives.
        own: Option<WindowLock>,
        checking: Option<(Pending<Result<Option<Release>, String>>, bool)>,
        /// Downloads that were cancelled but haven't finished, with their release tag: each file
        /// is deleted when its download ends, unless Update now is pressed again for the same
        /// release first (it then carries on with that download).
        orphans: Vec<(String, Pending<Result<PathBuf, String>>)>,
        /// This window wrote [`UPDATE_CLAIM`] for the check it is running.
        claimed: bool,
        banner: Option<Banner>,
        /// The Update Settings dialog, open, with its checkbox.
        settings: Option<bool>,
    }

    impl Default for Updates {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Updates {
        /// The app's, made once per window at start-up: GitHub, this build's version, and
        /// Update now only for the installed Windows exe (the releases carry only a Windows
        /// asset). Takes this window's lock in `data_dir()/windows`.
        pub fn new() -> Updates {
            let exe = std::env::current_exe().ok();
            let installed = cfg!(windows) && exe.as_deref().is_some_and(is_installed);
            Updates::custom(
                update::RELEASES_API,
                env!("CARGO_PKG_VERSION"),
                if installed { exe } else { None },
                stl_platform::paths::data_dir().join("windows"),
            )
        }

        /// Tests: another releases list, version, exe (`None`: can't update itself) and windows
        /// folder. Each `Updates` holds a window lock there, so two of them are two windows.
        pub fn custom(api: &str, current: &str, running_exe: Option<PathBuf>, windows_dir: PathBuf) -> Updates {
            let own = WindowLock::take(&windows_dir).inspect_err(|e| eprintln!("Couldn't take the window lock: {e}")).ok();
            Updates {
                api: api.to_string(),
                current: current.to_string(),
                running_exe,
                windows_dir,
                own,
                checking: None,
                orphans: Vec::new(),
                claimed: false,
                banner: None,
                settings: None,
            }
        }

        /// Tests: ask another releases list from now on.
        #[doc(hidden)]
        pub fn set_api(&mut self, api: &str) {
            self.api = api.to_string();
        }

        fn others_open(&self) -> bool {
            other_windows_open(&self.windows_dir, self.own.as_ref().map(|l| l.name.as_str()))
        }

        /// The start-up check: at most once a day across windows (`update_last`, written after a
        /// check that worked, as 1.x), else the cached answer (`update_cache`); nothing when checks
        /// are off. A check another window is running right now isn't repeated. Returns at once.
        pub fn start_background(&mut self, p: &dyn Platform, now: f64) {
            if self.checking.is_some() || p.setting_json(keys::UPDATE_CHECK) == Some(Value::Bool(false)) {
                return;
            }
            let recent = |key: &str, within: f64| {
                // a time in the future (the clock was changed) counts as stale
                p.setting_json(key).and_then(|v| v.as_f64()).is_some_and(|t| t <= now && t > now - within)
            };
            if recent(keys::UPDATE_LAST, DAY_SECS) || recent(UPDATE_CLAIM, CLAIM_SECS) {
                if let Some(rel) = p.setting_json(keys::UPDATE_CACHE).as_ref().and_then(from_cache) {
                    self.offer(p, rel, false);
                }
                return;
            }
            p.set_setting_json(UPDATE_CLAIM, json!(now));
            self.claimed = true;
            self.spawn_check(false);
        }

        /// Help > Check for Updates…: asks now, whatever the settings say.
        pub fn check_now(&mut self, _p: &dyn Platform, _now: f64) {
            if let Some((_, manual)) = &mut self.checking {
                *manual = true;
                return;
            }
            self.spawn_check(true);
        }

        fn spawn_check(&mut self, manual: bool) {
            let pending = Pending::new();
            let out = pending.clone();
            let (api, current) = (self.api.clone(), self.current.clone());
            std::thread::Builder::new()
                .name("update-check".into())
                .spawn(move || out.set(update::newest_2x(&api, &current)))
                .ok();
            self.checking = Some((pending, manual));
        }

        /// A check or a download is running.
        pub fn is_busy(&self) -> bool {
            self.checking.is_some() || !self.orphans.is_empty() || matches!(self.banner, Some(Banner { phase: Phase::Downloading { .. }, .. }))
        }

        /// The busy note for the app's overlay: 1.x's while Check for Updates asks GitHub.
        pub fn busy_note(&self) -> Option<&'static str> {
            matches!(self.checking, Some((_, true))).then_some(CHECKING)
        }

        /// Collect what the background threads have finished (no drawing).
        pub fn poll(&mut self, p: &dyn Platform, now: f64) -> Option<UpdateAction> {
            if let Some((pending, manual)) = &self.checking {
                if let Some(result) = pending.take() {
                    let manual = *manual;
                    self.checking = None;
                    if std::mem::take(&mut self.claimed) {
                        p.set_setting_json(UPDATE_CLAIM, Value::Null);
                    }
                    match result {
                        Ok(found) => {
                            p.set_setting_json(keys::UPDATE_LAST, json!(now));
                            p.set_setting_json(keys::UPDATE_CACHE, to_cache(found.as_ref()));
                            let shown = found.is_some_and(|rel| self.offer(p, rel, manual));
                            if manual && !shown {
                                return Some(UpdateAction::Toast { text: latest_text(&self.current), warn: false });
                            }
                        }
                        // 1.x: a failed background check is quiet; Check for Updates says why
                        Err(text) => {
                            if manual {
                                return Some(UpdateAction::Toast { text, warn: true });
                            }
                        }
                    }
                }
            }
            let mut i = 0;
            while i < self.orphans.len() {
                match self.orphans[i].1.take() {
                    Some(result) => {
                        self.orphans.remove(i);
                        if let Ok(path) = result {
                            let _ = std::fs::remove_file(path);
                        }
                    }
                    None => i += 1,
                }
            }
            let result = match &self.banner {
                Some(Banner { phase: Phase::Downloading { pending }, .. }) => pending.take()?,
                _ => return None,
            };
            let others = result.is_ok() && self.others_open();
            let banner = self.banner.as_mut()?;
            let new = match result {
                Ok(new) => new,
                Err(e) => {
                    banner.phase = Phase::Failed(e);
                    return None;
                }
            };
            // another window opened while the download ran
            if others {
                let _ = std::fs::remove_file(&new);
                banner.phase = Phase::Offer { note: Some(OTHER_WINDOWS.to_string()) };
                return None;
            }
            let Some(running) = self.running_exe.clone() else {
                let _ = std::fs::remove_file(&new);
                banner.phase = Phase::Failed("This copy can't update itself.".into());
                return None;
            };
            match update::swap(&new, &running) {
                Ok(()) => {
                    banner.phase = Phase::Restarting;
                    Some(UpdateAction::Restart { exe: running })
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&new);
                    banner.phase = Phase::Failed(e);
                    None
                }
            }
        }

        /// Shows the banner for `rel` if it is newer and (unless `manual`) not skipped.
        fn offer(&mut self, p: &dyn Platform, rel: Release, manual: bool) -> bool {
            if !update::is_newer(&rel.version, &self.current) {
                return false;
            }
            let skipped = p.setting_json(keys::UPDATE_SKIP).and_then(|v| v.as_str().map(str::to_owned)) == Some(rel.version.clone());
            if skipped && !manual {
                return false;
            }
            // a download already under way keeps its banner
            if !matches!(self.banner, Some(Banner { phase: Phase::Downloading { .. } | Phase::Restarting, .. })) {
                self.banner = Some(Banner { rel, phase: Phase::Offer { note: None } });
            }
            true
        }

        pub fn banner(&self) -> Option<BannerView> {
            let b = self.banner.as_ref()?;
            let how = if self.running_exe.is_some() { INSTALL_LINE } else { DOWNLOAD_LINE };
            let (note, buttons) = match &b.phase {
                Phase::Offer { note } => {
                    let btns = if self.running_exe.is_some() {
                        vec![BannerButton::Skip, BannerButton::Later, BannerButton::WhatsNew, BannerButton::UpdateNow]
                    } else {
                        vec![BannerButton::Skip, BannerButton::Later, BannerButton::Download]
                    };
                    (note.clone(), btns)
                }
                Phase::Downloading { .. } => (Some(DOWNLOADING.to_string()), vec![BannerButton::Cancel]),
                Phase::Restarting => (Some(RESTARTING.to_string()), vec![]),
                Phase::Failed(e) => (Some(e.clone()), vec![BannerButton::Close, BannerButton::DownloadInstead]),
            };
            Some(BannerView {
                version: b.rel.version.clone(),
                title: available_title(&b.rel.version),
                line: format!("You have {}. {how}", self.current),
                note,
                buttons: buttons.into_iter().map(BannerButton::label).collect(),
            })
        }

        /// A banner button was pressed.
        pub fn press(&mut self, which: BannerButton, p: &dyn Platform) -> Option<UpdateAction> {
            let others = which == BannerButton::UpdateNow && self.others_open();
            let b = self.banner.as_mut()?;
            let page = release_page(&b.rel.tag);
            match which {
                BannerButton::WhatsNew => Some(UpdateAction::OpenUrl(page)),
                BannerButton::Download | BannerButton::DownloadInstead => {
                    self.banner = None;
                    Some(UpdateAction::OpenUrl(page))
                }
                BannerButton::Skip => {
                    p.set_setting_json(keys::UPDATE_SKIP, json!(b.rel.version));
                    self.banner = None;
                    None
                }
                BannerButton::Later | BannerButton::Close => {
                    if !matches!(b.phase, Phase::Downloading { .. } | Phase::Restarting) {
                        self.banner = None;
                    }
                    None
                }
                // 1.x: Cancel hides the banner at once; the download's file is removed when it ends
                BannerButton::Cancel => {
                    if let Some(Banner { phase: Phase::Downloading { pending }, rel }) = self.banner.take() {
                        self.orphans.push((rel.tag, pending));
                    }
                    None
                }
                BannerButton::UpdateNow => {
                    if !matches!(b.phase, Phase::Offer { .. } | Phase::Failed(_)) {
                        return None;
                    }
                    let Some(running) = self.running_exe.clone() else { return None };
                    if others {
                        b.phase = Phase::Offer { note: Some(OTHER_WINDOWS.to_string()) };
                        return None;
                    }
                    if let Some(i) = self.orphans.iter().position(|(tag, _)| *tag == b.rel.tag) {
                        let (_, pending) = self.orphans.remove(i);
                        b.phase = Phase::Downloading { pending };
                        return None;
                    }
                    let pending = Pending::new();
                    let out = pending.clone();
                    let rel = b.rel.clone();
                    let new = update::new_path(&running, &rel.tag);
                    std::thread::Builder::new()
                        .name("update-download".into())
                        .spawn(move || out.set(update::download(&rel, &new).map(|()| new)))
                        .ok();
                    b.phase = Phase::Downloading { pending };
                    None
                }
            }
        }

        /// Help > Update Settings…
        pub fn open_settings(&mut self, p: &dyn Platform) {
            self.settings = Some(p.setting_json(keys::UPDATE_CHECK) != Some(Value::Bool(false)));
        }

        /// The settings dialog's checkbox while it is open.
        pub fn settings_open(&self) -> Option<bool> {
            self.settings
        }

        /// OK in Update Settings: store the switch; on checks now (1.x), off says so.
        pub fn set_checks(&mut self, p: &dyn Platform, on: bool, now: f64) -> Option<UpdateAction> {
            self.settings = None;
            p.set_setting_json(keys::UPDATE_CHECK, json!(on));
            if on {
                self.check_now(p, now);
                None
            } else {
                Some(UpdateAction::Toast { text: CHECKS_OFF.into(), warn: false })
            }
        }

        /// Every frame: poll, then draw the banner and the settings dialog.
        pub fn show(&mut self, ctx: &egui::Context, tk: &Tokens, p: &dyn Platform, now: f64) -> Option<UpdateAction> {
            if self.is_busy() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if let Some(a) = self.poll(p, now) {
                return Some(a);
            }
            if let Some(on) = self.settings {
                if let Some(a) = self.draw_settings(ctx, tk, p, on, now) {
                    return Some(a);
                }
            }
            let view = self.banner()?;
            let pressed = draw_banner(ctx, tk, &view)?;
            let which = BUTTONS.into_iter().find(|b| b.label() == pressed)?;
            self.press(which, p)
        }

        fn draw_settings(&mut self, ctx: &egui::Context, tk: &Tokens, p: &dyn Platform, mut on: bool, now: f64) -> Option<UpdateAction> {
            let screen = ctx.content_rect();
            Area::new(Id::unique("update-settings-scrim")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
                ui.painter().rect_filled(r, 0.0, tk.scrim);
            });
            let mut ok = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
            let mut cancel = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
            Area::new(Id::unique("update-settings")).order(Order::Tooltip).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                Frame::new()
                    .fill(tk.panel)
                    .stroke(Stroke::new(1.0, tk.line))
                    .corner_radius(CornerRadius::same(9))
                    .shadow(tk.shadow_lg)
                    .inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 14 })
                    .show(ui, |ui| {
                        ui.set_max_width(400.0);
                        let bold = egui::FontId::new(15.0, egui::FontFamily::Name("bold".into()));
                        ui.label(RichText::new(SETTINGS_TITLE).font(bold).color(tk.strong));
                        ui.add_space(10.0);
                        ui.checkbox(&mut on, RichText::new(SETTINGS_CHECK).size(12.5).color(tk.text));
                        ui.add_space(6.0);
                        ui.label(RichText::new(SETTINGS_NOTE).size(11.5).color(tk.muted));
                        ui.add_space(14.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ok |= button(ui, tk, "OK", true).clicked();
                            cancel |= button(ui, tk, "Cancel", false).clicked();
                        });
                    });
            });
            if ok {
                return self.set_checks(p, on, now);
            }
            self.settings = if cancel { None } else { Some(on) };
            None
        }
    }

    /// 1.x's `#banner`: top right under the title bar, an accent bar on the left, max 380 px.
    /// Returns the label of the button pressed.
    fn draw_banner(ctx: &egui::Context, tk: &Tokens, v: &BannerView) -> Option<&'static str> {
        let mut pressed = None;
        Area::new(Id::unique("update-banner")).order(Order::Foreground).anchor(Align2::RIGHT_TOP, Vec2::new(-14.0, 38.0)).show(ctx, |ui| {
            let resp = Frame::new()
                .fill(tk.panel)
                .stroke(Stroke::new(1.0, tk.line))
                .corner_radius(CornerRadius::same(tk.radius as u8))
                .shadow(tk.shadow_lg)
                .inner_margin(Margin { left: 16, right: 12, top: 10, bottom: 10 })
                .show(ui, |ui| {
                    ui.set_max_width(380.0 - 28.0);
                    ui.label(RichText::new(&v.title).size(12.5).strong().color(tk.strong));
                    ui.add_space(2.0);
                    ui.label(RichText::new(&v.line).size(12.0).color(tk.muted));
                    if let Some(n) = &v.note {
                        ui.add_space(6.0);
                        ui.label(RichText::new(n).size(12.0).color(tk.text));
                    }
                    if !v.buttons.is_empty() {
                        ui.add_space(8.0);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for label in v.buttons.iter().rev() {
                                let primary = BUTTONS.iter().any(|b| b.primary() && b.label() == *label);
                                if button(ui, tk, label, primary).clicked() {
                                    pressed = Some(*label);
                                }
                            }
                        });
                    }
                });
            let r = resp.response.rect;
            let bar = egui::Rect::from_min_size(r.min, Vec2::new(4.0, r.height()));
            ui.painter().rect_filled(bar, CornerRadius { nw: tk.radius as u8, sw: tk.radius as u8, ne: 0, se: 0 }, tk.accent);
        });
        pressed
    }

    /// A lock file held for as long as a 2.0 window runs: `<windows dir>/<pid>-<n>.lock`. The
    /// operating system frees the lock when the process ends, even after a crash.
    pub struct WindowLock {
        file: Option<File>,
        path: PathBuf,
        name: String,
    }

    impl WindowLock {
        pub fn take(dir: &Path) -> std::io::Result<WindowLock> {
            static N: AtomicU32 = AtomicU32::new(0);
            std::fs::create_dir_all(dir)?;
            let name = format!("{}-{}.lock", std::process::id(), N.fetch_add(1, Ordering::Relaxed));
            let path = dir.join(&name);
            let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)?;
            file.lock()?;
            Ok(WindowLock { file: Some(file), path, name })
        }
    }

    impl Drop for WindowLock {
        fn drop(&mut self) {
            self.file = None;
            let _ = std::fs::remove_file(&self.path);
        }
    }

    /// Is another 2.0 window running? Each holds a [`WindowLock`] in `dir`; `own` (this window's
    /// file name) doesn't count. A lock that can be taken belongs to a window that has gone: its
    /// file is removed once it is a minute old (a younger one may be a window still starting).
    pub fn other_windows_open(dir: &Path, own: Option<&str>) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else { return false };
        let mut found = false;
        for e in entries.flatten() {
            let name = e.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.ends_with(".lock") || Some(name) == own {
                continue;
            }
            let Ok(f) = OpenOptions::new().read(true).write(true).open(e.path()) else { continue };
            match f.try_lock() {
                Err(TryLockError::WouldBlock) => found = true,
                Ok(()) => {
                    drop(f);
                    let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > Duration::from_secs(60));
                    if old {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
                Err(_) => {}
            }
        }
        found
    }

    /// Does `exe` run from the install folder (`xtask install`)? Only that copy updates itself;
    /// a build run from elsewhere (a `cargo` target folder) offers Download instead.
    pub fn is_installed(exe: &Path) -> bool {
        let dir = stl_platform::paths::install_dir();
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        exe.parent().is_some_and(|p| canon(p) == canon(&dir))
    }

    /// After the first frame: the new copy started, so the previous exe can go.
    pub fn finish_swap_after_first_frame() {
        if let Ok(exe) = std::env::current_exe() {
            update::finish_swap(&exe);
        }
    }

    /// Seconds since 1970, for `now`.
    pub fn now_secs() -> f64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
    }

    fn to_cache(rel: Option<&Release>) -> Value {
        match rel {
            Some(r) => json!({ "tag": r.tag, "version": r.version, "url": r.url, "size": r.size, "digest": r.digest, "notes": r.notes }),
            None => json!({ "version": null }),
        }
    }

    fn from_cache(v: &Value) -> Option<Release> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        Some(Release {
            tag: s("tag")?,
            version: s("version")?,
            url: s("url")?,
            size: v.get("size").and_then(Value::as_u64)?,
            digest: s("digest"),
            notes: s("notes").unwrap_or_default(),
        })
    }
}
