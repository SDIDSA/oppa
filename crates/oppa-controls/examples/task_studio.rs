//! Task Studio reference app (Round 24.1, decision 335; live OS
//! integration Round 26.2, decision 342): a real window running the
//! studio screen -- task grid, search/sort, inspector, context
//! actions, tooltips, theme toggle, export buttons -- with the full
//! close/export loop owned through `run_desktop_with`: a dirty-gated
//! veto raises the save modal, and the poll hook writes the modal's
//! export through the native save dialog, then asks for close.
//!
//! Run with `cargo run -p oppa-controls --example task_studio`.

use std::rc::Rc;

use oppa::{Collection, FileDialogOptions, FileFilter};
use oppa_app::{run_desktop_with, WindowOptions};
use oppa_controls::studio::{
    export_tasks_json, sample_tasks, StudioHooks, StudioProps, Task, TaskStudio,
};

fn main() {
    let options = WindowOptions::new("Task Studio", 1100, 750);
    let hooks = StudioHooks::default();
    let props = StudioProps {
        seed: sample_tasks(),
        key: oppa::fetch_key("task-studio"),
        hooks: Some(hooks.clone()),
    };
    if let Err(e) = run_desktop_with(options, props, TaskStudio, |loop_| {
        // Mount already settled, so the hooks outbox is published and
        // the collection is joinable (the E2E twin asserts the same).
        let coll: Collection<Task> =
            Collection::new(&loop_.host().runtime(), oppa::fetch_key("task-studio"));
        // Dirty-gated veto: drift from the last saved snapshot raises
        // the save modal and refuses; clean closes proceed.
        let veto_hooks = hooks.clone();
        loop_.set_close_handler(Rc::new(move || {
            let Some(h) = veto_hooks.get() else {
                return true;
            };
            if export_tasks_json(&coll.rows()) != h.saved_snapshot.get().to_string() {
                h.save_requested.set(true);
                false
            } else {
                true
            }
        }));
        // Poll writer: the modal's Save fills export_out/export_name
        // and raises exit_requested (Discard raises it with empty
        // export). Write through the native save dialog, consume the
        // request exactly once, then ask for close -- the veto sees
        // the fresh snapshot state on re-consult.
        let poll_hooks = hooks.clone();
        loop_.set_poll_hook(Some(Box::new(move |loop_| {
            let Some(h) = poll_hooks.get() else {
                return;
            };
            if !h.exit_requested.get() {
                return;
            }
            h.exit_requested.set(false);
            if !h.export_out.get().is_empty() {
                let name = h.export_name.get().to_string();
                let options = FileDialogOptions {
                    title: "Save tasks".to_string(),
                    filters: vec![FileFilter {
                        name: "JSON".to_string(),
                        patterns: vec!["*.json".to_string()],
                    }],
                    default_name: name,
                    initial_dir: None,
                };
                if let Some(path) = loop_.save_file_dialog(options) {
                    if let Err(e) = std::fs::write(&path, h.export_out.get().to_string()) {
                        eprintln!("task_studio: save failed: {e}");
                        return;
                    }
                    h.saved_snapshot.set(h.export_out.get().clone());
                } else {
                    // Dialog cancelled: stay open with the drift intact
                    // (the next close re-raises the modal).
                    return;
                }
            }
            loop_.host().request_close();
        })));
    }) {
        eprintln!("task_studio: {e}");
        std::process::exit(1);
    }
}
