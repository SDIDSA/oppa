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

#[test]
fn studio_demand_sources_instrumentation() {
    use crate::tests::FakeText;
    use oppa::ComponentHost;

    // Bounded-frame harness (Decision 383) — never run_until_idle on full TaskStudio.
    let host = ComponentHost::new();
    host.mount(
        "Studio",
        StudioProps {
            seed: sample_tasks(),
            key: oppa::fetch_key("test:studio-instrument"),
            hooks: None,
        },
        TaskStudio,
    );
    let mut ran_count = 0;
    for _ in 0..5 {
        if host.run_once() {
            ran_count += 1;
        }
    }
    assert!(ran_count <= 2, "bare studio settles in <= 2 frames");
    assert!(!host.has_demand(), "bare studio achieves quiescence");
    assert!(host.demand_sources().is_empty());
    assert!(host.pending_labels().is_empty());

    // Host with FakeText service:
    let host_text = ComponentHost::new();
    host_text.set_text_service(Box::new(FakeText));
    host_text.mount(
        "Studio",
        StudioProps {
            seed: sample_tasks(),
            key: oppa::fetch_key("test:studio-instrument-text"),
            hooks: None,
        },
        TaskStudio,
    );
    let mut ran_text_count = 0;
    for _ in 0..5 {
        if host_text.run_once() {
            ran_text_count += 1;
        }
    }
    assert!(
        ran_text_count <= 2,
        "studio with text service settles in <= 2 frames"
    );
    assert!(
        !host_text.has_demand(),
        "studio with text service achieves quiescence"
    );
    assert!(host_text.demand_sources().is_empty());
    assert!(host_text.pending_labels().is_empty());
}
