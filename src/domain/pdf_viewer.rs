use iced::widget::image;
use lru::LruCache;
use pdfium_render::prelude::*;
use std::num::NonZeroUsize;
use std::sync::OnceLock;

// One Pdfium binding for the whole program's lifetime.
static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

fn pdfium_instance() -> &'static Pdfium {
    PDFIUM.get_or_init(|| {
        let bindings =
            Pdfium::bind_to_system_library().expect("failed to bind to pdfium system library");
        Pdfium::new(bindings)
    })
}

pub struct PdfViewer {
    document: PdfDocument<'static>,
    pub cache: LruCache<usize, image::Handle>,
    pub current_page: usize,
}

impl PdfViewer {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, PdfiumError> {
        let pdfium = pdfium_instance(); // &'static Pdfium
        let document = pdfium.load_pdf_from_byte_vec(bytes, None)?;
        Ok(Self {
            document,
            cache: LruCache::new(NonZeroUsize::new(5).unwrap()),
            current_page: 0,
        })
    }

    pub fn page_count(&self) -> usize {
        self.document.pages().len() as usize
    }

    pub fn render_current_page(&mut self) -> Result<image::Handle, PdfiumError> {
        let page_number = self.current_page;
        if let Some(img) = self.cache.get(&page_number) {
            return Ok(img.clone());
        }
        let page = self.document.pages().get(page_number as i32)?;
        let bitmap = page.render_with_config(&PdfRenderConfig::new().set_target_width(1200))?;
        let dyn_image = bitmap.as_image()?;
        let rgba = dyn_image.to_rgba8();
        let handle = image::Handle::from_rgba(rgba.width(), rgba.height(), rgba.into_raw());
        self.cache.put(page_number, handle.clone());
        Ok(handle)
    }

    pub fn next_page(&mut self) {
        if self.current_page + 1 < self.page_count() {
            self.current_page += 1;
        }
    }

    pub fn previous_page(&mut self) {
        if self.current_page > 0 {
            self.current_page -= 1;
        }
    }

    /// Render an arbitrary page without touching current_page.
    pub fn render_page(&mut self, page_number: usize) -> Result<image::Handle, PdfiumError> {
        if let Some(img) = self.cache.get(&page_number) {
            return Ok(img.clone());
        }
        let page = self.document.pages().get(page_number as i32)?;
        let bitmap = page.render_with_config(&PdfRenderConfig::new().set_target_width(1200))?;
        let rgba = bitmap.as_image()?.to_rgba8();
        let handle = image::Handle::from_rgba(rgba.width(), rgba.height(), rgba.into_raw());
        self.cache.put(page_number, handle.clone());
        Ok(handle)
    }

    pub fn is_cached(&self, page_number: usize) -> bool {
        self.cache.contains(&page_number)
    }
}
