use crate::common::*;
use oppa::*;
use std::rc::Rc;
// ---------------------------------------------------------------------------
// TextInput
// ---------------------------------------------------------------------------
/// TextInput props (controlled — `value` is the author-owned content
/// signal; editing state lives in the keyed [`EditSession`](oppa::EditSession)).
/// `on_change` reports committed edits (round 5.4 — `None` keeps the
/// exact pre-5.4 behavior); see [`UncontrolledTextInput`] for the
/// self-managed companion.
///
/// Form validation (Phase 38a, decision 366 — G7): `invalid` /
/// `required` / `error_message` announce through [`Semantics`]
/// (validators stay app-side); `helper_text` renders a dimmed caption
/// under the field (visual-only). All four default off/empty —
/// `new` keeps the exact pre-38a shape.
#[derive(Clone)]
pub struct TextInputProps {
    pub label: SharedString,
    pub value: Signal<SharedString>,
    pub placeholder: Option<SharedString>,
    pub enabled: bool,
    pub width: f32,
    pub height: f32,
    pub style: TextClass,
    pub debug: SharedString,
    pub on_change: Option<Change<SharedString>>,
    pub masked: bool,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
    pub borderless: bool,
}

impl Props for TextInputProps {}

impl TextInputProps {
    pub fn new(label: &str, value: Signal<SharedString>) -> Self {
        Self {
            label: SharedString::from(label),
            value,
            placeholder: None,
            enabled: true,
            width: 200.0,
            height: 32.0,
            style: Text::body_secondary,
            debug: SharedString::from("text-input"),
            on_change: None,
            masked: false,
            invalid: false,
            required: false,
            error_message: None,
            helper_text: None,
            borderless: false,
        }
    }

    pub fn borderless(mut self, borderless: bool) -> Self {
        self.borderless = borderless;
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
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

    pub fn style(mut self, style: TextClass) -> Self {
        self.style = style;
        self
    }

    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }
}

/// Text input: bordered field box + [`TextField`](oppa::TextField) payload
/// (the DOM backend's verdict-(b) `<input>` shape; native backends render
/// the text run). The [`EditSession`](oppa::EditSession) is keyed to this
/// instance, so caret/selection/undo survive hot reloads and swaps; the
/// value signal stays the controlled source of truth (locked #24).
/// Press focuses through the M5 router (press ownership); the handlers are
/// shaper-aware tap-to-caret (Round 8.1, decision 297 — the tap point maps
/// through `click_x`, Shift+Click through `shift_click_x`) plus
/// drag-selection and double/triple-click word/line selection (Round 8.2,
/// decision 298 — Moves stream through `drag_x`, count 2 selects the word,
/// count 3+ the hard line; keyboard activation falls back to
/// `caret_to_end`). Disabled drops the handlers structurally
/// (decision 213 — no tab stop either).
/// An empty value with a placeholder renders the placeholder as a plain
/// dimmed `Text` span (never a `TextField`: the leaf's text is the
/// field's bindable value, and the placeholder must never leak into it;
/// on DOM the backend renders `value=""` with a native `placeholder`
/// attribute from that span (decision 293) — typing feeds only typed
/// text through the session fallback, on every backend).
///
/// Web value loop (Phase 39a, decisions 377–379 — the U8 authoring
/// shape): pass an author-owned `Signal<SharedString>` as `value`
/// and read it back by rendering it (no retained `NodeId` ever
/// leaks into app code — the signal IS the field's name). The
/// browser owns insertion, caret, selection, IME, and undo
/// (verdict (b)); each `input` event forwards pid + full value as
/// `InputEvent::Text`, which commits through the session (firing
/// `on_change` exactly like a native insert — validators and
/// bridges observe every commit) and re-renders around the focused
/// field without touching it (keyed patches skip it; composition
/// preedit never forwards — `isComposing` guards the bridge, so
/// framework normalization can never reset a live IME). Programmatic
/// `value.set` flows out through the `value` property; it never
/// re-fires `on_change` (no loop). Rejected commits (a bridge that
/// ignores a parse) simply never write the signal — the next
/// keystroke self-heals (level-triggered values).
pub fn TextInput(ctx: &Ctx, props: &TextInputProps) -> VNode {
    let t = ctx.theme().tokens();
    let session = ctx.edit_session(props.value.clone());
    // Masked backing rides the session (Round 22.1, decision 331):
    // published every render so toggling `masked` takes effect
    // without remount; `UncontrolledTextInput` delegates here, so
    // both spellings are covered by this one line.
    session.set_masked(props.masked);
    // Committed edits report through `on_change` (round 5.4 —
    // installed per render, idempotent; the session owns the
    // funnel, the control owns the subscription).
    if let Some(notify) = props.on_change.clone() {
        session.set_on_change(Rc::new(move |value| notify(value)));
    }
    // Phase 38a (decision 366): validation marks announce; the edge
    // tints `error` while invalid (valid fields keep exact pixels).
    let semantics = with_validation_marks(
        Semantics::text_field()
            .label(&props.label)
            .disabled(!props.enabled),
        props.invalid,
        props.required,
        &props.error_message,
    );
    let raw_val = props.value.get();
    let display_text = if props.masked && !raw_val.is_empty() {
        SharedString::from("•".repeat(raw_val.chars().count()))
    } else {
        raw_val.clone()
    };
    let payload: VNode = match (raw_val.is_empty(), &props.placeholder) {
        (true, Some(placeholder)) => with_ink(
            VNode::from(Text {
                text: placeholder.clone(),
                style: props.style,
            }),
            t.text_secondary,
        ),
        _ => TextField {
            text: display_text,
            style: props.style,
            label: props.label.clone(),
        }
        .into(),
    };
    // (One `.style()` call — the builder replaces the whole style per
    // call, so size/bg/border/pad fold into a single chain. Round 8.3:
    // enabled fields show the I-beam; disabled stays the arrow.
    // Phase 38a: the border edges `error` while invalid.)
    let mut field_style = Style::new().size(props.width, props.height);
    if props.borderless {
        field_style = field_style.pad_y(4);
    } else {
        field_style = field_style
            .radius(4)
            .bg(if props.enabled { t.surface } else { t.disabled })
            .border(1, field_edge(ctx, props.invalid))
            .pad_x(8)
            .pad_y(4);
    }
    if props.enabled {
        field_style = field_style.cursor(CursorIcon::Text);
    }
    let builder = Div(&props.debug)
        // Round 23.2: keyboard focus swaps the gray edge for the
        // themed ring (same inset band, recolored — layout-safe).
        // Borderless fields inherit container chrome (no double box).
        .style(if props.borderless {
            field_style
        } else {
            focus_ringed(ctx, props.enabled, field_style)
        })
        .semantics(semantics);
    let builder = if props.enabled {
        let host = ctx.host();
        let tap_session = session.clone();
        let tap_style = props.style;
        let drag_host = host.clone();
        let drag_session = tap_session.clone();
        builder
            .on_press(move || {
                field_press(&host, &tap_session, tap_style);
            })
            .on_drag(move || {
                field_drag(&drag_host, &drag_session, tap_style);
            })
    } else {
        builder
    };
    // Phase 38a: footer stacks under the field only when a message
    // is present (the wrapper debug derives from the field debug).
    let debug = props.debug.clone();
    let root = builder.child(payload);
    with_validation_footer(
        root,
        ctx,
        &debug,
        props.invalid,
        &props.error_message,
        &props.helper_text,
    )
}

/// Uncontrolled text input (round 5.4, OQ-G2-4): self-managed
/// `value` from `initial`, committed edits reported through
/// `on_change` (payload-carrying — the session hook fires with
/// the new content). See [`UncontrolledToggle`] for the contract
/// notes.
#[derive(Clone)]
pub struct UncontrolledTextInputProps {
    pub label: SharedString,
    pub initial: SharedString,
    pub placeholder: Option<SharedString>,
    pub enabled: bool,
    pub width: f32,
    pub height: f32,
    pub style: TextClass,
    pub debug: SharedString,
    pub on_change: Option<Change<SharedString>>,
    pub masked: bool,
    pub invalid: bool,
    pub required: bool,
    pub error_message: Option<SharedString>,
    pub helper_text: Option<SharedString>,
}

impl Props for UncontrolledTextInputProps {}

impl UncontrolledTextInputProps {
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }
}

pub fn UncontrolledTextInput(ctx: &Ctx, props: &UncontrolledTextInputProps) -> VNode {
    let value = ctx.signal(props.initial.clone());
    ctx.child_auto(
        &TextInputProps {
            label: props.label.clone(),
            value,
            placeholder: props.placeholder.clone(),
            enabled: props.enabled,
            width: props.width,
            height: props.height,
            style: props.style,
            debug: props.debug.clone(),
            on_change: props.on_change.clone(),
            masked: props.masked,
            invalid: props.invalid,
            required: props.required,
            error_message: props.error_message.clone(),
            helper_text: props.helper_text.clone(),
            borderless: false,
        },
        TextInput,
    )
}
