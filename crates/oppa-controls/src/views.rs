use crate::button::{Button, ButtonProps};
use crate::common::*;
use oppa::*;
use oppa_macros::Props;
use std::rc::Rc;
// ---------------------------------------------------------------------------
// FilePicker (Phase 38c, G21)
// ---------------------------------------------------------------------------

/// FilePicker mode: which dialog backend the browse trigger wraps
/// (one trigger, three backends — the mode selects the backend and
/// its options, never two at once).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FilePickerMode {
    #[default]
    Open,
    Save,
    Folder,
}

/// FilePicker props (controlled `path` — the picked destination as
/// display text; `None` until the first successful pick — dismissed
/// dialogs leave it untouched, never clear it). Backends are
/// runner-owned handles (the KitchenSink `ScriptedDialog` precedent
/// — the shell seam returns `&mut` per pump, so runners hold and
/// forward the handle; the control drives them through its own
/// render-stable [`oppa::PickSession`] signal). Options ride per
/// mode (each mode reads its own — `open_options` / `save_options`
/// / `folder_options`).
#[derive(Clone, Props)]
pub struct FilePickerProps {
    pub path: Signal<Option<SharedString>>,
    pub mode: FilePickerMode,
    pub open_dialog: Option<std::rc::Rc<std::cell::RefCell<dyn oppa::FileDialog>>>,
    /// The unified pick session driving `open_dialog` (decision
    /// 429/431): one shared session per backend — apps forward the
    /// stable handle beside the backend (the sink forwards
    /// `SinkDialogs::open_session`), never a fresh one per render
    /// (props rebuild every render; a fresh session resets to idle
    /// and every press requests over the awaiting settlement).
    /// `new()` seeds a fresh session, which suffices for
    /// synchronously-settling backends.
    pub open_session: std::rc::Rc<std::cell::RefCell<oppa::PickSession>>,
    pub save_dialog: Option<std::rc::Rc<std::cell::RefCell<dyn oppa::SaveFileDialog>>>,
    pub folder_dialog: Option<std::rc::Rc<std::cell::RefCell<dyn oppa::FolderDialog>>>,
    pub open_options: oppa::FilePickerOptions,
    pub save_options: oppa::FileDialogOptions,
    pub folder_options: oppa::FolderDialogOptions,
    pub browse_label: SharedString,
    pub enabled: bool,
    pub debug: SharedString,
}

impl FilePickerProps {
    pub fn new(path: Signal<Option<SharedString>>, mode: FilePickerMode) -> Self {
        Self {
            path,
            mode,
            open_dialog: None,
            open_session: std::rc::Rc::new(std::cell::RefCell::new(oppa::PickSession::new())),
            save_dialog: None,
            folder_dialog: None,
            open_options: oppa::FilePickerOptions::default(),
            save_options: oppa::FileDialogOptions::default(),
            folder_options: oppa::FolderDialogOptions::default(),
            browse_label: SharedString::from("Browse…"),
            enabled: true,
            debug: SharedString::from("file-picker"),
        }
    }
}

/// Browse/save/folder trigger wrapping the dialog request/poll
/// protocol (the ~30-lines-of-glue gap, closed): one Browse button,
/// the picked-path display, and an instance-owned status caption.
/// The display is filename-first two-line (decision 430): the file
/// name on line one, the parent directory wrapping beneath in
/// secondary ink under the row's remaining width — a single
/// absolute-path line overflows on phones. Bare names and `(none)`
/// render one line.
/// The open mode drives the unified [`oppa::PickSession`] (decision
/// 429 — one press series per dialog on every platform: a press
/// consumes an awaiting settlement instead of requesting over it,
/// requests only when idle, and consumes each settlement once, so
/// blocking shells settle in-press while async shells settle across
/// presses with the same observable captions). Dismissal
/// (`Ok(vec![])` / `Ok(None)`) and refusals surface in the caption
/// and leave the path untouched (dismissal is data — decision 230,
/// never a clear). A missing backend for the chosen mode refuses
/// loudly (wiring bug, never a silent button). Disabled drops the
/// trigger structurally (decision 213).
pub fn FilePicker(ctx: &Ctx, props: &FilePickerProps) -> VNode {
    let status = ctx.signal(SharedString::from(""));
    let browse = {
        let (path, status) = (props.path.clone(), status.clone());
        let mode = props.mode;
        let (open_dlg, open_session, save_dlg, folder_dlg) = (
            props.open_dialog.clone(),
            props.open_session.clone(),
            props.save_dialog.clone(),
            props.folder_dialog.clone(),
        );
        let (open_opts, save_opts, folder_opts) = (
            props.open_options.clone(),
            props.save_options.clone(),
            props.folder_options.clone(),
        );
        action(move || match mode {
            FilePickerMode::Open => {
                let Some(dlg) = open_dlg.clone() else {
                    panic!("file picker in Open mode without an open backend — refused, never a silent button");
                };
                let mut dlg = dlg.borrow_mut();
                let mut session = open_session.borrow_mut();
                let outcome = oppa::OpenOutcome::of(session.press(&mut *dlg, open_opts.clone()));
                if let Some(first) = outcome.path_text() {
                    path.set(Some(SharedString::from(first)));
                }
                status.set(SharedString::from(outcome.status()));
            }
            FilePickerMode::Save => {
                let Some(dlg) = save_dlg.clone() else {
                    panic!("file picker in Save mode without a save backend — refused, never a silent button");
                };
                let mut backend = dlg.borrow_mut();
                match backend.save(save_opts.clone()) {
                    Ok(Some(p)) => {
                        path.set(Some(SharedString::from(p.to_string_lossy().into_owned())));
                        status.set(SharedString::from(format!("save to {}", p.display())));
                    }
                    Ok(None) => status.set(SharedString::from("dismissed")),
                    Err(e) => status.set(SharedString::from(format!("picker failed: {e}"))),
                }
            }
            FilePickerMode::Folder => {
                let Some(dlg) = folder_dlg.clone() else {
                    panic!("file picker in Folder mode without a folder backend — refused, never a silent button");
                };
                let mut backend = dlg.borrow_mut();
                match backend.pick(folder_opts.clone()) {
                    Ok(Some(p)) => {
                        path.set(Some(SharedString::from(p.to_string_lossy().into_owned())));
                        status.set(SharedString::from(format!("folder {}", p.display())));
                    }
                    Ok(None) => status.set(SharedString::from("dismissed")),
                    Err(e) => status.set(SharedString::from(format!("picker failed: {e}"))),
                }
            }
        })
    };
    let browse_btn = ctx.child_auto(
        &ButtonProps {
            label: props.browse_label.clone(),
            enabled: props.enabled,
            width: 96.0,
            height: 32.0,
            debug: SharedString::from(format!("{}-browse", props.debug.as_ref())),
            on_press: browse,
            icon: None,
            icon_size: 16.0,
        },
        Button,
    );
    // Path display (decision 430): filename-first two lines — the
    // file name (what was picked) on line one, the parent directory
    // (where it lives) wrapping beneath in secondary ink. A single
    // absolute-path line overflows the row on phones (observed
    // `/data/user/0/…tapme.txt` running off-screen); the name is
    // short by construction while the directory wraps under the
    // row's remaining width (`fill_width` gives the text its finite
    // wrap constraint — unconstrained text measures one overflowing
    // line). `(none)` stays one line, as before.
    let mut path_lines = vec![VNode::from(Text {
        text: match props.path.get() {
            Some(p) => {
                let s: &str = &p;
                let name = std::path::Path::new(s)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|n| !n.is_empty());
                SharedString::from(name.unwrap_or(s))
            }
            None => SharedString::from("(none)"),
        },
        style: Text::body_secondary,
    })];
    if let Some(p) = props.path.get() {
        let s: &str = &p;
        if let Some(dir) = std::path::Path::new(s)
            .parent()
            .and_then(|d| d.to_str())
            .filter(|d| !d.is_empty())
        {
            path_lines.push(VNode::from(Text::new(dir).size(12)));
        }
    }
    let mut row_children = vec![
        browse_btn,
        Column::new()
            .style(Style::new().fill_width())
            .children(path_lines),
    ];
    if !status.get().to_string().is_empty() {
        row_children.push(with_ink(
            VNode::from(Text::new(status.get()).size(12)),
            ctx.theme().tokens().text_secondary,
        ));
    }
    Row(format!("{}-row", props.debug.as_ref()).as_str())
        .style(Style::new().gap(8).align_items(AlignItems::Center))
        .children(row_children)
}

// ---------------------------------------------------------------------------
// G22 control wrappers (Phase 38c — the Phase 36 leaves as components)
// ---------------------------------------------------------------------------

/// RichText display control: multi-span text over the Phase 36
/// `VNode::RichText` leaf (shape-per-span then join, shared size —
/// decision 355). Display-only (no handlers, no editing — the
/// caret/selection space stays the joined bytes by construction).
/// A `label` wraps the leaf in a named `Div` carrying it (leaves
/// carry no semantics payload — `VNode::RichText` has none, so the
/// wrapper announces, never the leaf); `None` returns the bare leaf
/// (byte-identical to the hand-built `VNode`).
#[derive(Clone, Props)]
pub struct RichTextViewProps {
    pub spans: Vec<oppa::TextSpan>,
    pub style: TextClass,
    pub label: Option<SharedString>,
    pub debug: SharedString,
}

pub fn RichTextView(ctx: &Ctx, props: &RichTextViewProps) -> VNode {
    let leaf = VNode::from(oppa::RichText::new(props.spans.clone()).style(props.style));
    let Some(label) = props.label.clone() else {
        return leaf;
    };
    let _ = ctx;
    Div(&props.debug)
        .semantics(Semantics::default().label(&label))
        .child(leaf)
}

/// Image control: static-image leaf over the Phase 36 one-deposit
/// cache (decision 359 — `insert_pixels` once, every backend serves
/// from the deposit). `image` is the author-resolved cache id (the
/// handle stays app-owned like `KvStore` handles — `load` never
/// fails, so resolution is one call, never a fallible seam in the
/// body). `alt` names the image (`""` = decorative intent — the
/// payload carries it opaquely, like `value_text`).
#[derive(Clone, Props)]
pub struct ImageViewProps {
    pub image: oppa::ImageId,
    pub size: f32,
    pub radius: f32,
    pub alt: SharedString,
}

pub fn ImageView(_ctx: &Ctx, props: &ImageViewProps) -> VNode {
    let vnode = VNode::from(oppa::Img {
        src: props.image,
        size: props.size,
        radius: props.radius,
    });
    let VNode::Element(mut el) = vnode else {
        panic!("image leaf is not an element — refused, never silent");
    };
    el.semantics = Some(Semantics::default().label(&props.alt));
    VNode::Element(el)
}

/// Canvas control: retained vector surface over the Phase 36
/// `Tag::Canvas` leaf (decision 358 — lowers to Rect/RRect/Path/Text
/// ops, never a new `DrawOp`). `ops` replay through the [`Canvas`]
/// builder (same loud refusals — empty/invalid/paintless never
/// build silently); geometry rides the explicit `width`/`height`
/// (childless leaf, explicit-or-zero box — content never sizes
/// it). A `label` attaches to the element (which carries semantics,
/// unlike the RichText leaf).
#[derive(Clone, Props)]
pub struct CanvasViewProps {
    pub ops: Vec<oppa::CanvasOp>,
    pub width: f32,
    pub height: f32,
    pub label: Option<SharedString>,
    pub debug: SharedString,
}

pub fn CanvasView(_ctx: &Ctx, props: &CanvasViewProps) -> VNode {
    let mut b = oppa::Canvas::new(&props.debug).style(Style::new().size(props.width, props.height));
    for op in &props.ops {
        b = match op {
            oppa::CanvasOp::Rect { x, y, w, h, color } => {
                b.rect(x.get(), y.get(), w.get(), h.get(), *color)
            }
            oppa::CanvasOp::RRect {
                x,
                y,
                w,
                h,
                radius,
                color,
            } => b.rrect(x.get(), y.get(), w.get(), h.get(), radius.get(), *color),
            oppa::CanvasOp::Path { data, fill, stroke } => b.path(data, *fill, *stroke),
            oppa::CanvasOp::Text {
                text,
                size_px,
                weight,
                ink,
                x,
                y,
            } => b.text(text, *size_px, *weight, *ink, x.get(), y.get()),
        };
    }
    let vnode = b.build();
    let Some(label) = props.label.clone() else {
        return vnode;
    };
    let VNode::Element(mut el) = vnode else {
        panic!("canvas leaf is not an element — refused, never silent");
    };
    el.semantics = Some(Semantics::default().label(&label));
    VNode::Element(el)
}

/// One route view: name + factory (the TabItem-content precedent —
/// factories rebuild per render and stay lazy; inactive routes
/// build nothing).
#[derive(Clone)]
pub struct RouteView {
    pub name: SharedString,
    pub content: Rc<dyn Fn(&Ctx) -> VNode>,
}

/// NavHost props: declarative router over an author-owned
/// [`NavStack`](oppa::NavStack) (stack position is identity —
/// decision 218; the stack signal is the single truth, like
/// Select-`open`). Renders the current route's factory; an unknown
/// route name renders a quiet empty panel (the unmatched-Tabs
/// precedent — controlled-contract edge, documented not silent).
/// System back flows through the runner-owned BackPress chain
/// (`handle_back` → [`BackOutcome::Unhandled`](oppa::BackOutcome)
/// → runner pops or exits — `nav.rs`): the host never pops what it
/// does not own, so back wiring stays runner-side, stated.
#[derive(Clone, Props)]
pub struct NavHostProps {
    pub stack: Signal<oppa::NavStack>,
    pub routes: Vec<RouteView>,
}

pub fn NavHost(ctx: &Ctx, props: &NavHostProps) -> VNode {
    let current = props.stack.get().current().cloned();
    let Some(route) = current else {
        return Div("nav-empty").build();
    };
    match props
        .routes
        .iter()
        .find(|entry| entry.name.to_string() == route.name)
    {
        Some(entry) => (entry.content)(ctx),
        None => Div("nav-empty").build(),
    }
}
