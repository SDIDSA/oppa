use crate::button::{Button, ButtonProps};
use oppa::*;
use std::rc::Rc;
/// Fallback render closure for [`ErrorBoundary`]: receives `(&Ctx, error_message, reset_fn)`.
pub type ErrorFallback = Rc<dyn Fn(&Ctx, &str, &dyn Fn()) -> VNode>;

/// Error listener callback for [`ErrorBoundary`].
pub type ErrorListener = Rc<dyn Fn(&str)>;

/// Props for [`ErrorBoundary`].
#[derive(Clone)]
pub struct ErrorBoundaryProps {
    pub child: Rc<dyn Fn(&Ctx) -> VNode>,
    pub fallback: Option<ErrorFallback>,
    pub on_error: Option<ErrorListener>,
}

impl Props for ErrorBoundaryProps {}

impl ErrorBoundaryProps {
    /// Creates a new error boundary wrapping the given child render closure.
    pub fn new(child: impl Fn(&Ctx) -> VNode + 'static) -> Self {
        Self {
            child: Rc::new(child),
            fallback: None,
            on_error: None,
        }
    }

    /// Custom fallback renderer: `|ctx, error_msg, reset| ...`.
    pub fn fallback(mut self, fallback: impl Fn(&Ctx, &str, &dyn Fn()) -> VNode + 'static) -> Self {
        self.fallback = Some(Rc::new(fallback));
        self
    }

    /// Error notification callback.
    pub fn on_error(mut self, on_error: impl Fn(&str) + 'static) -> Self {
        self.on_error = Some(Rc::new(on_error));
        self
    }
}

/// Error boundary component (Round 18.1, Decision 320):
/// Catches child component unwind panics via `catch_unwind`, preventing
/// host panics and thread crashes. On error, renders a fallback card with
/// error details and a retry trigger. When reset, re-evaluates the child.
pub fn ErrorBoundary(ctx: &Ctx, props: &ErrorBoundaryProps) -> VNode {
    let t = ctx.theme().tokens();
    let retry = ctx.signal(0u64);
    let _ = retry.get(); // re-evaluates on retry trigger

    let reset_action = {
        let retry = retry.clone();
        Rc::new(move || {
            retry.set(retry.get().wrapping_add(1));
        })
    };

    let child = props.child.clone();
    let outcome = ctx.catch_unwind(|| child(ctx));

    match outcome {
        Ok(vnode) => vnode,
        Err(err) => {
            if let Some(ref on_err) = props.on_error {
                on_err(&err);
            }
            if let Some(ref custom_fallback) = props.fallback {
                let r = reset_action.clone();
                custom_fallback(ctx, &err, &move || r())
            } else {
                let r = reset_action.clone();
                let btn_props =
                    ButtonProps::new("Retry", move || r()).debug("error-boundary-retry");
                let retry_btn = ctx.child_auto(&btn_props, Button);
                let card = Div("error-boundary-card")
                    .style(
                        Style::new()
                            .pad_x(16)
                            .pad_y(12)
                            .border(1, t.border)
                            .radius(6)
                            .bg(t.surface),
                    )
                    .child(Column::new().gap(8).children([
                        VNode::from(Text::new(SharedString::from(format!("Error: {err}")))),
                        retry_btn,
                    ]));
                Div("error-boundary-container").child(card)
            }
        }
    }
}
