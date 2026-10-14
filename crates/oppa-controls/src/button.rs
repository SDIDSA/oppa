use crate::common::*;
use oppa::*;
// ---------------------------------------------------------------------------
// Button
// ---------------------------------------------------------------------------

/// Vector path icon for [`ButtonProps::vector_icon`] (decision 510):
/// single-path SVG data with its ink and render size — the native
/// `Path` twin of the baked [`ImageCache`](oppa::ImageCache) slot
/// (filled outlines suit the tessellator; decorative and handlerless,
/// never a tab stop).
#[derive(Clone)]
pub struct ButtonVectorIcon {
    pub data: SharedString,
    pub ink: Color,
    pub size: f32,
}

/// Label centering rule for icon buttons (decision 510): `Group`
/// (default) centers the icon+label group as one unit; `FullWidth`
/// centers the label on the full button width with symmetric insets
/// (the icon rides the left pad, an equal spacer balances the right —
/// the sera-login sign-in recipe, where the glyph must not push the
/// label off center). Iconless buttons render identically either way.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ButtonLabelCenter {
    #[default]
    Group,
    FullWidth,
}

/// Button props (stateless — activation rides `on_press`).
///
/// Leading icon (decision 478): `icon` takes an [`ImageCache`](oppa::ImageCache)
/// id (deposit SVGs once through [`deposit_svg_icon`](crate::deposit_svg_icon))
/// painted at `icon_size` ahead of the label; the icon+label group
/// stays centered as one unit. `None` (default) renders exactly the
/// pre-478 tree — same box, same label, same tab order (the icon is
/// decorative and handlerless, never a stop).
///
/// Brand overrides (decision 510 — the sera-login hand-rolled press
/// row, now expressible): `fill` / `pressed_fill` / `ink` / `radius`
/// / `text_style` each default to `None`, which keeps the exact
/// shipped themed pixels; setting any of them pins that dimension
/// (dark ink on a periwinkle fill, 12px card radius, 15px semibold
/// labels) while press/toggle/focus/tab semantics stay the control's.
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
    /// Native vector icon slot (decision 510): SVG path data painted
    /// as a `Path` leaf at its own size ahead of the label (same
    /// centering rule as [`icon`](Self::icon)). Setting both icon
    /// slots refuses loudly (one leading visual, never two).
    pub vector_icon: Option<ButtonVectorIcon>,
    /// Fill override (default `None` — the theme `primary`, or
    /// `text_secondary` when disabled).
    pub fill: Option<Color>,
    /// Pressed-fill override (default `None` — `primary_pressed`
    /// over the theme primary, current fill otherwise).
    pub pressed_fill: Option<Color>,
    /// Label ink override (default `None` — the shipped label ink).
    pub ink: Option<Color>,
    /// Corner-radius override (default `None` — the shipped 6px).
    pub radius: Option<f32>,
    /// Label text-class override (default `None` — the shipped
    /// `body_secondary`).
    pub text_style: Option<TextClass>,
    /// Label centering rule for icon buttons (default
    /// [`Group`](ButtonLabelCenter::Group) — the shipped centered
    /// unit).
    pub label_center: ButtonLabelCenter,
    /// Full-width box (default false — the shipped fixed `width ×
    /// height`; true spans the parent width at `height`, the card
    /// submit-button shape).
    pub full_width: bool,
    /// Horizontal content inset in px (default 0.0 — the shipped
    /// edge-to-edge content; the submit recipe insets 12px each side).
    pub content_pad: f32,
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
            vector_icon: None,
            fill: None,
            pressed_fill: None,
            ink: None,
            radius: None,
            text_style: None,
            label_center: ButtonLabelCenter::Group,
            full_width: false,
            content_pad: 0.0,
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

    /// Height-only size (width rides [`full_width`](Self::full_width)
    /// or the shipped `width` untouched).
    pub fn height(mut self, height: f32) -> Self {
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

    /// Native vector icon slot (see
    /// [`vector_icon`](Self::vector_icon)): SVG path data painted as
    /// a `Path` leaf in `ink` at `size` px ahead of the label.
    pub fn vector_icon(mut self, data: &str, ink: Color, size: f32) -> Self {
        self.vector_icon = Some(ButtonVectorIcon {
            data: SharedString::from(data),
            ink,
            size,
        });
        self
    }

    /// Fill override (see [`fill`](Self::fill)).
    pub fn fill(mut self, fill: Color) -> Self {
        self.fill = Some(fill);
        self
    }

    /// Pressed-fill override (see [`pressed_fill`](Self::pressed_fill)).
    pub fn pressed_fill(mut self, fill: Color) -> Self {
        self.pressed_fill = Some(fill);
        self
    }

    /// Label ink override (see [`ink`](Self::ink)).
    pub fn ink(mut self, ink: Color) -> Self {
        self.ink = Some(ink);
        self
    }

    /// Corner-radius override (see [`radius`](Self::radius)).
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    /// Label text-class override (see
    /// [`text_style`](Self::text_style)).
    pub fn text_style(mut self, style: TextClass) -> Self {
        self.text_style = Some(style);
        self
    }

    /// Centers the label on the full button width (see
    /// [`FullWidth`](ButtonLabelCenter::FullWidth)).
    pub fn full_width_label(mut self) -> Self {
        self.label_center = ButtonLabelCenter::FullWidth;
        self
    }

    /// Full-width box (see [`full_width`](Self::full_width)).
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    /// Horizontal content inset (see
    /// [`content_pad`](Self::content_pad)).
    pub fn content_pad(mut self, px: f32) -> Self {
        self.content_pad = px;
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
/// (Round 11.2 — Light reproduces the catalog palette exactly),
/// unless the decision-510 brand overrides pin them.
pub fn Button(ctx: &Ctx, props: &ButtonProps) -> VNode {
    let t = ctx.theme().tokens();
    if props.icon.is_some() && props.vector_icon.is_some() {
        panic!(
            "controls: Button {:?} sets both icon slots — one leading visual, never two",
            props.debug
        );
    }
    let semantics = Semantics::button()
        .label(&props.label)
        .disabled(!props.enabled);
    let press = props.enabled.then(|| props.on_press.clone());
    // Pressed flag rides this instance (each button tints alone;
    // disabled never flags by router construction).
    let pressed = props.enabled && ctx.pressed().get();
    // Hover wash (decision 514): enabled buttons read lighter while
    // hovered (never the pressed depth — distinct affordances).
    let hovered = props.enabled && ctx.hovered().get();
    let themed_bg = if props.enabled {
        t.primary
    } else {
        t.text_secondary
    };
    let mut bg = props.fill.unwrap_or(themed_bg);
    if pressed {
        if let Some(pressed_fill) = props.pressed_fill {
            bg = pressed_fill;
        } else if props.fill.is_none() && bg == t.primary {
            bg = t.primary_pressed;
        }
    } else if hovered {
        bg = crate::motion::hover_fill(bg);
    }
    // Round 8.3: enabled buttons show the hand (disabled stays the
    // platform arrow — inert controls never promise clicks).
    let mut button_style = if props.full_width {
        Style::new().fill_width().h(props.height)
    } else {
        Style::new().size(props.width, props.height)
    };
    button_style = button_style
        .radius(props.radius.unwrap_or(6.0))
        .bg(bg)
        // Decision 514: press/hover swaps tween (one feel, no app
        // code — static scenes declare but never run it).
        .transition(Transition::micro());
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
    let mut label_style = if props.full_width {
        Style::new().fill_width().h(props.height)
    } else {
        Style::new().size(props.width, props.height)
    };
    label_style = label_style
        .justify_content(JustifyContent::Center)
        .align_items(AlignItems::Center);
    if props.content_pad > 0.0 {
        label_style = label_style.pad_x(props.content_pad);
    }
    if props.icon.is_some() || props.vector_icon.is_some() {
        label_style = label_style.gap(8);
    }
    let label_leaf = VNode::from(Text {
        text: props.label.clone(),
        style: props.text_style.unwrap_or(Text::body_secondary),
    });
    let label_leaf = match props.ink {
        Some(ink) => with_ink(label_leaf, ink),
        None => label_leaf,
    };
    let icon_cell: Option<VNode> = props
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
        .or_else(|| {
            props.vector_icon.as_ref().map(|v| {
                Div(format!("{}-icon", props.debug.as_ref()).as_str())
                    .style(
                        Style::new()
                            .size(v.size, v.size)
                            .align_items(AlignItems::Center)
                            .justify_content(JustifyContent::Center),
                    )
                    .child(
                        Path::new(format!("{}-icon-path", props.debug.as_ref()).as_str())
                            .data(&v.data)
                            .fill(v.ink)
                            .size(v.size, v.size)
                            .build(),
                    )
            })
        });
    // FullWidth pins the label to the true center: the icon rides the
    // left pad while an equal spacer balances the right, so the middle
    // column lands exactly on center (row-level centering lands it on
    // middle height). Group keeps the shipped icon+label unit.
    if props.label_center == ButtonLabelCenter::FullWidth {
        if let Some(icon_cell) = icon_cell {
            let icon_px = props
                .vector_icon
                .as_ref()
                .map(|v| v.size)
                .unwrap_or(props.icon_size);
            return builder.child(
                Row("button-label").style(label_style).children([
                    icon_cell,
                    Column::new()
                        .style(
                            Style::new()
                                .flex_grow(1.0)
                                .align_items(AlignItems::Center)
                                .justify_content(JustifyContent::Center),
                        )
                        .children([label_leaf]),
                    Div(format!("{}-pad", props.debug.as_ref()).as_str())
                        .style(Style::new().size(icon_px, icon_px))
                        .build(),
                ]),
            );
        }
    }
    builder.child(
        Row("button-label")
            .style(label_style)
            .children(icon_cell.into_iter().chain([label_leaf])),
    )
}
