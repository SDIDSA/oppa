use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

/// Button props (stateless — activation rides `on_press`).
#[derive(Clone)]
pub struct ButtonProps {
    pub label: SharedString,
    pub enabled: bool,
    pub width: f32,
    pub height: f32,
    /// Retained debug label (default `"button"`; the Slider sets
    /// `"step-dec"`/`"step-inc"` so tests and diagnostics address each
    /// stepper separately).
    pub debug: SharedString,
    pub on_press: Action,
}

impl Props for ButtonProps {}

impl ButtonProps {
    pub fn new(label: &str, on_press: impl Fn() + 'static) -> Self {
        Self {
            label: SharedString::from(label),
            enabled: true,
            width: 96.0,
            height: 32.0,
            debug: SharedString::from("button"),
            on_press: action(on_press),
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn debug(mut self, debug: &str) -> Self {
        self.debug = SharedString::from(debug);
        self
    }
}

/// Push-button: label + `button` role; press activates (no-op when
/// disabled — structurally handler-less, decision 213). Radius-6
/// Div with a centered label (the label centers through a full-size
/// inner Row — a block-lite Div stretches the leaf full-width — and
/// the outer stays a Div so tab panels keep remounting fresh on
/// incompatible tags). While held, the Primary fill deepens (Round
/// 7.15 pressed feedback). Fill and press tint ride the app theme
/// (Round 11.2 — Light reproduces the catalog palette exactly).
pub fn Button(ctx: &Ctx, props: &ButtonProps) -> VNode {
    let t = ctx.theme().tokens();
    let semantics = Semantics::button()
        .label(&props.label)
        .disabled(!props.enabled);
    let press = props.enabled.then(|| props.on_press.clone());
    // Pressed flag rides this instance (each button tints alone;
    // disabled never flags by router construction).
    let pressed = props.enabled && ctx.pressed().get();
    let mut bg = if props.enabled {
        t.primary
    } else {
        t.text_secondary
    };
    if pressed && bg == t.primary {
        bg = t.primary_pressed;
    }
    // Round 8.3: enabled buttons show the hand (disabled stays the
    // platform arrow — inert controls never promise clicks).
    let mut button_style = Style::new()
        .size(props.width, props.height)
        .radius(6)
        .bg(bg);
    if props.enabled {
        button_style = button_style.cursor(CursorIcon::Pointer);
    }
    let builder = Div(&props.debug)
        // Round 23.2: keyboard focus rings the button (inset —
        // layout-safe, oracle-neutral).
        .style(focus_ringed(ctx, props.enabled, button_style))
        .semantics(semantics);
    // Handlers are payload-less ids (ADR-0007): capture the `Rc` action.
    // (`on_press` rides the builder — `.child()` is terminal, so the
    // handler attaches first.)
    let builder = match press {
        Some(press) => builder.on_press(move || press()),
        None => builder,
    };
    builder.child(
        Row("button-label")
            .style(
                Style::new()
                    .size(props.width, props.height)
                    .justify_content(JustifyContent::Center)
                    .align_items(AlignItems::Center),
            )
            .child(VNode::from(Text {
                text: props.label.clone(),
                style: Text::body_secondary,
            })),
    )
}
