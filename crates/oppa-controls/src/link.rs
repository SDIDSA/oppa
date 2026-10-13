use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// Link
// ---------------------------------------------------------------------------

/// Underline behavior for [`Link`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LinkUnderline {
    /// Underline only while hovered (default — the classic link:
    /// quiet text that affords on approach).
    #[default]
    Hover,
    /// Always underlined.
    Always,
    /// Never underlined.
    Never,
}

/// Text-link props: an inline pressable label that sizes to its own
/// content (never full-width — the underline hugs the text, not the
/// row), with `button` semantics (no dedicated link role exists in
/// the contract; press-to-activate is what it announces).
///
/// Ink rides the theme (`primary`, `text_secondary` when disabled)
/// unless [`ink`](Self::ink) overrides it.
#[derive(Clone)]
pub struct LinkProps {
    pub label: SharedString,
    pub enabled: bool,
    pub on_press: Option<Action>,
    pub underline: LinkUnderline,
    pub style: TextClass,
    /// Ink override (default `None` — theme primary; authors may pin
    /// an exact brand blue).
    pub ink: Option<Color>,
    /// Retained debug label (default `"link"`).
    pub debug: SharedString,
}

impl Props for LinkProps {}

impl LinkProps {
    pub fn new(label: &str, on_press: impl Fn() + 'static) -> Self {
        Self {
            label: SharedString::from(label),
            enabled: true,
            on_press: Some(action(on_press)),
            underline: LinkUnderline::Hover,
            style: Text::body_secondary,
            ink: None,
            debug: SharedString::from("link"),
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn underline(mut self, underline: LinkUnderline) -> Self {
        self.underline = underline;
        self
    }

    pub fn style(mut self, style: TextClass) -> Self {
        self.style = style;
        self
    }

    /// Ink override (see [`ink`](Self::ink)).
    pub fn ink(mut self, ink: Color) -> Self {
        self.ink = Some(ink);
        self
    }

    pub fn debug(mut self, debug: &str) -> Self {
        self.debug = SharedString::from(debug);
        self
    }
}

/// Text link: content-sized label in primary ink with `button`
/// semantics; press activates (no-op when disabled or handlerless —
/// structurally handler-less either way, decision 213). Hover paints
/// a 1px inset bottom band as the underline (paint-only, never
/// layout — appearing never moves siblings); enabled links show the
/// hand, everything else the platform arrow. Fill and press behavior
/// ride the app theme (Round 11.2).
pub fn Link(ctx: &Ctx, props: &LinkProps) -> VNode {
    let t = ctx.theme().tokens();
    let semantics = Semantics::button()
        .label(&props.label)
        .disabled(!props.enabled);
    let active = props.enabled && props.on_press.is_some();
    let press = if active { props.on_press.clone() } else { None };
    let ink = props.ink.unwrap_or(if props.enabled {
        t.primary
    } else {
        t.text_secondary
    });
    // Per-instance hover (each link underlines alone; disabled and
    // handlerless links never underline — inert text promises no click).
    let hovered = active && ctx.hovered().get();
    let underline = match props.underline {
        LinkUnderline::Hover => hovered,
        LinkUnderline::Always => active,
        LinkUnderline::Never => false,
    };
    let mut style = Style::new().align_items(AlignItems::Center);
    if active {
        style = style.cursor(CursorIcon::Pointer);
    }
    if underline {
        style = style.border_bottom(1, ink);
    }
    let builder = Row(&props.debug).style(style).semantics(semantics);
    let builder = match press {
        Some(press) => builder.on_press(move || press()),
        None => builder,
    };
    builder.child(with_ink(
        VNode::from(Text {
            text: props.label.clone(),
            style: props.style,
        }),
        ink,
    ))
}
