use crate::common::*;
use crate::menu;
use oppa::*;
use oppa_macros::Props;
// ---------------------------------------------------------------------------
// Tooltip
// ---------------------------------------------------------------------------

/// Tooltip props (Round 17.3, decision 319): wraps author content,
/// opens an anchored portal tooltip card after `delay_ms` dwell
/// (default 500ms), and dismisses on hover-leave or press.
#[derive(Clone, Props)]
pub struct TooltipProps<C> {
    pub tip: SharedString,
    pub content: fn(&Ctx, &C) -> VNode,
    pub content_props: C,
    pub delay_ms: u64,
    pub debug: SharedString,
}

impl<C> TooltipProps<C> {
    pub fn new(
        tip: impl Into<SharedString>,
        content: fn(&Ctx, &C) -> VNode,
        content_props: C,
    ) -> Self {
        Self {
            tip: tip.into(),
            content,
            content_props,
            delay_ms: 500,
            debug: SharedString::from("tooltip"),
        }
    }

    pub fn delay_ms(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    pub fn debug(mut self, debug: impl Into<SharedString>) -> Self {
        self.debug = debug.into();
        self
    }
}

/// Anchored tooltip over author content (Round 17.3, decision 319):
/// mounts an anchored card portal after dwell time (default 500ms)
/// under pointer hover, and auto-dismisses on leave or press.
pub fn Tooltip<C: Props>(ctx: &Ctx, props: &TooltipProps<C>) -> VNode {
    let host = ctx.host();
    let inst = ctx.instance_id();
    let _ = ctx.hover_move().get();
    let t = ctx.theme().tokens();
    let anchor_label = format!("tooltip-anchor-{inst}");
    let open = ctx.signal(false);
    let hover_start = ctx.signal(Option::<f64>::None);

    let anchor_id = oppa::find_retained_by_debug(&host, &anchor_label)
        .into_iter()
        .next();

    let hovered = ctx.hovered().get()
        || anchor_id.is_some_and(|aid| {
            if let Some(mut cur) = host.hovered_node() {
                host.with_retained_mut(|rec, _| loop {
                    if cur == aid {
                        return true;
                    }
                    if let Some(n) = rec.get(cur) {
                        if let Some(p) = n.parent {
                            cur = p;
                            continue;
                        }
                    }
                    return false;
                })
            } else {
                false
            }
        });

    let pressed = ctx.pressed().get()
        || anchor_id.is_some_and(|aid| {
            if let Some(mut cur) = host.capture_node() {
                host.with_retained_mut(|rec, _| loop {
                    if cur == aid {
                        return true;
                    }
                    if let Some(n) = rec.get(cur) {
                        if let Some(p) = n.parent {
                            cur = p;
                            continue;
                        }
                    }
                    return false;
                })
            } else {
                false
            }
        });

    let now = host.runtime().now_secs();
    if pressed || !hovered {
        if open.get() {
            open.set(false);
        }
        if hover_start.get().is_some() {
            hover_start.set(None);
        }
    } else {
        match hover_start.get() {
            None => {
                hover_start.set(Some(now));
            }
            Some(start) => {
                let elapsed_ms = ((now - start) * 1000.0).max(0.0) as u64;
                if elapsed_ms >= props.delay_ms && !open.get() {
                    open.set(true);
                }
            }
        }
    }

    let anchor_box = host.committed_box_by_debug(&anchor_label);
    let ay_off = anchor_box.as_ref().map(|b| b.h + 4.0).unwrap_or(28.0);

    let popup = if open.get() {
        let card = Div("tooltip-card")
            .style(
                Style::new()
                    .bg(t.surface)
                    .border(1, t.border)
                    .radius(4)
                    .pad_x(8)
                    .pad_y(4),
            )
            .semantics(Semantics::default().label(&props.tip))
            .child(with_ink(
                VNode::from(Text {
                    text: props.tip.clone(),
                    style: Text::body_secondary,
                }),
                t.text_primary,
            ));
        // Viewport-edge clamping (Round 21.3, decision 330): flip
        // above/left when the settled card would overflow the
        // viewport. One settled read per (wrapper origin, tip):
        // the pre-layout pass renders below the anchor and stays
        // unplaced; the placed pass commits and never reads settled
        // again — a persistent generation subscription here spins
        // exactly like Menu's did (re-render re-dirties layout
        // through the content child).
        let (vw, vh) = host.viewport_size();
        let (wx, wy) = anchor_box
            .as_ref()
            .map(|b| (b.x, b.y))
            .unwrap_or((0.0, 0.0));
        let tplaced = ctx.signal(None::<((f32, f32), SharedString, (f32, f32))>);
        let tip_key = props.tip.clone();
        let origin = match tplaced.get() {
            Some((w0, t0, o)) if w0 == (wx, wy) && t0 == tip_key => o,
            _ => match host
                .settled_box_by_debug("tooltip-card")
                .map(|b| (b.w, b.h))
            {
                None => (0.0, ay_off),
                Some((cw, ch)) => {
                    let (cx, cy) = menu::clamp_popup_anchor((vw, vh), (wx, wy + ay_off), (cw, ch));
                    let o = (cx - wx, cy - wy);
                    if tplaced.get() != Some(((wx, wy), tip_key.clone(), o)) {
                        tplaced.set(Some(((wx, wy), tip_key.clone(), o)));
                    }
                    o
                }
            },
        };
        Portal("tooltip-portal")
            .style(Style::new().x(origin.0).absolute_y(origin.1))
            .child(card)
    } else {
        Portal("tooltip-closed")
            .style(Style::new().size(0, 0))
            .build()
    };

    let content_node = (props.content)(ctx, &props.content_props);
    let (open_h, hover_h) = (open.clone(), hover_start.clone());
    Div(anchor_label.as_str())
        .on_press(move || {
            open_h.set(false);
            hover_h.set(None);
        })
        .children([content_node, popup])
}
