//! Round 24.1 (decision 335): Task Studio end-to-end. One
//! headless [`DesktopLoop`](oppa_app::DesktopLoop) drives the full
//! workflow — create task → search/sort → edit title + multi-line
//! notes → right-click Duplicate/Delete → close attempt with
//! unsaved changes → modal veto → save via the mock dialog → file
//! write — asserting CPU paint, Vello retention, and DOM parity
//! along the way with 0 errors.

use std::rc::Rc;

use oppa::{
    fetch_key, find_retained_by_debug,
    input::{keys, KeyState, Modifiers, PointerAction, PointerButton},
    Collection, InputEvent,
};
use oppa_app::DesktopLoop;
use oppa_controls::studio::{
    export_tasks_json, sample_tasks, StudioHooks, StudioProps, Task, TaskStudio,
};

// ---------------------------------------------------------------------------
// Fakes (the oppa-app FakeText shape: 8.75px/char at body size)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeText;

impl oppa::TextService for FakeText {
    fn enumerate_fonts(&self) -> Vec<oppa::FontInfo> {
        Vec::new()
    }

    fn shape(
        &self,
        text: &str,
        style: &oppa::TextStyle,
    ) -> Result<oppa::ShapedRun, oppa::TextError> {
        if text.is_empty() {
            return Err(oppa::TextError::EmptyText);
        }
        let em = style.font_size_px * style.device_pixel_ratio;
        let adv = em * 0.625;
        let metrics = oppa::FontMetrics {
            ascent: em * 0.75,
            descent: em * 0.25,
            line_gap: em * 0.125,
        };
        let mut glyphs = Vec::new();
        let mut clusters = Vec::new();
        for (k, (i, ch)) in text.char_indices().enumerate() {
            let len = ch.len_utf8();
            glyphs.push(oppa::ShapedGlyph {
                glyph_id: k as u32,
                x_advance: adv,
                x_offset: 0.0,
                y_offset: 0.0,
            });
            clusters.push(oppa::Cluster {
                byte_range: (i, i + len),
                glyph_range: (k, k + 1),
            });
        }
        Ok(oppa::ShapedRun {
            glyphs,
            runs: vec![oppa::TextRun {
                byte_range: (0, text.len()),
                glyph_range: (0, clusters.len()),
                rtl: false,
                script: 0,
                font_id: oppa::FontId(0),
                font_metrics: metrics,
            }],
            clusters,
            total_advance: adv * text.chars().count() as f32,
            text_len_bytes: text.len(),
        })
    }
}

// ---------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------

const STUDIO_KEY_SRC: &str = "task-studio-e2e";

struct Rig {
    loop_: DesktopLoop,
    hooks: StudioHooks,
    coll: Collection<Task>,
}

fn rig() -> Rig {
    let mut loop_ =
        DesktopLoop::new(1100, 750, Box::new(FakeText), "Studio").expect("headless loop builds");
    let hooks = StudioHooks::default();
    loop_.mount(
        "Studio",
        StudioProps {
            seed: sample_tasks(),
            key: fetch_key(STUDIO_KEY_SRC),
            hooks: Some(hooks.clone()),
        },
        TaskStudio,
    );
    loop_.host().run_until_idle();
    loop_.repaint().expect("first paint works");
    let coll: Collection<Task> =
        Collection::new(&loop_.host().runtime(), fetch_key(STUDIO_KEY_SRC));
    assert_eq!(coll.len(), 5, "seeded once");
    // Runner close contract (content-defined dirtiness): veto +
    // raise the save modal while the export drifts from the last
    // saved snapshot.
    let (hooks_c, coll_c) = (hooks.clone(), coll.clone());
    loop_.set_close_handler(Rc::new(move || {
        let Some(h) = hooks_c.get() else {
            return true;
        };
        if export_tasks_json(&coll_c.rows()) != h.saved_snapshot.get().to_string() {
            h.save_requested.set(true);
            false
        } else {
            true
        }
    }));
    Rig { loop_, hooks, coll }
}

fn tap(rig: &mut Rig, debug: &str) {
    let id = find_retained_by_debug(rig.loop_.host(), debug)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("e2e: no node {debug:?}"));
    let b = rig.loop_.host().committed_box(id).expect("laid out");
    rig.loop_
        .step(InputEvent::pointer_down(b.x + b.w / 2.0, b.y + b.h / 2.0))
        .expect("down steps");
    rig.loop_
        .step(InputEvent::pointer_up(b.x + b.w / 2.0, b.y + b.h / 2.0))
        .expect("up steps");
}

/// Visible title rows, top to bottom (retained order is NOT
/// visual — sort by committed y; skip the header band in case a
/// stale box lingers under the pinned header).
fn visual_titles(rig: &Rig) -> Vec<oppa::NodeId> {
    let mut rows: Vec<(f32, oppa::NodeId)> = find_retained_by_debug(rig.loop_.host(), "task-title")
        .into_iter()
        .filter_map(|id| {
            let b = rig.loop_.host().committed_box(id)?;
            (b.y >= 64.0).then_some((b.y, id))
        })
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    rows.into_iter().map(|(_, id)| id).collect()
}

fn row_point(rig: &Rig, id: oppa::NodeId) -> (f32, f32) {
    // Raw committed centers hit reliably (proven by probe: the
    // hit-test consumes these boxes as-is).
    let b = rig.loop_.host().committed_box(id).expect("laid out");
    (b.x + b.w / 2.0, b.y + b.h / 2.0)
}

fn row_rclick(rig: &mut Rig, id: oppa::NodeId) {
    let (x, y) = row_point(rig, id);
    let ptr = |action| InputEvent::Pointer {
        id: Some(0),
        action,
        x,
        y,
        modifiers: Modifiers::NONE,
    };
    rig.loop_
        .step(ptr(PointerAction::Down {
            button: PointerButton::Secondary,
        }))
        .expect("rdown steps");
    rig.loop_
        .step(ptr(PointerAction::Up {
            button: PointerButton::Secondary,
        }))
        .expect("rup steps");
}

fn key(rig: &mut Rig, code: u32, shift: bool, ctrl: bool) {
    rig.loop_
        .step(InputEvent::Key {
            code,
            modifiers: Modifiers {
                shift,
                ctrl,
                alt: false,
                meta: false,
            },
            state: KeyState::Pressed,
            repeat: false,
        })
        .expect("key steps");
}

fn count(rig: &Rig, debug: &str) -> usize {
    find_retained_by_debug(rig.loop_.host(), debug).len()
}

/// The headless workflow, one leg per backend family.
#[test]
fn task_studio_end_to_end() {
    let mut rig = rig();

    // -- create -------------------------------------------------------
    tap(&mut rig, "studio-add");
    assert_eq!(rig.coll.len(), 6, "Add ingests a task");
    assert!(
        rig.coll
            .rows()
            .iter()
            .any(|r| r.value.title.as_ref() == "New task 6"),
        "numbered title"
    );

    // -- search -------------------------------------------------------
    tap(&mut rig, "studio-search");
    rig.loop_.type_text("coffee").expect("types");
    rig.loop_.host().run_until_idle();
    assert_eq!(count(&rig, "task-title"), 1, "filter narrows to coffee");
    // Clear via the 22.1 shortcut + the runner backspace seam
    // (decision 243 — shells route Backspace through
    // `DesktopLoop::backspace`, the same call Win32 makes).
    key(&mut rig, keys::A, false, true);
    rig.loop_.backspace().expect("backspaces");
    rig.loop_.host().run_until_idle();
    assert_eq!(count(&rig, "task-title"), 6, "clear restores all rows");

    // -- sort ---------------------------------------------------------
    tap(&mut rig, "studio-sort-priority");
    rig.loop_.host().run_until_idle();

    // -- select + edit title & notes ----------------------------------
    // (visual order, not retained — see visual_titles).
    let first = visual_titles(&rig)
        .into_iter()
        .next()
        .expect("a visible row");
    row_rclick(&mut rig, first);
    key(&mut rig, keys::ENTER, false, false); // Open (highlight 0)
    assert!(
        count(&rig, "insp-title") == 1,
        "inspector opens for the task"
    );
    tap(&mut rig, "insp-title");
    key(&mut rig, keys::A, false, true); // select all
    rig.loop_.type_text("Renamed").expect("types");
    tap(&mut rig, "insp-notes");
    rig.loop_.type_text("extra").expect("types");
    rig.loop_
        .step(InputEvent::key(keys::ENTER, KeyState::Pressed))
        .expect("newline steps");
    rig.loop_.type_text("lines").expect("types");
    rig.loop_.host().run_until_idle();
    let edited: Vec<Task> = rig
        .coll
        .rows()
        .into_iter()
        .map(|r| r.value)
        .filter(|t| t.title.as_ref() == "Renamed")
        .collect();
    assert_eq!(edited.len(), 1, "title edit lands in the collection");
    assert!(
        edited[0].notes.as_ref().contains('\n'),
        "multi-line notes land too: {:?}",
        edited[0].notes
    );

    // -- context duplicate + delete -----------------------------------
    // (visual order — see visual_titles).
    let before = rig.coll.len();
    let first = visual_titles(&rig)
        .into_iter()
        .next()
        .expect("a visible row");
    row_rclick(&mut rig, first);
    key(&mut rig, keys::DOWN, false, false); // Duplicate
    key(&mut rig, keys::ENTER, false, false);
    rig.loop_.host().run_until_idle();
    assert_eq!(rig.coll.len(), before + 1, "Duplicate ingests a copy");
    assert!(
        rig.coll
            .rows()
            .iter()
            .any(|r| r.value.title.as_ref().ends_with("(copy)")),
        "copy labeled"
    );
    // Delete the last visible row (never the just-opened first —
    // the inspector keeps its task and textarea).
    let last = visual_titles(&rig).into_iter().next_back().expect("rows");
    row_rclick(&mut rig, last);
    key(&mut rig, keys::DOWN, false, false);
    key(&mut rig, keys::DOWN, false, false); // Delete
    key(&mut rig, keys::ENTER, false, false);
    rig.loop_.host().run_until_idle();
    assert_eq!(rig.coll.len(), before, "Delete removes one row");

    // -- CPU leg -------------------------------------------------------
    let damage = rig.loop_.repaint().expect("repaints");
    assert!(damage > 0, "content paints");
    assert_eq!(
        rig.loop_.rgba8().expect("pixels").len(),
        1100 * 750 * 4,
        "full-viewport pixmap"
    );

    // -- Vello leg ------------------------------------------------------
    // Headless honesty: GPU paint needs a device + injected faces
    // (the 7.4 suite proves paint on text-free scenes); what runs
    // here is twin acceptance — every studio diff commits into a
    // fresh Vello backend with 0 errors (the loop's own twin
    // commits the same way on every repaint above).
    {
        use oppa::RendererBackend;
        let mut twin = oppa_vello::VelloBackend::new();
        let mut n = 0usize;
        for d in rig.loop_.host().diffs_from(0) {
            twin.commit(&d).expect("vello accepts studio diffs");
            n += 1;
        }
        assert!(n > 0, "scene committed to the Vello twin");
    }

    // -- close veto + save modal ---------------------------------------
    assert!(!rig.loop_.close_requested(), "unsaved changes veto");
    rig.loop_.host().run_until_idle();
    assert_eq!(count(&rig, "modal-card"), 1, "save modal mounts");
    tap(&mut rig, "modal-confirm"); // Save
    let h = rig.hooks.get().expect("hooks published");
    assert!(h.exit_requested.get(), "save requests exit");
    assert!(!h.export_out.get().is_empty(), "export filled");
    assert_eq!(
        h.saved_snapshot.get().to_string(),
        h.export_out.get().to_string(),
        "snapshot converges"
    );
    assert!(rig.loop_.close_requested(), "clean close proceeds");

    // -- mock-dialog file write -----------------------------------------
    let dir = std::env::temp_dir().join(format!(
        "oppa-studio-e2e-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("tempdir");
    let path = dir.join("tasks.json");
    let mut dialog = oppa::ScriptedSaveDialog::new();
    dialog.push_response(Some(path.clone()));
    rig.loop_.set_save_dialog(Box::new(dialog));
    let picked = rig
        .loop_
        .save_file_dialog(oppa::FileDialogOptions {
            title: "Export tasks".to_string(),
            default_name: h.export_name.get().to_string(),
            ..Default::default()
        })
        .expect("dialog settles");
    assert_eq!(picked, path);
    std::fs::write(&path, h.export_out.get().to_string()).expect("writes");
    let back = std::fs::read_to_string(&path).expect("reads");
    assert!(
        back.contains("Renamed") && back.contains("\"tasks\":["),
        "file holds the export: {back:.120}"
    );
    std::fs::remove_dir_all(&dir).ok();

    // -- DOM leg ----------------------------------------------------------
    let mut dom = oppa_dom::DomBackend::new(1.0);
    let mut sheet = oppa_dom::StyleSheet::new(1.0);
    {
        use oppa::RendererBackend;
        for d in rig.loop_.host().diffs_from(0) {
            dom.commit(&d).expect("commit");
        }
    }
    rig.loop_
        .host()
        .with_retained_mut(|rec, styles| dom.sync(rec, styles, &mut sheet))
        .expect("sync");
    let page = oppa_dom::render_page("t", &dom, &sheet);
    assert!(page.contains("<textarea"), "notes area emits");
    assert!(page.contains("Renamed"), "edited title emits");
    // Priority sort: a High task precedes the Low coffee task.
    let hi = page.find("Ship the release notes").expect("high emits");
    let lo = page.find("Buy office coffee").expect("low emits");
    assert!(hi < lo, "DOM order follows the priority sort");
}
