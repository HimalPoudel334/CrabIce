//! Direct2D + DirectWrite implementation of `ui::canvas::Canvas`.
//! Everything is drawn in DIPs: the render target's DPI is set to the
//! window's DPI, so the rest of the app never sees physical pixels.

use std::collections::HashMap;

use windows::core::{Result, HRESULT};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F,
    D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
    ID2D1StrokeStyle, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_DRAW_TEXT_OPTIONS_CLIP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use crate::ui::canvas::{Align, Canvas, TextStyle};
use crate::ui::geom::{Color, Rect};

// Not exposed by the bindings: EndDraw returns this when the device is lost
// and the render target (and everything made from it) must be rebuilt.
const D2DERR_RECREATE_TARGET: HRESULT = HRESULT(0x8899000Cu32 as i32);

fn d_rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: r.l, top: r.t, right: r.r, bottom: r.b }
}

fn d_color(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a }
}

pub struct Renderer {
    factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    hwnd: HWND,
    rt: Option<ID2D1HwndRenderTarget>,
    brush: Option<ID2D1SolidColorBrush>,
    formats: HashMap<(u32, bool, Align), IDWriteTextFormat>,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        unsafe {
            let factory =
                D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite = DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)?;
            Ok(Self {
                factory,
                dwrite,
                hwnd: HWND::default(),
                rt: None,
                brush: None,
                formats: HashMap::new(),
            })
        }
    }

    pub fn attach(&mut self, hwnd: HWND) {
        self.hwnd = hwnd;
    }

    fn create_target(&mut self) -> Result<()> {
        unsafe {
            let mut rc = RECT::default();
            GetClientRect(self.hwnd, &mut rc)?;
            let size = D2D_SIZE_U {
                width: (rc.right - rc.left).max(1) as u32,
                height: (rc.bottom - rc.top).max(1) as u32,
            };
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_UNKNOWN,
                    alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                },
                dpiX: 0.0,
                dpiY: 0.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd: self.hwnd,
                pixelSize: size,
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let rt = self.factory.CreateHwndRenderTarget(&props, &hwnd_props)?;
            let dpi = GetDpiForWindow(self.hwnd) as f32;
            rt.SetDpi(dpi, dpi);
            let black = D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
            let brush = rt.CreateSolidColorBrush(&black, None)?;
            self.rt = Some(rt);
            self.brush = Some(brush);
        }
        Ok(())
    }

    /// Returns false if the target couldn't be created (skip this frame).
    pub fn begin_draw(&mut self) -> bool {
        if self.rt.is_none() && self.create_target().is_err() {
            return false;
        }
        if let Some(rt) = &self.rt {
            unsafe { rt.BeginDraw() };
            true
        } else {
            false
        }
    }

    pub fn end_draw(&mut self) {
        let Some(rt) = &self.rt else { return };
        let res = unsafe { rt.EndDraw(None, None) };
        if let Err(e) = res {
            if e.code() == D2DERR_RECREATE_TARGET {
                // device lost: drop everything made from the old target
                self.rt = None;
                self.brush = None;
            }
        }
    }

    /// Size of the drawing area in DIPs.
    pub fn size(&self) -> (f32, f32) {
        match &self.rt {
            Some(rt) => {
                let s = unsafe { rt.GetSize() };
                (s.width, s.height)
            }
            None => (0.0, 0.0),
        }
    }

    /// `width`/`height` in physical pixels (as delivered by WM_SIZE).
    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(rt) = &self.rt {
            let size = D2D_SIZE_U { width: width.max(1), height: height.max(1) };
            if unsafe { rt.Resize(&size) }.is_err() {
                self.rt = None;
                self.brush = None;
            }
        }
    }

    pub fn update_dpi(&mut self) {
        if let Some(rt) = &self.rt {
            let dpi = unsafe { GetDpiForWindow(self.hwnd) } as f32;
            unsafe { rt.SetDpi(dpi, dpi) };
        }
    }

    pub fn clear(&self, c: Color) {
        if let Some(rt) = &self.rt {
            let col = d_color(c);
            unsafe { rt.Clear(Some(&col as *const D2D1_COLOR_F)) };
        }
    }

    fn format(&mut self, style: TextStyle) -> Option<IDWriteTextFormat> {
        let key = ((style.size * 10.0) as u32, style.bold, style.align);
        if let Some(f) = self.formats.get(&key) {
            return Some(f.clone());
        }
        let fmt = unsafe {
            let weight = if style.bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
            let fmt = self
                .dwrite
                .CreateTextFormat(
                    windows::core::w!("Segoe UI"),
                    None::<&IDWriteFontCollection>,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    style.size,
                    windows::core::w!(""),
                )
                .ok()?;
            let _ = fmt.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let _ = fmt.SetTextAlignment(match style.align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            if let Ok(sign) = self.dwrite.CreateEllipsisTrimmingSign(&fmt) {
                let trimming = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                };
                let _ = fmt.SetTrimming(&trimming, &sign);
            }
            fmt
        };
        self.formats.insert(key, fmt.clone());
        Some(fmt)
    }

    fn with_brush(&self, c: Color, f: impl FnOnce(&ID2D1HwndRenderTarget, &ID2D1SolidColorBrush)) {
        if let (Some(rt), Some(brush)) = (&self.rt, &self.brush) {
            let col = d_color(c);
            unsafe { brush.SetColor(&col) };
            f(rt, brush);
        }
    }
}

impl Canvas for Renderer {
    fn fill_rect(&mut self, r: Rect, c: Color) {
        self.with_brush(c, |rt, brush| unsafe {
            rt.FillRectangle(&d_rect(r), brush);
        });
    }

    fn stroke_rect(&mut self, r: Rect, c: Color, width: f32) {
        self.with_brush(c, |rt, brush| unsafe {
            rt.DrawRectangle(&d_rect(r), brush, width, None::<&ID2D1StrokeStyle>);
        });
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), c: Color, width: f32) {
        self.with_brush(c, |rt, brush| unsafe {
            rt.DrawLine(
                D2D_POINT_2F { x: a.0, y: a.1 },
                D2D_POINT_2F { x: b.0, y: b.1 },
                brush,
                width,
                None::<&ID2D1StrokeStyle>,
            );
        });
    }

    fn text(&mut self, s: &str, r: Rect, c: Color, style: TextStyle) {
        let Some(fmt) = self.format(style) else { return };
        let wide: Vec<u16> = s.encode_utf16().collect();
        let rect = d_rect(r);
        self.with_brush(c, |rt, brush| unsafe {
            rt.DrawText(
                &wide,
                &fmt,
                &rect,
                brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        });
    }

    fn push_clip(&mut self, r: Rect) {
        if let Some(rt) = &self.rt {
            unsafe { rt.PushAxisAlignedClip(&d_rect(r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
        }
    }

    fn pop_clip(&mut self) {
        if let Some(rt) = &self.rt {
            unsafe { rt.PopAxisAlignedClip() };
        }
    }
}
