//! Image decoding (Windows Imaging Component) and drawing (Direct2D bitmaps).
//! Decoded pixels are kept in memory so the GPU bitmap can be rebuilt after a device loss.

use super::{d_rect, Renderer};
use crate::ui::canvas::ImageInfo;
use crate::ui::geom::Rect;
use windows::core::Interface;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT, D2D_SIZE_U};
use windows::Win32::Graphics::Direct2D::{
    ID2D1Bitmap, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR, D2D1_BITMAP_PROPERTIES,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, IWICPalette, WICBitmapDitherTypeNone,
    WICBitmapInterpolationModeHighQualityCubic, WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{CoCreateInstance, IStream, CLSCTX_INPROC_SERVER};

/// Longest side kept when decoding: bigger images are scaled down (GPU bitmaps have a size limit).
const MAX_SIDE: u32 = 4096;

pub struct ImgEntry {
    info: ImageInfo,
    /// 32-bit premultiplied BGRA, `info.w * 4` bytes per row.
    pixels: Vec<u8>,
    bitmap: Option<ID2D1Bitmap>,
}

pub struct Wic(IWICImagingFactory);

impl Renderer {
    pub(super) fn image_load(&mut self, key: u64, bytes: &[u8]) -> Option<ImageInfo> {
        if let Some(e) = self.images.get(&key) {
            return Some(e.info);
        }
        let (info, pixels) = unsafe { self.decode(bytes) }.ok()?;
        self.images.insert(key, ImgEntry { info, pixels, bitmap: None });
        Some(info)
    }

    unsafe fn decode(&mut self, bytes: &[u8]) -> windows::core::Result<(ImageInfo, Vec<u8>)> {
        if self.wic.is_none() {
            self.wic = Some(Wic(CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?));
        }
        let f = &self.wic.as_ref().unwrap().0;
        let stream = f.CreateStream()?;
        stream.InitializeFromMemory(bytes)?;
        let decoder = f.CreateDecoderFromStream(&stream.cast::<IStream>()?, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?;
        let frame = decoder.GetFrame(0)?; // animated GIFs show their first frame
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h)?;
        let (tw, th) = if w.max(h) > MAX_SIDE {
            let k = MAX_SIDE as f64 / w.max(h) as f64;
            (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1))
        } else {
            (w.max(1), h.max(1))
        };
        let conv = f.CreateFormatConverter()?;
        if (tw, th) != (w, h) {
            let scaler = f.CreateBitmapScaler()?;
            scaler.Initialize(&frame, tw, th, WICBitmapInterpolationModeHighQualityCubic)?;
            conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None::<&IWICPalette>, 0.0, WICBitmapPaletteTypeMedianCut)?;
        } else {
            conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None::<&IWICPalette>, 0.0, WICBitmapPaletteTypeMedianCut)?;
        }
        let stride = tw * 4;
        let mut pixels = vec![0u8; (stride * th) as usize];
        conv.CopyPixels(std::ptr::null(), stride, &mut pixels)?;
        Ok((ImageInfo { w: tw, h: th, full_w: w, full_h: h }, pixels))
    }

    pub(super) fn image_draw(&mut self, key: u64, dst: Rect) {
        let Some(rt) = self.rt.clone() else { return };
        let Some(e) = self.images.get_mut(&key) else { return };
        if e.bitmap.is_none() {
            let props = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
            };
            let size = D2D_SIZE_U { width: e.info.w, height: e.info.h };
            e.bitmap = unsafe { rt.CreateBitmap(size, Some(e.pixels.as_ptr() as *const core::ffi::c_void), e.info.w * 4, &props) }.ok();
        }
        let Some(bitmap) = &e.bitmap else { return };
        // pixel art zoomed in far stays crisp; everything else is smoothed
        let mode = if dst.width() / e.info.w as f32 >= 3.0 { D2D1_BITMAP_INTERPOLATION_MODE_NEAREST_NEIGHBOR } else { D2D1_BITMAP_INTERPOLATION_MODE_LINEAR };
        let r = d_rect(dst);
        unsafe { rt.DrawBitmap(bitmap, Some(&r as *const _), 1.0, mode, None) };
    }

    /// GPU bitmaps belong to one render target: drop them when it is recreated.
    pub(super) fn image_drop_gpu(&mut self) {
        for e in self.images.values_mut() {
            e.bitmap = None;
        }
    }
}
