//! The pictures a note shows. The store walks the document before each layout
//! and loads what it names, so a view (which must stay pure) can ask how any
//! source stands, and paint can fetch the pixels by key. Files come from
//! gpui's asset cache.

use crate::live::{InlineObjectKind, LiveDocument};
use gpui::{App, ImgResourceLoader, RenderImage, Resource, Window};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// How an image stands right now, for a view to decide what the line shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageStatus {
    /// Loaded, at this natural size in logical px.
    Ready {
        width: f32,
        height: f32,
    },
    Loading,
    /// The file is gone or undecodable: the line keeps its source.
    Missing,
}

#[derive(Clone)]
enum Entry {
    Ready(Arc<RenderImage>),
    Loading,
    Missing,
}

impl Entry {
    fn status(&self) -> ImageStatus {
        match self {
            Self::Ready(data) => {
                let natural = data.size(0);
                ImageStatus::Ready {
                    width: natural.width.0 as f32,
                    height: natural.height.0 as f32,
                }
            }
            Self::Loading => ImageStatus::Loading,
            Self::Missing => ImageStatus::Missing,
        }
    }
}

/// The note's images, by source.
#[derive(Default)]
pub struct ImageStore {
    /// Resolved paths, so a layout never touches the disk for a source it has
    /// already seen.
    paths: HashMap<String, Option<PathBuf>>,
    entries: HashMap<String, Entry>,
}

impl ImageStore {
    /// Forgets the files: their base folder changed.
    pub fn clear(&mut self) {
        self.paths.clear();
        self.entries.clear();
    }

    /// An image is still on its way: refresh again on the next layout.
    pub fn pending(&self) -> bool {
        self.entries
            .values()
            .any(|entry| matches!(entry, Entry::Loading))
    }

    /// How `src` stands. A source the last [`refresh`](Self::refresh) has not
    /// seen counts as missing.
    pub fn status(&self, src: &str) -> ImageStatus {
        self.entries
            .get(src)
            .map_or(ImageStatus::Missing, Entry::status)
    }

    /// The pixels behind a key of a view.
    pub fn data(&self, key: &str) -> Option<Arc<RenderImage>> {
        match self.entries.get(key) {
            Some(Entry::Ready(data)) => Some(data.clone()),
            _ => None,
        }
    }

    /// Loads everything `doc` names and drops what it no longer does. Returns
    /// whether a known source changed status, which makes the views stale.
    pub fn refresh(
        &mut self,
        doc: &LiveDocument,
        base_dir: Option<&Path>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let mut entries = HashMap::new();
        let mut changed = false;
        let objects = doc.lines.iter().flat_map(|line| &line.objects);
        for object in objects {
            let InlineObjectKind::Image { src, .. } = &object.kind else {
                continue;
            };
            if entries.contains_key(src) {
                continue;
            }
            let entry = self.load(src, base_dir, window, cx);
            changed |= self
                .entries
                .get(src)
                .is_some_and(|known| known.status() != entry.status());
            entries.insert(src.clone(), entry);
        }
        self.entries = entries;
        changed
    }

    fn load(
        &mut self,
        src: &str,
        base_dir: Option<&Path>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entry {
        let resource: Resource = if src.starts_with("http://") || src.starts_with("https://") {
            Resource::Uri(src.to_string().into())
        } else {
            match self.resolve(src, base_dir) {
                Some(path) => Resource::Path(Arc::from(path)),
                None => return Entry::Missing,
            }
        };
        match window.use_asset::<ImgResourceLoader>(&resource, cx) {
            Some(Ok(data)) => Entry::Ready(data),
            Some(Err(_)) => Entry::Missing,
            None => Entry::Loading,
        }
    }

    /// `src` next to the note, or in its `assets` folder, the way embeds name
    /// an attachment by file alone. Percent escapes are spaces and friends.
    fn resolve(&mut self, src: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
        if let Some(found) = self.paths.get(src) {
            return found.clone();
        }
        let decoded = percent_decode(src);
        let plain = Path::new(&decoded);
        let candidates: Vec<PathBuf> = if plain.is_absolute() {
            vec![plain.to_path_buf()]
        } else {
            match base_dir {
                Some(base) => vec![base.join(plain), base.join("assets").join(plain)],
                None => Vec::new(),
            }
        };
        let found = candidates.into_iter().find(|path| path.is_file());
        self.paths.insert(src.to_string(), found.clone());
        found
    }
}

fn percent_decode(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut ix = 0;
    while ix < bytes.len() {
        let escape = (bytes[ix] == b'%')
            .then(|| src.get(ix + 1..ix + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escape {
            Some(byte) => {
                out.push(byte);
                ix += 3;
            }
            None => {
                out.push(bytes[ix]);
                ix += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

/// An image's size before any fitting: a `|width` or `|WxH` request wins over
/// the natural size.
pub fn requested_size(
    natural_w: f32,
    natural_h: f32,
    want_w: Option<u32>,
    want_h: Option<u32>,
) -> (f32, f32) {
    let aspect = if natural_w > 0. {
        natural_h / natural_w
    } else {
        1.
    };
    match (want_w, want_h) {
        (Some(w), Some(h)) => (w as f32, h as f32),
        (Some(w), None) => (w as f32, w as f32 * aspect),
        _ => (natural_w, natural_h),
    }
}

/// A size scaled down, keeping its aspect, to at most `room` wide.
pub fn fit_width(width: f32, height: f32, room: f32) -> (f32, f32) {
    if width > room && room > 0. {
        (room, height * room / width)
    } else {
        (width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_requests_win_over_the_natural_size() {
        assert_eq!(requested_size(240., 120., None, None), (240., 120.));
        assert_eq!(requested_size(240., 120., Some(60), None), (60., 30.));
        assert_eq!(requested_size(240., 120., Some(60), Some(10)), (60., 10.));
    }

    #[test]
    fn fitting_keeps_the_aspect() {
        assert_eq!(fit_width(240., 120., 500.), (240., 120.));
        assert_eq!(fit_width(1000., 500., 500.), (500., 250.));
    }

    #[test]
    fn percent_escapes_decode_to_the_file_name() {
        assert_eq!(percent_decode("my%20file.png"), "my file.png");
        assert_eq!(percent_decode("100%.png"), "100%.png");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
