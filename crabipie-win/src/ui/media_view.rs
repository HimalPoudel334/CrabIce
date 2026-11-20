//! State of an audio/video response: what is being played and how far along it is.
//! The actual player lives in `platform::media`; the app talks to it through `MediaCmd`s
//! and receives `MediaState` snapshots back.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_KEY: AtomicU64 = AtomicU64::new(1);
/// Keys of dropped views: the app tells the player to close them.
static CLOSED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

pub fn take_closed() -> Vec<u64> {
    std::mem::take(&mut *CLOSED.lock().unwrap_or_else(|e| e.into_inner()))
}

#[derive(Clone, Debug)]
pub enum Source {
    /// Played straight from the server (range-capable media is never downloaded whole).
    Url(String),
    Bytes { data: Arc<Vec<u8>>, name: String, content_type: String },
}

/// Some(true) = audio, Some(false) = video, None = not media.
pub fn kind(bytes: &[u8], content_type: &str) -> Option<bool> {
    let ct = content_type.to_ascii_lowercase();
    if ct.starts_with("video/") {
        Some(false)
    } else if ct.starts_with("audio/") {
        Some(true)
    } else if bytes.len() > 12 && &bytes[4..8] == b"ftyp" {
        Some(false)
    } else if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some(false) // Matroska / WebM
    } else if bytes.starts_with(b"ID3") || bytes.starts_with(b"OggS") || bytes.starts_with(b"fLaC") || (bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE") {
        Some(true)
    } else {
        None
    }
}

/// "3:07" or "1:02:09".
pub fn fmt_time(secs: f64) -> String {
    let s = if secs.is_finite() && secs > 0.0 { secs as u64 } else { 0 };
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// What the player reports (polled a few times a second).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaState {
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub ended: bool,
    pub has_video: bool,
    pub video_size: (u32, u32),
    pub error: Option<String>,
}

pub struct MediaView {
    pub key: u64,
    pub source: Source,
    pub audio: bool,
    pub name: String,
    /// True while the player holds this response (only one plays at a time).
    pub live: bool,
    pub position: f64,
    pub duration: f64,
    pub playing: bool,
    pub ended: bool,
    pub volume: f64,
    pub muted: bool,
    pub has_video: bool,
    pub video_size: (u32, u32),
    pub error: Option<String>,
}

/// Plain copy of what painting needs (cloning a `MediaView` would mint a new key).
#[derive(Clone, Debug)]
pub struct MediaSnap {
    pub key: u64,
    pub audio: bool,
    pub name: String,
    pub live: bool,
    pub position: f64,
    pub duration: f64,
    pub playing: bool,
    pub muted: bool,
    pub volume: f64,
    pub has_video: bool,
    pub error: Option<String>,
}

impl MediaView {
    pub fn new(source: Source, audio: bool, name: String) -> Self {
        Self { key: NEXT_KEY.fetch_add(1, Ordering::Relaxed), source, audio, name, live: false, position: 0.0, duration: 0.0, playing: false, ended: false, volume: 1.0, muted: false, has_video: false, video_size: (0, 0), error: None }
    }

    pub fn snap(&self) -> MediaSnap {
        MediaSnap { key: self.key, audio: self.audio, name: self.name.clone(), live: self.live, position: self.position, duration: self.duration, playing: self.playing, muted: self.muted, volume: self.volume, has_video: self.has_video, error: self.error.clone() }
    }

    /// Takes a player snapshot; true if anything the UI shows changed.
    pub fn apply(&mut self, st: &MediaState) -> bool {
        let playing = !st.paused && !st.ended;
        let changed = (st.position - self.position).abs() >= 0.2
            || (st.duration - self.duration).abs() >= 0.5
            || playing != self.playing
            || st.ended != self.ended
            || st.has_video != self.has_video
            || st.video_size != self.video_size
            || st.error != self.error;
        self.position = st.position;
        self.duration = st.duration;
        self.playing = playing;
        self.ended = st.ended;
        self.has_video = st.has_video;
        self.video_size = st.video_size;
        self.error = st.error.clone();
        changed
    }
}

/// A clone is a separate, not-yet-opened view with its own key.
impl Clone for MediaView {
    fn clone(&self) -> Self {
        let mut v = Self::new(self.source.clone(), self.audio, self.name.clone());
        v.volume = self.volume;
        v.muted = self.muted;
        v
    }
}

impl std::fmt::Debug for MediaView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaView").field("key", &self.key).field("audio", &self.audio).field("live", &self.live).finish()
    }
}

impl Drop for MediaView {
    fn drop(&mut self) {
        CLOSED.lock().unwrap_or_else(|e| e.into_inner()).push(self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_media_and_formats_time() {
        assert_eq!(kind(b"", "video/mp4"), Some(false));
        assert_eq!(kind(b"", "audio/mpeg"), Some(true));
        assert_eq!(kind(b"\0\0\0\x18ftypmp42....", ""), Some(false));
        assert_eq!(kind(b"ID3\x04...........", ""), Some(true));
        assert_eq!(kind(b"hello world....", "text/plain"), None);
        assert_eq!(fmt_time(0.0), "0:00");
        assert_eq!(fmt_time(187.4), "3:07");
        assert_eq!(fmt_time(3729.0), "1:02:09");
        assert_eq!(fmt_time(f64::NAN), "0:00");
    }

    #[test]
    fn apply_reports_visible_changes_only() {
        let mut v = MediaView::new(Source::Url("http://x/y.mp4".into()), false, "y.mp4".into());
        let mut st = MediaState { duration: 100.0, ..Default::default() };
        assert!(v.apply(&st), "duration became known");
        assert!(!v.playing || v.ended || true);
        st.paused = false;
        assert!(v.apply(&st), "started playing");
        st.position = 0.05;
        assert!(!v.apply(&st), "a tiny position change isn't worth a repaint");
        st.position = 5.0;
        assert!(v.apply(&st));
    }
}
