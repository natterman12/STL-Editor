//! The debounced preview flow (1.x `schedulePreview` / `runPreview`, app.js:866).
//!
//! A change asks for a preview ([`PreviewState::schedule`]); the app checks the clock once a
//! frame ([`PreviewState::tick`]) and sends `Command::Preview` when the delay has passed. Only
//! the answer for the newest `seq` is shown ([`PreviewState::accepts`]).

use web_time::{Duration, Instant};

/// `schedulePreview(delay = 280)`.
pub const DELAY_MS: u64 = 280;
/// The "Building supports…" note appears when the answer takes longer than this (app.js:885).
pub const BUSY_MS: u64 = 150;

/// What the line under the form says, and in which colour (`toolMsg(m, kind)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgKind {
    Info,
    Warn,
    Err,
    /// With a spinner.
    Busy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolMsg {
    pub kind: MsgKind,
    pub text: String,
}

impl ToolMsg {
    pub fn new(kind: MsgKind, text: impl Into<String>) -> ToolMsg {
        ToolMsg { kind, text: text.into() }
    }
}

#[derive(Debug, Default)]
pub struct PreviewState {
    /// A preview was asked for: this many ms from the next tick (`_previewTimer`).
    requested: Option<u64>,
    /// When it runs.
    due: Option<Instant>,
    /// The `seq` of the newest preview this tool sent (or cleared): the only answer that may land
    /// (`_previewSeq`). The app's counter is shared by all tools, so an answer for a tool that was
    /// closed can never match the next one.
    pub seq: u64,
    /// When "Building…" shows if the answer hasn't come (`_busyTimer`), and for which `seq`.
    busy_due: Option<(Instant, u64)>,
    /// Waiting for the answer to `seq`.
    pub waiting: bool,
}

impl PreviewState {
    /// `schedulePreview(delay)`. Tools with `noLivePreview` never schedule.
    pub fn schedule(&mut self, delay_ms: u64, no_live_preview: bool) {
        self.requested = None;
        self.due = None;
        if no_live_preview {
            return;
        }
        self.requested = Some(delay_ms);
    }

    /// True when a preview is waiting for its delay.
    pub fn pending(&self) -> bool {
        self.requested.is_some() || self.due.is_some()
    }

    /// Drop a scheduled preview (the tool closed, or `runPreview` is about to run).
    pub fn cancel(&mut self) {
        self.requested = None;
        self.due = None;
        self.busy_due = None;
    }

    /// Once a frame: true when the preview should run now. `now` is the clock.
    pub fn tick(&mut self, now: Instant) -> bool {
        if let Some(ms) = self.requested.take() {
            self.due = Some(now + Duration::from_millis(ms));
        }
        match self.due {
            Some(d) if d <= now => {
                self.due = None;
                true
            }
            _ => false,
        }
    }

    /// How long until the next thing happens (to wake the UI), if anything is waiting.
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        let a = self.due.map(|d| d.saturating_duration_since(now));
        let b = self.busy_due.map(|(d, _)| d.saturating_duration_since(now));
        match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// `runPreview` takes a new number; a preview still on its way is out of date now.
    pub fn start(&mut self, seq: u64, now: Instant, busy: bool) {
        self.seq = seq;
        self.waiting = true;
        self.busy_due = busy.then(|| (now + Duration::from_millis(BUSY_MS), seq));
    }

    /// `runPreview` found the form incomplete: whatever is on its way must not land.
    pub fn invalidate(&mut self, seq: u64) {
        self.seq = seq;
        self.waiting = false;
        self.busy_due = None;
    }

    /// Does this answer belong to the newest preview?
    pub fn accepts(&self, seq: u64) -> bool {
        self.waiting && seq == self.seq
    }

    /// An answer landed.
    pub fn done(&mut self) {
        self.waiting = false;
        self.busy_due = None;
    }

    /// True once the "Building…" delay has passed for the preview still waiting.
    pub fn busy_shown(&mut self, now: Instant) -> bool {
        match self.busy_due {
            Some((d, s)) if d <= now && s == self.seq && self.waiting => {
                self.busy_due = None;
                true
            }
            _ => false,
        }
    }
}
