use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

/// Button props (stateless — activation rides `on_press`).
///
/// Leading icon (decision 478): `icon` takes an [`ImageCache`](oppa::ImageCache)
/// id (deposit SVGs once through [`deposit_svg_icon`](crate::deposit_svg_icon))
/// painted at `icon_size` ahead of the label; the icon+label group
/// stays centered as one unit. `None` (default) renders exactly the
/// pre-478 tree — same box, same label, same tab order (the icon is
/// decorative and handlerless, never a stop).
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
    pub icon: Option<oppa::ImageId>,
    pub icon_size: f32,
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
            icon: None,
            icon_size: 16.0,
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

    /// Leading icon slot (decision 478): `ImageCache` id painted at
    /// `icon_size` (default 16px — override with
    /// [`icon_size`](Self::icon_size)); `None` (default) keeps the
    /// exact pre-478 tree.
    pub fn icon(mut self, image: oppa::ImageId) -> Self {
        self.icon = Some(image);
        self
    }

    /// Icon render size in px (only read when
    /// [`icon`](Self::icon) is set).
    pub fn icon_size(mut self, px: f32) -> Self {
        self.icon_size = px;
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
    // Decision 478: with an icon, the label row centers the
    // icon+label group as one unit (the icon is decorative and
    // handlerless); without one this whole branch is skipped —
    // same box, same label, same tab order.
    let mut label_style = Style::new()
        .size(props.width, props.height)
        .justify_content(JustifyContent::Center)
        .align_items(AlignItems::Center);
    if props.icon.is_some() {
        label_style = label_style.gap(8);
    }
    builder.child(
        Row("button-label").style(label_style).children(
            props
                .icon
                .map(|image| {
                    Div(format!("{}-icon", props.debug.as_ref()).as_str())
                        .style(
                            Style::new()
                                .size(props.icon_size, props.icon_size)
                                .align_items(AlignItems::Center)
                                .justify_content(JustifyContent::Center),
                        )
                        .child(crate::ImageView(
                            ctx,
                            &crate::ImageViewProps {
                                image,
                                size: props.icon_size,
                                radius: 0.0,
                                alt: SharedString::from(""),
                            },
                        ))
                })
                .into_iter()
                .chain([VNode::from(Text {
                    text: props.label.clone(),
                    style: Text::body_secondary,
                })]),
        ),
    )
}
