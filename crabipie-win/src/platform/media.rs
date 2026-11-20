//! Audio/video playback with the Media Foundation media engine (system codecs: nothing is
//! shipped with the app). Video renders into a child window that the app positions over the
//! video area; audio needs no window. State is polled (`state`), so the engine's event
//! callback can be a no-op.

use crate::ui::media_view::{MediaState, Source};
use std::cell::Cell;
use std::sync::Once;
use windows::core::{implement, Interface, BSTR, HSTRING, IUnknown};
use windows::Win32::Foundation::{BOOL, E_POINTER, HINSTANCE, HWND, RECT};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaEngineClassFactory, IMFMediaEngineEx, IMFMediaEngineNotify, IMFMediaEngineNotify_Impl, MFCreateAttributes, MFCreateMFByteStreamOnStream, MFStartup,
    CLSID_MFMediaEngineClassFactory, MFSTARTUP_FULL, MF_BYTESTREAM_CONTENT_TYPE, MF_MEDIA_ENGINE_AUDIOONLY, MF_MEDIA_ENGINE_CALLBACK, MF_MEDIA_ENGINE_PLAYBACK_HWND, MF_VERSION,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WINDOW_EX_STYLE, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
};

/// The engine insists on a callback object; state is polled, so it does nothing.
#[implement(IMFMediaEngineNotify)]
struct Notify;

impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, _event: u32, _param1: usize, _param2: u32) -> windows::core::Result<()> {
        Ok(())
    }
}

static MF_START: Once = Once::new();

pub struct Player {
    pub key: u64,
    engine: IMFMediaEngineEx,
    view: Option<HWND>,
    size: Cell<(i32, i32)>,
    _notify: IMFMediaEngineNotify,
}

impl Player {
    pub fn open(parent: HWND, key: u64, source: &Source, audio_only: bool) -> Result<Player, String> {
        unsafe { Self::open_inner(parent, key, source, audio_only) }.map_err(|e| {
            let m = e.message();
            if m.trim().is_empty() {
                "The media player could not be started".to_string()
            } else {
                m.trim().to_string()
            }
        })
    }

    unsafe fn open_inner(parent: HWND, key: u64, source: &Source, audio_only: bool) -> windows::core::Result<Player> {
        MF_START.call_once(|| {
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        });
        let view = if audio_only {
            None
        } else {
            let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();
            Some(CreateWindowExW(WINDOW_EX_STYLE::default(), windows::core::w!("STATIC"), None, WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN, 0, 0, 16, 16, parent, None, hinstance, None)?)
        };
        let notify: IMFMediaEngineNotify = Notify.into();
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2)?;
        let attrs = attrs.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
        attrs.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify.cast::<IUnknown>()?)?;
        if let Some(h) = view {
            attrs.SetUINT64(&MF_MEDIA_ENGINE_PLAYBACK_HWND, h.0 as usize as u64)?;
        }
        let factory: IMFMediaEngineClassFactory = CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)?;
        let flags = if audio_only { MF_MEDIA_ENGINE_AUDIOONLY.0 as u32 } else { 0 };
        let engine: IMFMediaEngineEx = factory.CreateInstance(flags, &attrs)?.cast()?;
        match source {
            Source::Url(url) => engine.SetSource(&BSTR::from(url.as_str()))?,
            Source::Bytes { data, name, content_type } => {
                let stream = SHCreateMemStream(Some(data.as_slice())).ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                let bytes = MFCreateMFByteStreamOnStream(&stream)?;
                if !content_type.is_empty() {
                    let ct = HSTRING::from(content_type.as_str());
                    bytes.cast::<IMFAttributes>()?.SetString(&MF_BYTESTREAM_CONTENT_TYPE, &ct)?;
                }
                let url = format!("http://crabipie.local/{}", if name.is_empty() { "media" } else { name.as_str() });
                engine.SetSourceFromByteStream(&bytes, &BSTR::from(url))?;
            }
        }
        Ok(Player { key, engine, view, size: Cell::new((0, 0)), _notify: notify })
    }

    pub fn play(&self) {
        let _ = unsafe { self.engine.Play() };
    }

    pub fn pause(&self) {
        let _ = unsafe { self.engine.Pause() };
    }

    pub fn seek(&self, secs: f64) {
        let _ = unsafe { self.engine.SetCurrentTime(secs) };
    }

    pub fn set_volume(&self, v: f64) {
        let _ = unsafe { self.engine.SetVolume(v.clamp(0.0, 1.0)) };
    }

    pub fn set_muted(&self, m: bool) {
        let _ = unsafe { self.engine.SetMuted(BOOL::from(m)) };
    }

    /// Shows the video window at this pixel rect (relative to the main window), or hides it.
    pub fn place(&self, visible: bool, x: i32, y: i32, w: i32, h: i32) {
        let Some(hwnd) = self.view else { return };
        unsafe {
            if visible && w > 0 && h > 0 {
                let _ = SetWindowPos(hwnd, HWND_TOP, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                if self.size.get() != (w, h) {
                    self.size.set((w, h));
                    let dst = RECT { left: 0, top: 0, right: w, bottom: h };
                    let _ = self.engine.UpdateVideoStream(None, Some(&dst as *const RECT), None);
                }
            } else {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    pub fn state(&self) -> MediaState {
        unsafe {
            let e = &self.engine;
            let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
            let (mut cx, mut cy) = (0u32, 0u32);
            let _ = e.GetNativeVideoSize(Some(&mut cx as *mut u32), Some(&mut cy as *mut u32));
            let error = e.GetError().ok().and_then(|err| match err.GetErrorCode() {
                0 => None,
                1 => Some("Playback was aborted".to_string()),
                2 => Some("A network error stopped playback".to_string()),
                3 => Some("This media could not be decoded".to_string()),
                4 => Some("Windows has no decoder for this format (try MP4/H.264, or install the codec)".to_string()),
                5 => Some("This media is protected (DRM)".to_string()),
                n => Some(format!("Playback error {n}")),
            });
            MediaState { position: finite(e.GetCurrentTime()), duration: finite(e.GetDuration()), paused: e.IsPaused().as_bool(), ended: e.IsEnded().as_bool(), has_video: e.HasVideo().as_bool(), video_size: (cx, cy), error }
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            let _ = self.engine.Shutdown();
            if let Some(h) = self.view {
                let _ = DestroyWindow(h);
            }
        }
    }
}
