//! Media helpers the chat surfaces needed and every app would otherwise
//! re-state: the mime guess for an attachment, and PNG bytes to a gpui
//! `RenderImage` for an inline image.

use std::sync::Arc;

/// The mime type for a file name, by extension. Unknown extensions are plain
/// text, which is what an attachment the app cannot preview should claim.
pub fn mime_for_path(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "pdf" => "image/pdf",
        _ => "text/plain",
    }
}

/// Decode PNG bytes into the frame gpui draws. Returns `None` for bytes that
/// are not an image, so a corrupt attachment renders as its name rather than
/// panicking the transcript.
pub fn decode_png_render(png: &[u8]) -> Option<Arc<gpui::RenderImage>> {
    let decoded = image::load_from_memory(png).ok()?;
    let rgba = decoded.to_rgba8();
    Some(Arc::new(gpui::RenderImage::new(smallvec::smallvec![
        image::Frame::new(rgba)
    ])))
}
