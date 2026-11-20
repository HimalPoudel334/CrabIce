//! PDF page rendering with the Windows.Data.Pdf component that ships with Windows 10+:
//! a page is rendered to PNG bytes, which the image viewer then shows.
//! The calls block until the async operations finish: run `render_page` on a worker thread.

use windows::core::Interface;
use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
use windows::Storage::Streams::{DataReader, DataWriter, IRandomAccessStream, InMemoryRandomAccessStream};
use windows::UI::Color;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

pub struct PdfPageImage {
    pub page_count: u32,
    pub png: Vec<u8>,
}

/// Renders `page` (0-based, clamped to the last page) of the PDF `bytes` at `width` pixels wide.
pub fn render_page(bytes: &[u8], page: u32, width: u32) -> Result<PdfPageImage, String> {
    // pool threads have no COM apartment yet; a repeat call just returns S_FALSE
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    render(bytes, page, width).map_err(|e| {
        let msg = e.message();
        if msg.trim().is_empty() {
            "PDF rendering failed".to_string()
        } else {
            msg.trim().to_string()
        }
    })
}

fn render(bytes: &[u8], page: u32, width: u32) -> windows::core::Result<PdfPageImage> {
    let stream = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&stream.GetOutputStreamAt(0)?)?;
    writer.WriteBytes(bytes)?;
    writer.StoreAsync()?.get()?;
    writer.FlushAsync()?.get()?;
    writer.DetachStream()?; // keep the stream itself open
    let source: IRandomAccessStream = stream.cast()?;
    source.Seek(0)?;

    let doc = PdfDocument::LoadFromStreamAsync(&source)?.get()?;
    let count = doc.PageCount()?;
    let page = doc.GetPage(page.min(count.saturating_sub(1)))?;
    let options = PdfPageRenderOptions::new()?;
    options.SetDestinationWidth(width.clamp(200, 4000))?;
    options.SetBackgroundColor(Color { A: 255, R: 255, G: 255, B: 255 })?; // paper white, not transparent

    let out = InMemoryRandomAccessStream::new()?;
    let out_stream: IRandomAccessStream = out.cast()?;
    page.RenderWithOptionsToStreamAsync(&out_stream, &options)?.get()?;
    let len = out.Size()? as u32;
    let reader = DataReader::CreateDataReader(&out.GetInputStreamAt(0)?)?;
    reader.LoadAsync(len)?.get()?;
    let mut png = vec![0u8; len as usize];
    reader.ReadBytes(&mut png)?;
    Ok(PdfPageImage { page_count: count, png })
}
