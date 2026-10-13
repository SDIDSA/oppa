use super::*;
use oppa::input::{keys, InputEvent, KeyState};
use oppa::{find_retained_by_debug, ComponentHost, SharedString, Signal};

// ------------------------------------------------------------------
// Phase 38c (decisions 371–372): Toolbar, Menubar, FilePicker, G22
// ------------------------------------------------------------------

fn toolbar_items(fired: &[Signal<bool>]) -> Vec<BarItem> {
    vec![
        BarItem::new("New", {
            let f = fired[0].clone();
            move || f.set(true)
        }),
        BarItem::separator(),
        BarItem::new("Save", {
            let f = fired[1].clone();
            move || f.set(true)
        })
        .disabled(),
        BarItem::new("Open", {
            let f = fired[2].clone();
            move || f.set(true)
        }),
    ]
}

/// G21: taps hit-test cells, arrows rove past separators/disabled,
/// Enter invokes the highlight; cells announce buttons.
#[test]
fn toolbar_taps_rove_and_enter_activates() {
    let host = ComponentHost::new();
    let rt = host.runtime();
    let fired: Vec<Signal<bool>> = (0..3).map(|_| rt.signal(false)).collect();
    host.mount(
        "TB",
        ToolbarProps {
            items: toolbar_items(&fired),
            ..ToolbarProps::new(vec![])
        },
        Toolbar,
    );
    host.run_until_idle();
    // Tap the first cell: invokes + roving follows the pointer.
    press_node(&host, node_by_debug(&host, "toolbar-cell-0"));
    assert!(fired[0].get(), "tap invokes the enabled cell");
    // Right roves past the separator (1) and disabled Save (2)
    // to Open (3); Enter invokes it through the router pulse.
    let bar = node_by_debug(&host, "toolbar");
    press_node(&host, bar);
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert!(fired[2].get(), "Enter invokes the roved highlight");
    // Left wraps back past disabled/separator to New.
    host.inject_input(InputEvent::key(keys::LEFT, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert!(fired[0].get());
    // Cells announce buttons; separators carry no semantics.
    let sem = host
        .retained_semantics(node_by_debug(&host, "toolbar-cell-0"))
        .expect("semantics");
    assert_eq!(sem.role, oppa::Role::Button);
    assert_eq!(sem.label.as_deref(), Some("New"));
    assert!(
        host.retained_semantics(node_by_debug(&host, "toolbar-cell-1"))
            .is_none(),
        "separators announce nothing"
    );
    // Disabled cells keep their press owner out of the tab order.
    assert!(
        !host
            .retained_handlers(node_by_debug(&host, "toolbar"))
            .is_empty(),
        "the bar container owns press + arrows (one tab stop)"
    );
}

fn menubar_titles(fired: &[Signal<bool>]) -> Vec<MenuTitle> {
    vec![
        MenuTitle::new(
            "File",
            vec![
                MenuItemProps::new("New", {
                    let f = fired[0].clone();
                    move || f.set(true)
                }),
                MenuItemProps::new("Open", {
                    let f = fired[1].clone();
                    move || f.set(true)
                }),
            ],
        ),
        MenuTitle::new(
            "Edit",
            vec![MenuItemProps::new("Undo", {
                let f = fired[2].clone();
                move || f.set(true)
            })],
        ),
    ]
}

fn mount_menubar() -> (ComponentHost, Vec<Signal<bool>>, Signal<Option<usize>>) {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let rt = host.runtime();
    let fired: Vec<Signal<bool>> = (0..3).map(|_| rt.signal(false)).collect();
    let open_title = rt.signal(None::<usize>);
    let props = MenubarProps {
        titles: menubar_titles(&fired),
        open_title: open_title.clone(),
        ..MenubarProps::new(vec![], open_title.clone())
    };
    host.mount("MB", props, Menubar);
    host.run_until_idle();
    (host, fired, open_title)
}

/// G21: title taps toggle the dropdown; Down opens; Tab + Enter
/// invokes the highlighted row and closes; Right moves the open
/// menu; Escape closes through the blur rule.
#[test]
fn menubar_opens_invokes_and_escapes() {
    let (host, fired, open_title) = mount_menubar();
    assert!(find_retained_by_debug(&host, "menu-popup").is_empty());
    // Title tap toggles the dropdown (attached to the bar focus).
    press_node(&host, node_by_debug(&host, "menubar-title-0"));
    assert_eq!(open_title.get(), Some(0));
    host.run_until_idle();
    assert_eq!(find_retained_by_debug(&host, "menu-popup").len(), 1);
    // Same-title tap closes.
    press_node(&host, node_by_debug(&host, "menubar-title-0"));
    assert_eq!(open_title.get(), None);
    assert!(find_retained_by_debug(&host, "menu-popup").is_empty());
    // Keyboard: Tab focuses the bar, Down opens title 0.
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    let bar = node_by_debug(&host, "menubar-bar");
    assert_eq!(host.focused_node(), Some(bar));
    host.inject_input(InputEvent::key(keys::DOWN, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(open_title.get(), Some(0));
    // Right moves the open menu with the highlight (native).
    host.inject_input(InputEvent::key(keys::RIGHT, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(open_title.get(), Some(1));
    // Tab into the list, Enter invokes Undo and closes.
    host.inject_input(InputEvent::key(keys::TAB, KeyState::Pressed));
    host.run_until_idle();
    host.inject_input(InputEvent::key(keys::ENTER, KeyState::Pressed));
    host.run_until_idle();
    assert!(fired[2].get(), "Enter invokes the highlighted row");
    assert_eq!(open_title.get(), None, "invoke closes the menu");
    // Escape closes an open menu through the blur rule.
    press_node(&host, node_by_debug(&host, "menubar-title-1"));
    assert_eq!(open_title.get(), Some(1));
    host.inject_input(InputEvent::key(keys::ESCAPE, KeyState::Pressed));
    host.run_until_idle();
    assert_eq!(open_title.get(), None, "Escape dismisses");
}

/// G21: browse wraps request/poll per mode (pick sets, dismissal
/// leaves the path, failures surface in the caption).
#[test]
fn file_picker_browse_sets_and_dismissal_keeps() {
    // Open mode.
    let host = ComponentHost::new();
    let path = host.runtime().signal(None::<SharedString>);
    let open_dlg = std::rc::Rc::new(std::cell::RefCell::new(oppa::ScriptedDialog::new()));
    open_dlg
        .borrow_mut()
        .push_response(vec![std::path::PathBuf::from("a.txt")]);
    open_dlg.borrow_mut().push_response(vec![]);
    let mut fp = FilePickerProps::new(path.clone(), FilePickerMode::Open);
    fp.open_dialog = Some(open_dlg);
    fp.debug = SharedString::from("fp-open");
    host.mount("FP", fp, FilePicker);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "fp-open-browse"));
    assert_eq!(path.get().as_deref(), Some("a.txt"));
    press_node(&host, node_by_debug(&host, "fp-open-browse"));
    assert_eq!(
        path.get().as_deref(),
        Some("a.txt"),
        "dismissal leaves the path untouched"
    );
    // Save mode.
    let host2 = ComponentHost::new();
    let path2 = host2.runtime().signal(None::<SharedString>);
    let save_dlg = std::rc::Rc::new(std::cell::RefCell::new(oppa::ScriptedSaveDialog::new()));
    save_dlg
        .borrow_mut()
        .push_response(Some(std::path::PathBuf::from("out.txt")));
    save_dlg.borrow_mut().push_response(None);
    let mut fp2 = FilePickerProps::new(path2.clone(), FilePickerMode::Save);
    fp2.save_dialog = Some(save_dlg);
    fp2.debug = SharedString::from("fp-save");
    host2.mount("FPS", fp2, FilePicker);
    host2.run_until_idle();
    press_node(&host2, node_by_debug(&host2, "fp-save-browse"));
    assert_eq!(path2.get().as_deref(), Some("out.txt"));
    press_node(&host2, node_by_debug(&host2, "fp-save-browse"));
    assert_eq!(path2.get().as_deref(), Some("out.txt"));
    // Folder mode.
    let host3 = ComponentHost::new();
    let path3 = host3.runtime().signal(None::<SharedString>);
    let folder_dlg = std::rc::Rc::new(std::cell::RefCell::new(oppa::ScriptedFolderDialog::new()));
    folder_dlg
        .borrow_mut()
        .push_response(Some(std::path::PathBuf::from("docs")));
    let mut fp3 = FilePickerProps::new(path3.clone(), FilePickerMode::Folder);
    fp3.folder_dialog = Some(folder_dlg);
    fp3.debug = SharedString::from("fp-folder");
    host3.mount("FPF", fp3, FilePicker);
    host3.run_until_idle();
    press_node(&host3, node_by_debug(&host3, "fp-folder-browse"));
    assert_eq!(path3.get().as_deref(), Some("docs"));
}

/// G21: a missing backend for the chosen mode refuses loudly
/// (wiring bug, never a silent button).
#[test]
#[should_panic(expected = "without an open backend")]
fn file_picker_missing_backend_refuses_loudly() {
    let host = ComponentHost::new();
    let path = host.runtime().signal(None::<SharedString>);
    let mut fp = FilePickerProps::new(path, FilePickerMode::Open);
    fp.debug = SharedString::from("fp-loud");
    host.mount("FPL", fp, FilePicker);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "fp-loud-browse"));
}

/// TEMPORARY box probe (delete after): dumps row/column/text
/// geometry for the two-line path display.
#[test]
fn tmp_file_picker_box_probe() {
    let host = ComponentHost::new();
    host.set_text_service(Box::new(FakeText));
    let path = host.runtime().signal(None::<SharedString>);
    let open_dlg = std::rc::Rc::new(std::cell::RefCell::new(oppa::ScriptedDialog::new()));
    open_dlg
        .borrow_mut()
        .push_response(vec![std::path::PathBuf::from(
            "/data/user/0/com.oppa.app/files/picked/tapme.txt",
        )]);
    let mut fp = FilePickerProps::new(path.clone(), FilePickerMode::Open);
    fp.open_dialog = Some(open_dlg);
    fp.debug = SharedString::from("fp-probe");
    host.mount("FPP", fp, FilePicker);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "fp-probe-browse"));
    for dbg in ["fp-probe-row", "column", "text"] {
        for id in find_retained_by_debug(&host, dbg) {
            let b = host.committed_box(id);
            let t = host.with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.text.clone()));
            let lines = b.as_ref().map(|bb| bb.lines.len()).unwrap_or(999);
            eprintln!(
                "PROBE {dbg} {id:?} xywh={:?} lines={lines} textlen={:?}",
                b.map(|bb| (bb.x, bb.y, bb.w, bb.h)),
                t.as_ref().map(|s| s.len()),
            );
        }
    }
    // Long-dir wrap check: 400-char directory must wrap, never one line.
    let host2 = ComponentHost::new();
    host2.set_text_service(Box::new(FakeText));
    let long_dir = format!("/{}", "d".repeat(400));
    let long_full = format!("{long_dir}/tapme.txt");
    let path2 = host2.runtime().signal(None::<SharedString>);
    let open2 = std::rc::Rc::new(std::cell::RefCell::new(oppa::ScriptedDialog::new()));
    open2
        .borrow_mut()
        .push_response(vec![std::path::PathBuf::from(&long_full)]);
    let mut fp2 = FilePickerProps::new(path2.clone(), FilePickerMode::Open);
    fp2.open_dialog = Some(open2);
    fp2.debug = SharedString::from("fp-probe2");
    host2.mount("FPP2", fp2, FilePicker);
    host2.run_until_idle();
    press_node(&host2, node_by_debug(&host2, "fp-probe2-browse"));
    for id in find_retained_by_debug(&host2, "text") {
        let b = host2.committed_box(id);
        let t = host2.with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.text.clone()));
        if let Some(s) = t {
            if s.len() > 100 {
                eprintln!(
                    "PROBE2 longtext len={} lines={:?} boxw={:?}",
                    s.len(),
                    b.as_ref().map(|bb| bb.lines.len()),
                    b.map(|bb| bb.w),
                );
            }
        }
    }
}

/// Decision 430: the picked path renders filename-first two-line
/// (name + wrapping directory, never one overflowing absolute-path
/// line); bare names and `(none)` stay one line.
#[test]
fn file_picker_renders_name_plus_wrapping_dir() {
    fn leaf_texts(host: &ComponentHost) -> Vec<String> {
        find_retained_by_debug(host, "text")
            .into_iter()
            .filter_map(|id| {
                host.with_retained_mut(|rec, _| rec.get(id).and_then(|n| n.text.clone()))
            })
            .map(|s| s.to_string())
            .collect()
    }
    let full = "/data/user/0/com.oppa.app/files/picked/tapme.txt";
    let host = ComponentHost::new();
    let path = host.runtime().signal(None::<SharedString>);
    let open_dlg = std::rc::Rc::new(std::cell::RefCell::new(oppa::ScriptedDialog::new()));
    open_dlg
        .borrow_mut()
        .push_response(vec![std::path::PathBuf::from(full)]);
    let mut fp = FilePickerProps::new(path.clone(), FilePickerMode::Open);
    fp.open_dialog = Some(open_dlg);
    fp.debug = SharedString::from("fp-wrap");
    host.mount("FPW", fp, FilePicker);
    host.run_until_idle();
    press_node(&host, node_by_debug(&host, "fp-wrap-browse"));
    let texts = leaf_texts(&host);
    assert!(
        texts.iter().any(|t| t == "tapme.txt"),
        "name line renders, got {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|t| t == "/data/user/0/com.oppa.app/files/picked"),
        "directory line renders, got {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t == full),
        "no single overflowing absolute-path line, got {texts:?}"
    );
}
