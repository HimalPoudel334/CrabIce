//! State of a PDF response: which page is shown and whether a render is pending.
//! Pages are rendered on a worker thread (platform::pdf) and shown by the image viewer.

use super::image_view::Edge;
use std::sync::Arc;

/// Width in pixels pages are rendered at (the viewer scales them to fit).
pub const RENDER_WIDTH: u32 = 2000;

pub fn is_pdf(bytes: &[u8], content_type: &str) -> bool {
    bytes.starts_with(b"%PDF-") || content_type.starts_with("application/pdf")
}

#[derive(Clone, Debug)]
pub struct PdfView {
    pub bytes: Arc<Vec<u8>>,
    /// 0-based page being shown (or being rendered).
    pub page: u32,
    pub pages: Option<u32>,
    /// Id of the render request whose result is wanted; older results are dropped.
    pub req_id: u64,
    pub loading: bool,
    pub error: Option<String>,
    /// Which end of the next page to show (scrolling back up lands at the bottom).
    pub land: Edge,
}

impl PdfView {
    pub fn new(bytes: Arc<Vec<u8>>) -> Self {
        Self { bytes, page: 0, pages: None, req_id: 0, loading: false, error: None, land: Edge::Top }
    }

    pub fn can_prev(&self) -> bool {
        self.page > 0
    }

    pub fn can_next(&self) -> bool {
        self.pages.map_or(false, |n| self.page + 1 < n)
    }

    /// "Page 2 / 9" (or just "Page 1" until the count is known).
    pub fn label(&self) -> String {
        match self.pages {
            Some(n) => format!("Page {} / {}", self.page + 1, n),
            None => format!("Page {}", self.page + 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pdfs_and_bounds_page_steps() {
        assert!(is_pdf(b"%PDF-1.7 ...", ""));
        assert!(is_pdf(b"", "application/pdf"));
        assert!(!is_pdf(b"hello", "text/plain"));
        let mut v = PdfView::new(Arc::new(vec![]));
        assert!(!v.can_prev() && !v.can_next(), "no next until the page count is known");
        v.pages = Some(3);
        assert!(v.can_next() && !v.can_prev());
        v.page = 2;
        assert!(!v.can_next() && v.can_prev());
        assert_eq!(v.label(), "Page 3 / 3");
    }
}
