//! Mermaid blocks as pictures. A block's source goes through Merman and resvg
//! off the UI thread and comes back as an image its view can show, kept by
//! source and look: a theme change redraws every diagram, while edits around
//! one never do. Everything here sits behind the `mermaid` feature; without it
//! no block ever leaves [`DiagramStatus::None`] and they stay code blocks.

use super::state::LiveEditorState;
use super::theme::LiveColors;
use crate::live::{LiveBlockKind, LiveDocument, LiveLineKind};
use gpui::{Context, RenderImage, SharedString, Window};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ops::Range;
use std::sync::Arc;

/// The fence language that makes a code block a diagram.
pub const MERMAID: &str = "mermaid";

/// What a Mermaid block shows right now.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(feature = "mermaid"), allow(dead_code))]
pub enum DiagramStatus {
    /// Not rendered (yet): it stays a code block.
    #[default]
    None,
    /// A picture at this logical size, `key` in the image lookup.
    Ready {
        key: String,
        width: f32,
        height: f32,
    },
    /// The source does not parse: a code block saying why.
    Failed(String),
}

/// Everything a rendered picture depends on besides its source.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub colors: LiveColors,
    pub em: f32,
    pub scale: f32,
    pub family: SharedString,
}

/// A finished render.
struct Finished {
    status: DiagramStatus,
    data: Option<Arc<RenderImage>>,
}

/// The note's diagrams.
#[derive(Default)]
pub struct Diagrams {
    /// What the store was last refreshed for.
    seen: Option<(u64, Look)>,
    /// Block index to its render.
    blocks: HashMap<usize, u64>,
    /// Renders by source and look; `None` while one is running.
    renders: HashMap<u64, Option<Finished>>,
    #[cfg(feature = "mermaid")]
    backend: Option<(Look, Arc<backend::Backend>)>,
}

impl Diagrams {
    /// How block `block` stands.
    pub fn status(&self, block: usize) -> DiagramStatus {
        self.blocks
            .get(&block)
            .and_then(|hash| self.renders.get(hash))
            .and_then(Option::as_ref)
            .map_or(DiagramStatus::None, |done| done.status.clone())
    }

    /// The pixels behind a key of [`DiagramStatus::Ready`].
    pub fn data(&self, key: &str) -> Option<Arc<RenderImage>> {
        self.renders
            .values()
            .flatten()
            .find(|done| matches!(&done.status, DiagramStatus::Ready { key: k, .. } if k == key))
            .and_then(|done| done.data.clone())
    }

    /// Starts rendering what `doc` shows and does not have yet, and forgets
    /// what it no longer shows.
    pub fn refresh(
        &mut self,
        revision: u64,
        text: &str,
        doc: &LiveDocument,
        look: &Look,
        window: &mut Window,
        cx: &mut Context<LiveEditorState>,
    ) {
        if !cfg!(feature = "mermaid") {
            return;
        }
        if self
            .seen
            .as_ref()
            .is_some_and(|(seen, seen_look)| *seen == revision && seen_look == look)
        {
            return;
        }
        self.seen = Some((revision, look.clone()));
        self.blocks.clear();
        let mut wanted = Vec::new();
        for (ix, source) in diagram_sources(doc) {
            let source = &text[source];
            let hash = render_key(source, look);
            self.blocks.insert(ix, hash);
            wanted.push(hash);
            if let Entry::Vacant(slot) = self.renders.entry(hash) {
                slot.insert(None);
                self.start(hash, source, look, window, cx);
            }
        }
        let stale: Vec<u64> = self
            .renders
            .keys()
            .filter(|hash| !wanted.contains(hash))
            .copied()
            .collect();
        for hash in stale {
            // a picture also lives in the window's atlas until told otherwise
            if let Some(Some(Finished {
                data: Some(data), ..
            })) = self.renders.remove(&hash)
            {
                cx.drop_image(data, Some(window));
            }
        }
    }

    #[cfg(not(feature = "mermaid"))]
    fn start(
        &mut self,
        _: u64,
        _: &str,
        _: &Look,
        _: &mut Window,
        _: &mut Context<LiveEditorState>,
    ) {
    }

    #[cfg(feature = "mermaid")]
    fn start(
        &mut self,
        hash: u64,
        source: &str,
        look: &Look,
        _window: &mut Window,
        cx: &mut Context<LiveEditorState>,
    ) {
        let reusable = self
            .backend
            .as_ref()
            .filter(|(built, _)| built == look)
            .map(|(_, backend)| backend.clone());
        let backend = reusable.unwrap_or_else(|| {
            let built = Arc::new(backend::Backend::new(look, cx));
            self.backend = Some((look.clone(), built.clone()));
            built
        });
        use gpui::AppContext as _;
        let source = source.to_string();
        let scale = look.scale;
        let task = cx.background_spawn(async move { backend.render(&source, scale) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| this.diagram_rendered(hash, result, cx))
                .ok();
        })
        .detach();
    }

    /// Takes in a finished render; `false` if nothing wants it anymore.
    #[cfg(feature = "mermaid")]
    pub fn finish(&mut self, hash: u64, result: Result<backend::Picture, String>) -> bool {
        let Some(slot) = self.renders.get_mut(&hash) else {
            return false;
        };
        *slot = Some(match result {
            Ok(picture) => Finished {
                status: DiagramStatus::Ready {
                    key: format!("mermaid:{hash:x}"),
                    width: picture.width,
                    height: picture.height,
                },
                data: Some(picture.data),
            },
            Err(message) => Finished {
                status: DiagramStatus::Failed(message),
                data: None,
            },
        });
        true
    }
}

/// The Mermaid blocks of `doc`: block index and the source between the fences.
pub fn diagram_sources(doc: &LiveDocument) -> Vec<(usize, Range<usize>)> {
    let mut found = Vec::new();
    for (ix, block) in doc.blocks.iter().enumerate() {
        let LiveBlockKind::Code { lang } = &block.kind else {
            continue;
        };
        if lang.as_deref() != Some(MERMAID) || block.lines.len() < 2 {
            continue;
        }
        let last = block.lines.end - 1;
        // an unclosed fence runs on to the end of the note
        let closed = doc.lines[last].kind == LiveLineKind::Fence;
        let end = if closed {
            doc.lines[last].range.start
        } else {
            doc.lines[last].range.end
        };
        found.push((ix, doc.lines[block.lines.start + 1].range.start..end));
    }
    found
}

fn render_key(source: &str, look: &Look) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{look:?}").hash(&mut hasher);
    hasher.finish()
}

#[cfg(feature = "mermaid")]
pub(super) mod backend {
    use super::{Arc, Look, RenderImage};
    use gpui::{App, Font, Hsla, Rgba, SvgRenderer, TextSystem, font, px};
    use merman::svg::{
        DeterministicTextMeasurer, MeasurementProfileId, SvgPipeline, SvgRenderOptions,
        TextMeasurementPolicy, TextMeasurementProfile, TextMeasurementProfileIdentity,
    };
    use merman::{
        Engine, MermaidConfig, OperationControl, RenderOutput, RenderRequest, Renderer,
        SvgEnvironment, SvgRequest,
    };
    use warpui_kit_core::palette;

    /// gpui rasterizes SVGs at this multiple of the requested scale.
    const SVG_OVERSAMPLE: f32 = gpui::SMOOTH_SVG_SCALE_FACTOR;
    /// The narrowest a flowchart node's label area gets.
    const MIN_NODE_WIDTH: f32 = 60.;
    /// A font's average advance, for a character it has no glyph for.
    const FALLBACK_ADVANCE_EM: f32 = 0.55;

    /// A rendered diagram.
    pub struct Picture {
        pub data: Arc<RenderImage>,
        /// Logical size.
        pub width: f32,
        pub height: f32,
    }

    /// One look's renderer, shared by the renders running off the UI thread.
    pub struct Backend {
        renderer: Renderer,
        request: SvgRequest,
        rasterizer: SvgRenderer,
    }

    impl Backend {
        pub fn new(look: &Look, cx: &App) -> Self {
            let config = MermaidConfig::from_value(site_config(look));
            let renderer = Renderer::new().with_engine(Engine::new().with_site_config(config));
            let environment = SvgEnvironment::deterministic().with_text_measurement_policy(
                measurement(cx.text_system().clone(), font(look.family.clone())),
            );
            let request = SvgRequest {
                environment,
                pipeline: Some(SvgPipeline::resvg_safe()),
                options: SvgRenderOptions {
                    diagram_id: Some("live-diagram".to_string()),
                    ..SvgRenderOptions::default()
                },
                ..SvgRequest::default()
            };
            Self {
                renderer,
                request,
                rasterizer: cx.svg_renderer(),
            }
        }

        /// Source to pixels; the error is one line for the reader.
        pub fn render(&self, source: &str, scale: f32) -> Result<Picture, String> {
            let request = RenderRequest::svg(source, OperationControl::new(), self.request.clone());
            let svg = match self.renderer.render(request) {
                Ok(RenderOutput::Svg(Some(svg))) => svg,
                Ok(_) => return Err("Not a diagram".to_string()),
                Err(err) => return Err(first_line(&err.to_string())),
            };
            let data = self
                .rasterizer
                .render_single_frame(svg.svg().as_bytes(), scale)
                .map_err(|err| first_line(&err.to_string()))?;
            let device = data.size(0);
            let per_px = scale * SVG_OVERSAMPLE;
            Ok(Picture {
                width: device.width.0 as f32 / per_px,
                height: device.height.0 as f32 / per_px,
                data,
            })
        }
    }

    fn first_line(message: &str) -> String {
        message.lines().next().unwrap_or(message).trim().to_string()
    }

    /// Measures labels in the font they will paint in, so nodes fit their text.
    fn measurement(system: Arc<TextSystem>, base: Font) -> TextMeasurementPolicy {
        let measurer =
            DeterministicTextMeasurer::default().with_width_callback(move |text, style| {
                let size = px(style.font_size as f32);
                let id = system.resolve_font(&base);
                let width: f32 = text
                    .chars()
                    .map(|c| {
                        system
                            .advance(id, size, c)
                            .map_or(size.as_f32() * FALLBACK_ADVANCE_EM, |advance| {
                                advance.width.as_f32()
                            })
                    })
                    .sum();
                f64::from(width)
            });
        let id = MeasurementProfileId::new("warpui-kit-markdown")
            .expect("the profile name is not empty");
        let identity = TextMeasurementProfileIdentity::new(id, env!("CARGO_PKG_VERSION"))
            .expect("the crate version is not empty");
        TextMeasurementPolicy::uniform(TextMeasurementProfile::new(identity, measurer))
    }

    fn css(color: impl Into<Hsla>) -> String {
        let rgba = Rgba::from(color.into());
        let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
        format!(
            "#{:02x}{:02x}{:02x}",
            channel(rgba.r),
            channel(rgba.g),
            channel(rgba.b)
        )
    }

    /// Mermaid's `base` theme filled from the editor's colors: it derives the
    /// rest, and what it would derive off-palette is set outright.
    fn site_config(look: &Look) -> serde_json::Value {
        let colors = &look.colors;
        let family = format!("\"{}\", sans-serif", look.family);
        let surface = css(colors.background);
        let raised = css(palette::CHIP);
        let well = css(palette::POPUP);
        let quiet = css(palette::INPUT);
        let line = css(colors.muted);
        let text = css(colors.text);
        let accent = css(colors.accent);
        // the accent first, then hues no accent preset sits next to
        let series = [
            css(colors.accent),
            css(palette::SUCCESS_BRIGHT),
            css(palette::AMBER_BRIGHT),
            css(palette::PURPLE_BRIGHT),
            css(palette::BLUE_BRIGHT),
            css(palette::WARN),
            css(palette::DANGER),
            css(palette::GOLD_BRIGHT),
        ];
        let fixed: [(&str, &str); 61] = [
            ("background", &surface),
            ("primaryColor", &raised),
            ("primaryBorderColor", &accent),
            ("primaryTextColor", &text),
            ("secondaryColor", &well),
            ("secondaryBorderColor", &line),
            ("secondaryTextColor", &text),
            ("tertiaryColor", &quiet),
            ("tertiaryBorderColor", &line),
            ("tertiaryTextColor", &text),
            ("lineColor", &line),
            ("textColor", &text),
            ("titleColor", &text),
            ("edgeLabelBackground", &surface),
            ("noteBkgColor", &well),
            ("noteBorderColor", &line),
            ("noteTextColor", &text),
            ("actorBkg", &raised),
            ("actorBorder", &accent),
            ("actorTextColor", &text),
            ("actorLineColor", &line),
            ("signalColor", &line),
            ("signalTextColor", &text),
            ("labelBoxBkgColor", &well),
            ("labelBoxBorderColor", &line),
            ("labelTextColor", &text),
            ("loopTextColor", &text),
            ("activationBkgColor", &quiet),
            ("activationBorderColor", &line),
            ("sequenceNumberColor", &surface),
            ("pieTitleTextColor", &text),
            ("pieLegendTextColor", &text),
            ("pieSectionTextColor", &text),
            ("pieStrokeColor", &surface),
            ("pieOuterStrokeColor", &line),
            ("sectionBkgColor", &well),
            ("altSectionBkgColor", &surface),
            ("sectionBkgColor2", &quiet),
            ("taskBkgColor", &accent),
            ("taskBorderColor", &accent),
            ("taskTextColor", &surface),
            ("taskTextDarkColor", &surface),
            ("taskTextLightColor", &text),
            ("taskTextOutsideColor", &text),
            ("activeTaskBkgColor", &series[4]),
            ("activeTaskBorderColor", &series[4]),
            ("doneTaskBkgColor", &quiet),
            ("doneTaskBorderColor", &line),
            ("critBkgColor", &series[6]),
            ("critBorderColor", &series[6]),
            ("gridColor", &line),
            ("todayLineColor", &accent),
            ("labelColor", &text),
            ("stateLabelColor", &text),
            ("transitionColor", &line),
            ("transitionLabelColor", &text),
            ("attributeBackgroundColorOdd", &well),
            ("attributeBackgroundColorEven", &raised),
            ("relationColor", &line),
            ("relationLabelBackground", &surface),
            ("relationLabelColor", &text),
        ];
        let mut variables = serde_json::Map::new();
        for (name, color) in fixed {
            variables.insert(name.to_string(), color.into());
        }
        for (ix, color) in series.iter().cycle().take(12).enumerate() {
            variables.insert(format!("pie{}", ix + 1), color.clone().into());
        }
        variables.insert("darkMode".to_string(), true.into());
        variables.insert("fontFamily".to_string(), family.clone().into());
        variables.insert("fontSize".to_string(), format!("{}px", look.em).into());
        serde_json::json!({
            "theme": "base",
            "fontFamily": family,
            "themeVariables": variables,
            // Mermaid 12 pads every node out to 120px; Obsidian's nodes hug
            // their labels far closer than that
            "flowchart": { "minNodeWidth": MIN_NODE_WIDTH },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::parse_document;

    #[test]
    fn only_mermaid_fences_are_diagrams() {
        let text = "```mermaid\ngraph TD\nA-->B\n```\n\n```rust\nfn a() {}\n```\n";
        let doc = parse_document(text);
        let found = diagram_sources(&doc);
        assert_eq!(found.len(), 1);
        assert_eq!(&text[found[0].1.clone()], "graph TD\nA-->B\n");
    }

    #[test]
    fn an_unclosed_fence_is_a_diagram_to_the_end_of_the_note() {
        let text = "```mermaid\ngraph TD\nA-->B";
        let doc = parse_document(text);
        let found = diagram_sources(&doc);
        assert_eq!(&text[found[0].1.clone()], "graph TD\nA-->B");
    }

    #[test]
    fn a_lone_fence_has_nothing_to_render() {
        assert!(diagram_sources(&parse_document("```mermaid")).is_empty());
    }
}
