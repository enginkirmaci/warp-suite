//! The embedded asset source: an app's rust-embed assets with gpui-kit's
//! bundled Lucide icon set as the fallback, so kit components (Input
//! decorations, popovers, …) resolve their IconName assets.

use gpui::AssetSource;
use rust_embed::RustEmbed;

/// The app's `#[derive(RustEmbed)] #[folder = "assets"]` struct, layered over
/// gpui-kit's bundled assets. Own assets win; the kit's are the fallback.
///
/// ```ignore
/// #[derive(RustEmbed)]
/// #[folder = "assets"]
/// struct Assets;
///
/// pub type EmbedAssets = warpui_kit_platform::assets::KitBackedAssets<Assets>;
/// ```
pub struct KitBackedAssets<E: RustEmbed + 'static> {
    kit: gpui_kit::assets::Assets,
    // fn-pointer phantom: keeps Send+Sync (AssetSource needs both) without
    // demanding them of E
    _local: std::marker::PhantomData<fn() -> E>,
}

impl<E: RustEmbed + 'static> KitBackedAssets<E> {
    pub fn new() -> Self {
        Self {
            kit: gpui_kit::assets::Assets,
            _local: std::marker::PhantomData,
        }
    }
}

impl<E: RustEmbed + 'static> Default for KitBackedAssets<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: RustEmbed + 'static> AssetSource for KitBackedAssets<E> {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match E::get(path) {
            Some(file) => Ok(Some(file.data)),
            None => self.kit.load(path),
        }
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<gpui::SharedString>> {
        let mut ours: Vec<gpui::SharedString> = E::iter()
            .filter(|p| p.starts_with(path))
            .map(gpui::SharedString::from)
            .collect();
        let mut kit = self.kit.list(path)?;
        ours.append(&mut kit);
        Ok(ours)
    }
}
