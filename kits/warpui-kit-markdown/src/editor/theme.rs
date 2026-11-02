//! The editor's colors: nothing here is Obsidian's. Text, accent, link,
//! selection and caret come from the kit theme, which the app maps its accent
//! preset onto, so a preset swap recolors the editor on the next frame. The
//! fixed roles (faint ink, code wells, callout hues) are family palette tokens.

use super::display::{Ink, Tone};
use gpui::{App, Hsla};
use gpui_kit::component::ActiveTheme as _;
use warpui_kit_core::palette;

/// How strongly a highlight washes its text, Obsidian's 30% in the family gold.
const HIGHLIGHT_ALPHA: f32 = 0.3;
/// A tag's wash of the accent.
const TAG_ALPHA: f32 = 0.15;
/// A callout's wash of its hue.
const CALLOUT_ALPHA: f32 = 0.1;

/// A link to a note that does not exist yet.
const UNRESOLVED_ALPHA: f32 = 0.7;
const UNRESOLVED_LINE_ALPHA: f32 = 0.3;

/// The resolved colors of one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveColors {
    pub background: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    pub accent: Hsla,
    pub on_accent: Hsla,
    pub link: Hsla,
    pub selection: Hsla,
    pub caret: Hsla,
    pub border: Hsla,
    pub code_well: Hsla,
    pub code_pill: Hsla,
    pub highlight: Hsla,
    pub tag_wash: Hsla,
}

impl LiveColors {
    /// Reads the active theme: the accent family the app applied, plus the
    /// fixed palette.
    pub fn from_theme(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            background: theme.background,
            text: theme.foreground,
            muted: palette::text_muted().into(),
            faint: palette::text_muted().into(),
            accent: theme.primary,
            on_accent: theme.primary_foreground,
            link: theme.link,
            selection: theme.selection,
            caret: theme.caret,
            border: palette::hairline().into(),
            code_well: palette::well().into(),
            code_pill: palette::raised().into(),
            highlight: Hsla {
                a: HIGHLIGHT_ALPHA,
                ..palette::accent_ink().into()
            },
            tag_wash: Hsla {
                a: TAG_ALPHA,
                ..theme.primary
            },
        }
    }

    /// The color behind an ink role.
    pub fn ink(&self, ink: Ink) -> Hsla {
        match ink {
            Ink::Text => self.text,
            Ink::Muted => self.muted,
            Ink::Faint => self.faint,
            Ink::Accent => self.accent,
            Ink::Link => self.link,
            Ink::Unresolved => Hsla {
                a: UNRESOLVED_ALPHA,
                ..self.link
            },
            Ink::UnresolvedLine => Hsla {
                a: UNRESOLVED_LINE_ALPHA,
                ..self.accent
            },
            Ink::Clear => gpui::transparent_black(),
            Ink::Highlight => self.highlight,
            Ink::TagBackground => self.tag_wash,
            Ink::Tone(tone) => self.tone(tone),
            Ink::Rgba(key) => super::code::rgba_color(key),
        }
    }

    /// A callout family's hue.
    pub fn tone(&self, tone: Tone) -> Hsla {
        match tone {
            Tone::Blue => palette::accent_ink().into(),
            // callout titles are ink, so they take the accent's text shade
            Tone::Accent => self.link,
            Tone::Green => palette::SUCCESS_BRIGHT.into(),
            Tone::Amber => palette::AMBER_BRIGHT.into(),
            Tone::Orange => palette::WARN.into(),
            Tone::Red => palette::DANGER.into(),
            Tone::Purple => palette::PURPLE_BRIGHT.into(),
            Tone::Gray => self.muted,
        }
    }

    /// A callout box's fill.
    pub fn tone_wash(&self, tone: Tone) -> Hsla {
        Hsla {
            a: CALLOUT_ALPHA,
            ..self.tone(tone)
        }
    }
}
