use super::*;
use oppa::{
    Cluster, Color, Div, FontId, FontMetrics, ShapedGlyph, ShapedRun, SharedString, Signal, Style,
    Text, TextError, TextRun,
};
/// Uniform-advance fake shaper (body 14px → 8.75px/char).
struct FakeText;

impl TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(&self, text: &str, style: &oppa::text::TextStyle) -> Result<ShapedRun, TextError> {
        if text.is_empty() {
            return Err(TextError::EmptyText);
        }
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = FontMetrics {
            ascent: em * 0.75,
            descent: em * 0.25,
            line_gap: em * 0.125,
        };
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        for (k, (i, ch)) in text.char_indices().enumerate() {
            let len = ch.len_utf8();
            glyphs.push(ShapedGlyph {
                glyph_id: k as u32,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            clusters.push(Cluster {
                byte_range: (i, i + len),
                glyph_range: (glyphs.len() - 1, glyphs.len()),
            });
        }
        Ok(ShapedRun {
            total_advance: adv * glyphs.len() as f32,
            text_len_bytes: text.len(),
            runs: vec![TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, glyphs.len()),
                rtl: false,
                script: 0,
                font_id: FontId(0),
                font_metrics: metrics,
            }],
            glyphs,
            clusters,
        })
    }
}

#[derive(Clone)]
struct FlipProps {
    on: Signal<bool>,
}
impl Props for FlipProps {}

fn flip_app(_ctx: &Ctx, props: &FlipProps) -> VNode {
    let on = props.on.clone();
    let bg = if on.get() {
        Color(0x22_66_CC)
    } else {
        Color(0x88_88_88)
    };
    Div("screen")
        .style(Style::new().size(200, 150).bg(Color(0xFF_FF_FF)))
        .child(
            Div("flip")
                .style(Style::new().size(96, 32).bg(bg))
                .semantics(oppa::Semantics::button().label("Flip"))
                .on_press(move || on.set(!on.get()))
                .child(VNode::from(Text {
                    text: oppa::SharedString::from("Flip"),
                    style: Text::body_secondary,
                })),
        )
}

fn harness() -> (DesktopLoop, Signal<bool>) {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let on = loop_.host().runtime().signal(false);
    loop_.mount("Flip", FlipProps { on: on.clone() }, flip_app);
    (loop_, on)
}

/// Minimal TextInput-shaped tree (decision 240's shape, local so
/// this crate never depends on the controls catalog): focusable
/// outer with its own session over the value signal.
#[derive(Clone)]
struct FieldProps {
    value: Signal<SharedString>,
}
impl Props for FieldProps {}

fn field_app(ctx: &Ctx, props: &FieldProps) -> VNode {
    let session = ctx.edit_session(props.value.clone());
    Div("field-box")
        .style(Style::new().size(200, 32).bg(Color(0xFF_FF_FF)))
        .semantics(oppa::Semantics::text_field().label("Name"))
        .on_press(move || session.caret_to_end())
        .child(VNode::from(oppa::TextField {
            text: props.value.get(),
            style: Text::body_secondary,
            label: SharedString::from("Name"),
        }))
}

fn field_harness() -> (DesktopLoop, Signal<SharedString>) {
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    let value = loop_.host().runtime().signal(SharedString::from(""));
    loop_.mount(
        "Field",
        FieldProps {
            value: value.clone(),
        },
        field_app,
    );
    (loop_, value)
}

fn focus_field(loop_: &mut DesktopLoop) {
    let id = oppa::find_retained_by_debug(loop_.host(), "field-box")
        .into_iter()
        .next()
        .expect("field box");
    let b = loop_.host().committed_box(id).expect("hit box");
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    loop_
        .step(InputEvent::pointer_down(cx, cy))
        .expect("down steps");
    loop_
        .step(InputEvent::pointer_up(cx, cy))
        .expect("up steps");
    assert_eq!(loop_.host().focused_node(), Some(id));
}

mod basic;
mod dpr_gpu;
mod editing;
mod ime;
mod paint;
mod repaint;
mod scroll;
mod services;
mod win_dpi;
mod win_ime;
