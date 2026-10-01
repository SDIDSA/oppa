//! Oppa Task Studio (Round 24.1, decision 335): the production
//! reference app. One screen composing the catalog against a
//! [`Collection`](oppa::Collection) of tasks — `DataGrid` +
//! search/sort + per-row status toggles (granular `update_row`,
//! decision 333), a detail inspector (`TextInput` title,
//! multi-line `TextArea` notes, `Select` priority, `Toggle` done),
//! `ContextMenu` row actions (decision 330), `Tooltip` hints,
//! Light/Dark toggle, JSON/CSV export, and a dirty-gated close
//! flow (save `Modal` with the 23.2 focus trap).
//!
//! Runner contract (explicit, never ambient): the runner owns the
//! [`Collection`] (seeded with [`sample_tasks`]) and installs the
//! close handler + file writes; the studio publishes its side of
//! that contract through [`StudioHooks`] (an opt-in outbox of
//! host-bound signals — signals cannot cross runtimes, so they
//! are minted in-body and published once, never passed in).
//! Dirtiness is content-defined (current export vs
//! `saved_snapshot`), never a flag that can desync. The shipped
//! example wires everything except OS close/export (no loop
//! handle through `run_desktop`); the E2E suite drives those
//! legs on a real [`DesktopLoop`](oppa_app::DesktopLoop).

use std::cell::RefCell;
use std::rc::Rc;

use oppa::{
    CanvasOp, Collection, Column, Ctx, Div, FontWeight, Props, Px, Row, RowId, SharedString,
    Signal, Style, Text, TextSpan, ThemeMode, VNode,
};

use super::{
    Action, Button, ButtonProps, CanvasView, CanvasViewProps, Change, ContextMenu,
    ContextMenuProps, DataGrid, DataGridProps, ErrorBoundary, ErrorBoundaryProps, GridCellProps,
    GridColumn, MenuItemProps, MenuTitle, Menubar, MenubarProps, Modal, ModalProps, RichTextView,
    RichTextViewProps, Select, SelectItem, SelectProps, Splitter, SplitterAxis, SplitterProps,
    TextArea, TextAreaProps, TextInput, TextInputProps, Toggle, ToggleProps, Tooltip, TooltipProps,
    UncontrolledTextInput, UncontrolledTextInputProps,
};
use oppa_macros::Props;

// ---------------------------------------------------------------------------
// Model + export
// ---------------------------------------------------------------------------

/// One task row: title, multi-line notes, priority (0 Low, 1
/// Medium, 2 High), done flag.
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub title: SharedString,
    pub notes: SharedString,
    pub priority: u8,
    pub done: bool,
}

/// Priority label (Low/Medium/High — anything else reads Low,
/// never panics on author data).
pub fn priority_label(priority: u8) -> &'static str {
    match priority {
        2 => "High",
        1 => "Medium",
        _ => "Low",
    }
}

/// The canonical five-task seed (shared by the example and the
/// E2E suite — one seed, never two spellings).
pub fn sample_tasks() -> Vec<Task> {
    vec![
        Task {
            title: SharedString::from("Ship the release notes"),
            notes: SharedString::from("Draft is in the shared folder.\nNeeds screenshots."),
            priority: 2,
            done: false,
        },
        Task {
            title: SharedString::from("Buy office coffee"),
            notes: SharedString::from("Dark roast, two bags."),
            priority: 0,
            done: false,
        },
        Task {
            title: SharedString::from("Review the access audit"),
            notes: SharedString::from(""),
            priority: 1,
            done: true,
        },
        Task {
            title: SharedString::from("Fix the flaky sync test"),
            notes: SharedString::from("Retries 3x on CI.\nRepro locally first."),
            priority: 2,
            done: false,
        },
        Task {
            title: SharedString::from("Water the plants"),
            notes: SharedString::from("Friday routine."),
            priority: 0,
            done: true,
        },
    ]
}

fn escape_json_into(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

/// Serializes rows to JSON (hand-rolled — no serde in the
/// workspace; control characters escaped, never raw).
pub fn export_tasks_json(rows: &[Row<Task>]) -> String {
    let mut out = String::from("{\"tasks\":[");
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"id\":{},\"title\":\"", row.id.0));
        escape_json_into(&row.value.title, &mut out);
        out.push_str("\",\"notes\":\"");
        escape_json_into(&row.value.notes, &mut out);
        out.push_str(&format!(
            "\",\"priority\":{},\"done\":{}}}",
            row.value.priority, row.value.done
        ));
    }
    out.push_str("]}");
    out
}

fn quote_csv_into(s: &str, out: &mut String) {
    if s.contains([',', '"', '\n', '\r']) {
        out.push('"');
        for c in s.chars() {
            if c == '"' {
                out.push_str("\"\"");
            } else {
                out.push(c);
            }
        }
        out.push('"');
    } else {
        out.push_str(s);
    }
}

/// Serializes rows to CSV (header + one row per task; fields
/// quoted exactly when they need it).
pub fn export_tasks_csv(rows: &[Row<Task>]) -> String {
    let mut out = String::from("id,title,notes,priority,done\n");
    for row in rows {
        out.push_str(&format!("{},", row.id.0));
        quote_csv_into(&row.value.title, &mut out);
        out.push(',');
        quote_csv_into(&row.value.notes, &mut out);
        out.push_str(&format!(",{},{}\n", row.value.priority, row.value.done));
    }
    out
}

// ---------------------------------------------------------------------------
// Runner contract: hooks outbox
// ---------------------------------------------------------------------------

/// Runner-side handles (filled once, in-body, with host-bound
/// signals): the close handler vetoes on content drift
/// (`export_tasks_json(all)` vs `saved_snapshot`), raises the
/// save modal through `save_requested`, and writes
/// `export_out`/`export_name` to disk; the modal's Save fills
/// all three and raises `exit_requested`.
#[derive(Clone)]
pub struct StudioHooksInner {
    pub save_requested: Signal<bool>,
    pub exit_requested: Signal<bool>,
    pub export_out: Signal<SharedString>,
    pub export_name: Signal<SharedString>,
    pub saved_snapshot: Signal<SharedString>,
}

/// Opt-in hooks outbox (`None` renders the studio standalone —
/// the shipped example; `Some` wires OS close/export — the E2E
/// runner). Signals cannot cross runtimes, hence publish-once
/// instead of props-in.
#[derive(Clone, Default)]
pub struct StudioHooks {
    inner: Rc<RefCell<Option<StudioHooksInner>>>,
}

impl StudioHooks {
    /// Reads the published handles (`None` before first render).
    pub fn get(&self) -> Option<StudioHooksInner> {
        self.inner.borrow().clone()
    }
}

/// Studio props: the task seed + collection key (the body
/// joins/creates the [`Collection`] on the mounted host's
/// runtime and seeds once when empty — collections are
/// runtime-keyed, so they cannot be built ahead of the host;
/// runners reach the live collection by joining the same key),
/// plus the optional hooks outbox.
#[derive(Clone, Props)]
pub struct StudioProps {
    pub seed: Vec<Task>,
    pub key: u64,
    pub hooks: Option<StudioHooks>,
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Guarded render-time sync (the Scrollbar flash-latch
/// precedent): converges local control signals onto the live
/// row without loops (equal values never write).
fn sync_signal<T: PartialEq + Clone + 'static>(sig: &Signal<T>, live: T) {
    if sig.get() != live {
        sig.set(live);
    }
}

/// Current export JSON over every committed row (commit order —
/// the close handler's drift check and the Save path share it).
fn current_json(tasks: &Collection<Task>) -> String {
    export_tasks_json(&tasks.rows())
}

// ---------------------------------------------------------------------------
// Grid cells
// ---------------------------------------------------------------------------

/// Per-row status toggle (the 23.1 granular showcase): flips
/// `done` through `update_row` (slot notify, zero version
/// fan-out — sibling rows never hear it).
fn task_status_cell(ctx: &Ctx, p: &GridCellProps<Task>) -> VNode {
    let done = p
        .rows
        .get_row(p.row.id)
        .get()
        .as_ref()
        .is_some_and(|t| t.done);
    let (coll, id) = (p.rows.clone(), p.row.id);
    let flip: Action = Rc::new(move || {
        if let Some(mut t) = coll.lookup(id) {
            t.done = !t.done;
            coll.update_row(id, t);
        }
    });
    ctx.child_keyed(
        id.0,
        &ButtonProps {
            label: SharedString::from(if done { "Done" } else { "Todo" }),
            enabled: true,
            width: 76.0,
            height: 28.0,
            debug: SharedString::from("task-done"),
            on_press: flip,
        },
        Button,
    )
}

/// Title cell content (ContextMenu payload): the live title,
/// click-selecting when the grid wires selection, bold + primary
/// while selected.
#[derive(Clone)]
struct TitleContentProps {
    row: Row<Task>,
    selected: Option<Signal<Option<RowId>>>,
}

impl Props for TitleContentProps {}

fn task_title_content(_ctx: &Ctx, p: &TitleContentProps) -> VNode {
    // Handlerless anchor (the ContextMenu contract — the wrapper
    // owns the anchor press; selection flows through the Open row
    // action, never a competing press).
    let is_sel = p
        .selected
        .as_ref()
        .is_some_and(|s| s.get() == Some(p.row.id));
    let mut label = Text::new(p.row.value.title.clone());
    if is_sel {
        label = label.bold();
    }
    Div("task-title").child(VNode::from(label))
}

/// Title cell: live title (subscribed — inspector edits refresh
/// the grid without any version traffic) with Duplicate/Delete
/// row actions on right-click.
fn task_title_cell(ctx: &Ctx, p: &GridCellProps<Task>) -> VNode {
    let live = p.rows.get_row(p.row.id).get();
    let row = Row {
        id: p.row.id,
        value: live.unwrap_or_else(|| p.row.value.clone()),
    };
    let (coll, id) = (p.rows.clone(), row.id);
    let sel_open = p.selected.clone();
    ctx.child_keyed(
        row.id.0,
        &ContextMenuProps {
            items: vec![
                MenuItemProps::new("Open", move || {
                    if let Some(sel) = sel_open.clone() {
                        sel.set(Some(id));
                    }
                }),
                MenuItemProps::new("Duplicate", {
                    let (coll, id) = (coll.clone(), id);
                    move || {
                        if let Some(t) = coll.lookup(id) {
                            let copy = Task {
                                title: SharedString::from(format!("{} (copy)", t.title)),
                                notes: t.notes.clone(),
                                priority: t.priority,
                                done: false,
                            };
                            coll.ingest(vec![copy]);
                        }
                    }
                }),
                MenuItemProps::new("Delete", {
                    let (coll, id) = (coll.clone(), id);
                    move || {
                        coll.remove(id);
                    }
                }),
            ],
            content: task_title_content,
            content_props: TitleContentProps {
                row,
                selected: p.selected.clone(),
            },
            width: 200.0,
        },
        ContextMenu,
    )
}

/// Priority cell: live label text.
fn task_priority_cell(_ctx: &Ctx, p: &GridCellProps<Task>) -> VNode {
    let label = p
        .rows
        .get_row(p.row.id)
        .get()
        .map(|t| priority_label(t.priority))
        .unwrap_or("Low");
    Div("task-priority").child(VNode::from(Text::new(SharedString::from(label))))
}

// ---------------------------------------------------------------------------
// Inspector (keyed per task — fresh control state per selection)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct InspectorProps {
    task_id: RowId,
    tasks: Collection<Task>,
}

impl Props for InspectorProps {}

/// Detail inspector for one task: controlled title/notes (value
/// signals synced from the live row — external granular writes
/// converge, typing flows through `on_change` + `update_row`),
/// priority `Select` (write-through on change), done `Toggle`.
fn inspector(ctx: &Ctx, p: &InspectorProps) -> VNode {
    let live = p.tasks.get_row(p.task_id).get();
    let Some(task) = live else {
        return Div("insp-gone").child(VNode::from(Text::new(SharedString::from("Select a task"))));
    };
    let title_v = ctx.signal(task.title.clone());
    sync_signal(&title_v, task.title.clone());
    let notes_v = ctx.signal(task.notes.clone());
    sync_signal(&notes_v, task.notes.clone());
    let pri_v = ctx.signal(task.priority);
    sync_signal(&pri_v, task.priority);
    let done_v = ctx.signal(task.done);
    sync_signal(&done_v, task.done);
    // Select commit: the control owns `selected`; persist drift
    // here (guarded — converges in one pass, the latch pattern).
    if pri_v.get() != task.priority {
        let mut t = task.clone();
        t.priority = pri_v.get();
        p.tasks.update_row(p.task_id, t);
    }
    let (tasks, id) = (p.tasks.clone(), p.task_id);
    let title_commit: Change<SharedString> = Rc::new(move |v| {
        if let Some(mut t) = tasks.lookup(id) {
            if t.title != v {
                t.title = v;
                tasks.update_row(id, t);
            }
        }
    });
    let (tasks, id) = (p.tasks.clone(), p.task_id);
    let notes_commit: Change<SharedString> = Rc::new(move |v| {
        if let Some(mut t) = tasks.lookup(id) {
            if t.notes != v {
                t.notes = v;
                tasks.update_row(id, t);
            }
        }
    });
    let (tasks, id) = (p.tasks.clone(), p.task_id);
    let done_commit: Change<bool> = Rc::new(move |v| {
        if let Some(mut t) = tasks.lookup(id) {
            if t.done != v {
                t.done = v;
                tasks.update_row(id, t);
            }
        }
    });
    let open_v = ctx.signal(false);
    let title_props = TextInputProps::new("Title", title_v);
    // Phase 39b: empty titles announce invalid (G7 exercised
    // functionally — the error never blocks, it only announces;
    // saving an empty title stays the author's choice, stated).
    let title_empty = task.title.is_empty();
    let title_props = TextInputProps {
        debug: SharedString::from("insp-title"),
        on_change: Some(title_commit),
        invalid: title_empty,
        required: true,
        error_message: if title_empty {
            Some(SharedString::from("Title is required"))
        } else {
            None
        },
        helper_text: None,
        ..title_props
    };
    let notes_props = TextAreaProps::new("Notes", notes_v);
    let notes_props = TextAreaProps {
        debug: SharedString::from("insp-notes"),
        width: 320.0,
        on_change: Some(notes_commit),
        ..notes_props
    };
    let pri_items = vec![
        SelectItem {
            value: 0u8,
            label: SharedString::from("Low"),
        },
        SelectItem {
            value: 1u8,
            label: SharedString::from("Medium"),
        },
        SelectItem {
            value: 2u8,
            label: SharedString::from("High"),
        },
    ];
    Column::new().gap(8).children([
        ctx.child_auto(&title_props, TextInput),
        ctx.child_auto(&notes_props, TextArea),
        ctx.child_auto(&SelectProps::new(pri_items, pri_v, open_v), Select),
        ctx.child_auto(
            &ToggleProps {
                label: SharedString::from("Done"),
                on: done_v,
                enabled: true,
                on_change: Some(done_commit),
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            Toggle,
        ),
    ])
}

// ---------------------------------------------------------------------------
// Studio root
// ---------------------------------------------------------------------------

/// Task Studio root: toolbar (search + sort + theme + export +
/// add), task grid, keyed inspector behind an error boundary,
/// and the dirty-gated save modal.
pub fn TaskStudio(ctx: &Ctx, props: &StudioProps) -> VNode {
    // The live collection (joined on this host's runtime) plus
    // once-only seeding (guarded — converges in one extra pass).
    let tasks = Collection::new(&ctx.host().runtime(), props.key);
    let seeded = ctx.signal(false);
    if !seeded.get() {
        seeded.set(true);
        if tasks.is_empty() {
            tasks.ingest(props.seed.clone());
        }
    }
    // Runner hooks (host-bound, published once).
    let save_requested = ctx.signal(false);
    let exit_requested = ctx.signal(false);
    let export_out = ctx.signal(SharedString::from(""));
    let export_name = ctx.signal(SharedString::from("tasks.json"));
    let saved_snapshot = ctx.signal(SharedString::from(""));
    if let Some(hooks) = &props.hooks {
        let mut slot = hooks.inner.borrow_mut();
        if slot.is_none() {
            *slot = Some(StudioHooksInner {
                save_requested: save_requested.clone(),
                exit_requested: exit_requested.clone(),
                export_out: export_out.clone(),
                export_name: export_name.clone(),
                saved_snapshot: saved_snapshot.clone(),
            });
        }
    }
    let search = ctx.signal(SharedString::from(""));
    let sort_mode = ctx.signal(0u8);
    let selected = ctx.signal(None::<RowId>);
    let export_status = ctx.signal(SharedString::from(""));

    // Toolbar actions (all plain closures over signal clones —
    // payload-less `Action`s, the ADR-0007 rule).
    let add: Action = {
        let (tasks, selected) = (tasks.clone(), selected.clone());
        Rc::new(move || {
            let n = tasks.len() + 1;
            let rows = tasks.ingest(vec![Task {
                title: SharedString::from(format!("New task {n}")),
                notes: SharedString::from(""),
                priority: 1,
                done: false,
            }]);
            if let Some(row) = rows.into_iter().next() {
                selected.set(Some(row.id));
            }
        })
    };
    let theme_toggle: Action = {
        let host = ctx.host();
        Rc::new(move || {
            let next = match host.theme().mode() {
                ThemeMode::Light => ThemeMode::Dark,
                ThemeMode::Dark => ThemeMode::Light,
            };
            host.set_theme(next);
        })
    };
    let export_json: Action = {
        let (tasks, export_out, export_name, export_status) = (
            tasks.clone(),
            export_out.clone(),
            export_name.clone(),
            export_status.clone(),
        );
        Rc::new(move || {
            let json = current_json(&tasks);
            export_status.set(SharedString::from(format!(
                "JSON ready ({} bytes)",
                json.len()
            )));
            export_name.set(SharedString::from("tasks.json"));
            export_out.set(SharedString::from(json));
        })
    };
    let export_csv: Action = {
        let (tasks, export_out, export_name, export_status) = (
            tasks.clone(),
            export_out.clone(),
            export_name.clone(),
            export_status.clone(),
        );
        Rc::new(move || {
            let csv = export_tasks_csv(&tasks.rows());
            export_status.set(SharedString::from(format!(
                "CSV ready ({} bytes)",
                csv.len()
            )));
            export_name.set(SharedString::from("tasks.csv"));
            export_out.set(SharedString::from(csv));
        })
    };
    let dark = ctx.theme().mode() == ThemeMode::Dark;

    // Grid filter/sort (tracked reads — typing + sort buttons
    // re-derive the window; the closures rebuild per render, the
    // M8 window-fn rule).
    let sq = search.clone();
    let sm = sort_mode.clone();
    let columns = vec![
        GridColumn {
            header: SharedString::from("Done"),
            width: 90.0,
            cell: task_status_cell,
        },
        GridColumn {
            header: SharedString::from("Title"),
            width: 330.0,
            cell: task_title_cell,
        },
        GridColumn {
            header: SharedString::from("Priority"),
            width: 110.0,
            cell: task_priority_cell,
        },
    ];
    let grid_props = DataGridProps::new(tasks.clone(), columns, |_: &Task| 36.0)
        .size(560.0, 440.0)
        .debug("studio-grid")
        .scrollbar(true)
        .selected(selected.clone())
        .filter(move |t: &Task| {
            let q = sq.get().to_lowercase();
            q.is_empty()
                || t.title.to_lowercase().contains(q.as_str())
                || t.notes.to_lowercase().contains(q.as_str())
        })
        .sort(move |a: &Task, b: &Task| match sm.get() {
            1 => b.priority.cmp(&a.priority),
            2 => a.done.cmp(&b.done),
            _ => a.title.cmp(&b.title),
        });

    // Save modal (dirty-gated close raises `save_requested`).
    let do_save: Action = {
        let (tasks, export_out, saved_snapshot, save_requested, exit_requested) = (
            tasks.clone(),
            export_out.clone(),
            saved_snapshot.clone(),
            save_requested.clone(),
            exit_requested.clone(),
        );
        Rc::new(move || {
            let json = current_json(&tasks);
            export_out.set(SharedString::from(json.clone()));
            saved_snapshot.set(SharedString::from(json));
            save_requested.set(false);
            exit_requested.set(true);
        })
    };
    let do_discard: Action = {
        let (save_requested, exit_requested) = (save_requested.clone(), exit_requested.clone());
        Rc::new(move || {
            save_requested.set(false);
            exit_requested.set(true);
        })
    };
    let mut save_props = ModalProps::new("Unsaved changes", save_requested.clone());
    save_props.confirm_label = SharedString::from("Save");
    save_props.cancel_label = SharedString::from("Discard");
    save_props.on_confirm = Some(do_save);
    save_props.on_cancel = Some(do_discard);

    // Inspector content lives in `inspector_factory` below (a
    // per-render factory for the Splitter's second pane — keyed per
    // task behind an error boundary, same as before).

    let search_value = search.clone();
    let search_commit: Change<SharedString> = Rc::new(move |v| search_value.set(v));
    let toolbar = Row("studio-toolbar").gap(8).children([
        ctx.child_auto(
            &UncontrolledTextInputProps {
                label: SharedString::from("Search"),
                initial: SharedString::from(""),
                placeholder: Some(SharedString::from("Filter tasks…")),
                enabled: true,
                width: 220.0,
                height: 32.0,
                style: Text::body_secondary,
                debug: SharedString::from("studio-search"),
                on_change: Some(search_commit),
                masked: false,
                invalid: false,
                required: false,
                error_message: None,
                helper_text: None,
            },
            UncontrolledTextInput,
        ),
        sort_button(ctx, &sort_mode, 0, "Title", "studio-sort-title"),
        sort_button(ctx, &sort_mode, 1, "Priority", "studio-sort-priority"),
        sort_button(ctx, &sort_mode, 2, "Status", "studio-sort-status"),
        ctx.child_auto(
            &ButtonProps {
                label: SharedString::from(if dark { "Theme: Dark" } else { "Theme: Light" }),
                enabled: true,
                width: 120.0,
                height: 32.0,
                debug: SharedString::from("studio-theme"),
                on_press: theme_toggle.clone(),
            },
            Button,
        ),
        export_button(
            ctx,
            "Export JSON",
            "studio-export-json",
            "Download tasks as JSON",
            export_json.clone(),
        ),
        export_button(
            ctx,
            "Export CSV",
            "studio-export-csv",
            "Download tasks as CSV",
            export_csv.clone(),
        ),
        ctx.child_auto(
            &ButtonProps {
                label: SharedString::from("+ Add task"),
                enabled: true,
                width: 120.0,
                height: 32.0,
                debug: SharedString::from("studio-add"),
                on_press: add.clone(),
            },
            Button,
        ),
    ]);
    let n = tasks.len();
    let t = ctx.theme().tokens();
    // Status line (Phase 39b, G22): a two-span RichText (bold
    // count + dimmed export note) over a Canvas done-meter (track
    // + done-fraction fill — the retained-surface leaf in a real
    // app, repainting through the same signal reads as the grid).
    let done_n = tasks.rows().iter().filter(|r| r.value.done).count();
    let meter_frac = if n == 0 {
        0.0
    } else {
        done_n as f32 / n as f32
    };
    let status = Div("studio-status").children([
        ctx.child_auto(
            &RichTextViewProps {
                spans: vec![
                    TextSpan::new(format!("{n} tasks")).weight(FontWeight::BOLD),
                    TextSpan::new(format!(" · {}", export_status.get())),
                ],
                style: Text::body_secondary,
                label: None,
                debug: SharedString::from("studio-status-text"),
            },
            RichTextView,
        ),
        ctx.child_auto(
            &CanvasViewProps {
                ops: vec![
                    CanvasOp::Rect {
                        x: Px::of(0.0),
                        y: Px::of(0.0),
                        w: Px::of(120.0),
                        h: Px::of(12.0),
                        color: t.disabled,
                    },
                    CanvasOp::Rect {
                        x: Px::of(0.0),
                        y: Px::of(0.0),
                        w: Px::of(120.0 * meter_frac.clamp(0.0, 1.0)),
                        h: Px::of(12.0),
                        color: t.primary,
                    },
                ],
                width: 120.0,
                height: 12.0,
                label: Some(SharedString::from("Done ratio")),
                debug: SharedString::from("studio-meter"),
            },
            CanvasView,
        ),
    ]);
    // Menubar (Phase 39b, G21): File duplicates the export/add
    // actions, View the sort orders + theme — native duplication
    // (same closures, never a second behavior). Sits UNDER the
    // body: the grid keeps its top-anchored geometry (first data
    // row under the E2E's header cutoff, as before), status follows.
    let open_title = ctx.signal(None::<usize>);
    let file_items = |export_json: &Action, export_csv: &Action, add: &Action| {
        vec![
            MenuItemProps::new("Export JSON", {
                let run = export_json.clone();
                move || run()
            }),
            MenuItemProps::new("Export CSV", {
                let run = export_csv.clone();
                move || run()
            }),
            MenuItemProps::new("+ Add task", {
                let run = add.clone();
                move || run()
            }),
        ]
    };
    let view_items = |sort_mode: &Signal<u8>, theme_toggle: &Action| {
        vec![
            MenuItemProps::new("Sort: Title", {
                let mode = sort_mode.clone();
                move || mode.set(0)
            }),
            MenuItemProps::new("Sort: Priority", {
                let mode = sort_mode.clone();
                move || mode.set(1)
            }),
            MenuItemProps::new("Sort: Status", {
                let mode = sort_mode.clone();
                move || mode.set(2)
            }),
            MenuItemProps::new("Toggle theme", {
                let run = theme_toggle.clone();
                move || run()
            }),
        ]
    };
    let menubar = ctx.child_auto(
        &MenubarProps::new(
            vec![
                MenuTitle::new("File", file_items(&export_json, &export_csv, &add)),
                MenuTitle::new("View", view_items(&sort_mode, &theme_toggle)),
            ],
            open_title,
        ),
        Menubar,
    );
    // Splitter body (Phase 39b, G13): grid | inspector proportions
    // are author-owned now (the gap's motivation — the fixed
    // 560/380 ratio is gone, same debugs inside). Panes are
    // factories rebuilding per render (move-only VNodes cannot be
    // shared across renders); once-built props ride `Rc`.
    let split = ctx.signal(0.6f32);
    let grid_shared: Rc<DataGridProps<Task>> = Rc::new(grid_props);
    let inspector_factory: Rc<dyn Fn(&Ctx) -> VNode> = {
        let (tasks, selected) = (tasks.clone(), selected.clone());
        Rc::new(move |inspector_ctx: &Ctx| match selected.get() {
            Some(id) => {
                let (tasks, task_id) = (tasks.clone(), id);
                let child = move |c: &Ctx| {
                    c.child(
                        "oppa::TaskInspector",
                        task_id.0,
                        &InspectorProps {
                            task_id,
                            tasks: tasks.clone(),
                        },
                        inspector,
                    )
                };
                inspector_ctx.child_auto(&ErrorBoundaryProps::new(child), ErrorBoundary)
            }
            None => Div("insp-empty").child(VNode::from(Text::new(SharedString::from(
                "Select a task to inspect",
            )))),
        })
    };
    let body = ctx.child_auto(
        &SplitterProps {
            fraction: split,
            axis: SplitterAxis::Vertical,
            width: 960.0,
            height: 440.0,
            divider_px: 8.0,
            min_first_px: 300.0,
            min_second_px: 200.0,
            enabled: true,
            debug: SharedString::from("studio-split"),
            first: {
                let grid_shared = grid_shared.clone();
                Rc::new(move |pane_ctx: &Ctx| pane_ctx.child_auto(&*grid_shared, DataGrid))
            },
            second: {
                let inspector_factory = inspector_factory.clone();
                Rc::new(move |pane_ctx: &Ctx| {
                    Div("studio-inspector")
                        .style(Style::new().w(380.0))
                        .child(inspector_factory(pane_ctx))
                })
            },
            on_change: None,
        },
        Splitter,
    );
    Div("studio").children([
        toolbar,
        body,
        menubar,
        status,
        ctx.child_auto(&save_props, Modal),
    ])
}

/// Sort toggle button (`✓` marks the active order).
fn sort_button(ctx: &Ctx, mode: &Signal<u8>, value: u8, label: &str, debug: &str) -> VNode {
    let active = mode.get() == value;
    let (mode, value) = (mode.clone(), value);
    let press: Action = Rc::new(move || mode.set(value));
    ctx.child_keyed(
        value as u64,
        &ButtonProps {
            label: SharedString::from(if active {
                format!("✓ {label}")
            } else {
                label.to_string()
            }),
            enabled: true,
            width: 100.0,
            height: 32.0,
            debug: SharedString::from(debug),
            on_press: press,
        },
        Button,
    )
}

/// Export button with a tooltip hint (the content fn closes over
/// props-carried signals — the cell-template pattern, no
/// captures).
#[derive(Clone)]
struct ExportContentProps {
    label: SharedString,
    debug: SharedString,
    on_export: Action,
}

impl Props for ExportContentProps {}

fn export_content(ctx: &Ctx, p: &ExportContentProps) -> VNode {
    ctx.child_auto(
        &ButtonProps {
            label: p.label.clone(),
            enabled: true,
            width: 110.0,
            height: 32.0,
            debug: p.debug.clone(),
            on_press: p.on_export.clone(),
        },
        Button,
    )
}

fn export_button(ctx: &Ctx, label: &str, debug: &str, tip: &str, on_export: Action) -> VNode {
    ctx.child(
        "oppa::StudioExport",
        if debug.contains("json") { 6 } else { 11 },
        &TooltipProps::new(
            tip,
            export_content,
            ExportContentProps {
                label: SharedString::from(label),
                debug: SharedString::from(debug),
                on_export,
            },
        ),
        Tooltip,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_of(tasks: Vec<Task>) -> Vec<Row<Task>> {
        tasks
            .into_iter()
            .enumerate()
            .map(|(i, value)| Row {
                id: RowId(i as u64),
                value,
            })
            .collect()
    }

    #[test]
    fn json_escapes_and_shapes() {
        let rows = rows_of(vec![Task {
            title: SharedString::from("Say \"hi\"\nnow"),
            notes: SharedString::from("back\\slash"),
            priority: 2,
            done: false,
        }]);
        assert_eq!(
            export_tasks_json(&rows),
            "{\"tasks\":[{\"id\":0,\"title\":\"Say \\\"hi\\\"\\nnow\",\"notes\":\"back\\\\slash\",\"priority\":2,\"done\":false}]}"
        );
        assert_eq!(export_tasks_json(&[]), "{\"tasks\":[]}");
    }

    #[test]
    fn csv_quotes_only_when_needed() {
        let rows = rows_of(vec![
            Task {
                title: SharedString::from("plain"),
                notes: SharedString::from("has, comma"),
                priority: 0,
                done: true,
            },
            Task {
                title: SharedString::from("quote \"me\""),
                notes: SharedString::from("line\nbreak"),
                priority: 1,
                done: false,
            },
        ]);
        assert_eq!(
            export_tasks_csv(&rows),
            "id,title,notes,priority,done\n0,plain,\"has, comma\",0,true\n1,\"quote \"\"me\"\"\",\"line\nbreak\",1,false\n"
        );
    }

    #[test]
    fn priority_labels_cover_bytes() {
        assert_eq!(priority_label(0), "Low");
        assert_eq!(priority_label(1), "Medium");
        assert_eq!(priority_label(2), "High");
        assert_eq!(priority_label(9), "Low");
    }

    /// Phase 39b (decision 380): the upgraded studio mounts the
    /// Splitter body (grid + inspector behind one divider), the
    /// Menubar (File/View titles opening a Menu popup), the
    /// RichText status, and the Canvas done-meter — same E2E
    /// debugs inside (`studio-add`, `studio-search`, `insp-title`,
    /// `task-title`, `modal-confirm` all preserved). Settles
    /// bounded (full-studio headless quiescence is NOT asserted:
    /// the tree keeps frame demand without input even unmodified
    /// (probe-verified pre-existing — DesktopLoop pumps it in
    /// production/E2E, so this test drives bounded frames and
    /// asserts structure, never `run_until_idle`).
    #[test]
    fn studio_mounts_splitter_menubar_and_meter() {
        use oppa::{find_retained_by_debug, ComponentHost, InputEvent};
        let host = ComponentHost::new();
        host.mount(
            "Studio",
            StudioProps {
                seed: sample_tasks(),
                key: oppa::fetch_key("test:studio-39b"),
                hooks: None,
            },
            TaskStudio,
        );
        for _ in 0..50 {
            host.run_once();
        }
        for debug in [
            "studio-split-divider",
            "menubar-bar",
            "studio-meter",
            "studio-grid",
            "studio-add",
            "studio-search",
        ] {
            assert_eq!(
                find_retained_by_debug(&host, debug).len(),
                1,
                "{debug} mounts"
            );
        }
        // File title opens a Menu popup (standalone dropdown).
        let title = find_retained_by_debug(&host, "menubar-title-0")[0];
        let b = host.committed_box(title).expect("title hit box");
        host.inject_input(InputEvent::pointer_down(b.x + b.w / 2.0, b.y + b.h / 2.0));
        host.inject_input(InputEvent::pointer_up(b.x + b.w / 2.0, b.y + b.h / 2.0));
        for _ in 0..50 {
            host.run_once();
        }
        assert_eq!(
            find_retained_by_debug(&host, "menu-popup").len(),
            1,
            "File opens its menu"
        );
    }
}
