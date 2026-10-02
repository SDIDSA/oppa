use super::*;
use oppa::FileFilter;
use std::cell::RefCell;
use std::rc::Rc;

/// Recorded spawns (program + argv) shared with assertions.
type Calls = Rc<RefCell<Vec<(String, Vec<String>)>>>;

/// Scripted process spawner (records argv, plays canned
/// outcomes): the zenity surface without a zenity.
struct StubRunner {
    calls: Calls,
    script: RefCell<Vec<Option<ChildOutcome>>>,
    fail_spawn: bool,
    kills: Rc<std::cell::Cell<usize>>,
}

struct StubChild {
    outcome: Option<ChildOutcome>,
    killed: Rc<RefCell<bool>>,
    kills: Rc<std::cell::Cell<usize>>,
}

impl CommandChild for StubChild {
    fn try_wait(&mut self) -> Result<Option<ChildOutcome>, PickError> {
        Ok(self.outcome.clone())
    }

    fn kill(&mut self) {
        *self.killed.borrow_mut() = true;
        self.kills.set(self.kills.get() + 1);
    }
}

impl CommandRunner for StubRunner {
    type Child = StubChild;

    fn spawn(&mut self, program: &str, args: &[String]) -> Result<Self::Child, PickError> {
        self.calls
            .borrow_mut()
            .push((program.to_string(), args.to_vec()));
        if self.fail_spawn {
            return Err(PickError::Backend("stub spawn failed".to_string()));
        }
        let killed = Rc::new(RefCell::new(false));
        let outcome = self.script.borrow_mut().pop().flatten();
        Ok(StubChild {
            outcome,
            killed,
            kills: self.kills.clone(),
        })
    }
}

fn stub() -> (
    LinuxFileDialog<StubRunner>,
    Calls,
    Rc<std::cell::Cell<usize>>,
) {
    let calls: Calls = Rc::new(RefCell::new(Vec::new()));
    let kills = Rc::new(std::cell::Cell::new(0));
    let runner = StubRunner {
        calls: calls.clone(),
        script: RefCell::new(vec![None]),
        fail_spawn: false,
        kills: kills.clone(),
    };
    // Force the zenity backend without probing the real box
    // (probe caching is itself asserted below — here the argv
    // and state paths are the question).
    let mut dialog = LinuxFileDialog::new(runner);
    dialog.probe = Some(Some(Backend::Zenity));
    (dialog, calls, kills)
}

fn options() -> FilePickerOptions {
    FilePickerOptions {
        title: "Pick".to_string(),
        filters: vec![FileFilter {
            name: "PNG images".to_string(),
            patterns: vec!["*.png".to_string(), "*.PNG".to_string()],
        }],
        multiple: true,
        initial_dir: Some(PathBuf::from("/tmp")),
    }
}

#[test]
fn zenity_argv_covers_single_multi_and_dir() {
    let (mut dialog, calls, _) = stub();
    dialog.request_open_files(options());
    let (_, argv) = calls.borrow().last().cloned().expect("spawned");
    assert!(argv.contains(&"--file-selection".to_string()), "{argv:?}");
    assert!(argv.contains(&"--multiple".to_string()), "{argv:?}");
    assert!(
        argv.iter().any(|a| a == "--separator=\n"),
        "newline separator: {argv:?}"
    );
    assert!(argv.iter().any(|a| a == "--title=Pick"), "{argv:?}");
    assert!(argv.iter().any(|a| a == "--filename=/tmp/"), "{argv:?}");
    assert!(
        argv.iter()
            .any(|a| a == "--file-filter=PNG images | *.png *.PNG"),
        "filter shape: {argv:?}"
    );
    // Directory mode: --directory, single, no filters/multiple.
    let (mut dialog, calls, _) = stub();
    dialog.request_open_dir(FilePickerOptions {
        title: String::new(),
        ..Default::default()
    });
    let (_, argv) = calls.borrow().last().cloned().expect("spawned");
    assert!(argv.contains(&"--directory".to_string()), "{argv:?}");
    assert!(!argv.iter().any(|a| a == "--multiple"), "{argv:?}");
    assert!(
        !argv.iter().any(|a| a.starts_with("--file-filter")),
        "{argv:?}"
    );
    assert!(
        !argv.iter().any(|a| a.starts_with("--title")),
        "empty title omitted: {argv:?}"
    );
}

#[test]
fn zenity_output_settles_paths_dismissal_and_errors() {
    // Multi pick, trailing newline dropped.
    let out = ChildOutcome {
        code: 0,
        stdout: b"/a/b.png\n/c/d.png\n".to_vec(),
        stderr: Vec::new(),
    };
    assert_eq!(
        settle_zenity(&out).expect("parses"),
        vec![PathBuf::from("/a/b.png"), PathBuf::from("/c/d.png")]
    );
    // Dismissal (Cancel / window-X) is data, not failure.
    let dismissed = ChildOutcome {
        code: 1,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    assert_eq!(
        settle_zenity(&dismissed).expect("dismisses"),
        Vec::<PathBuf>::new()
    );
    // Anything else carries stderr loudly.
    let failed = ChildOutcome {
        code: 2,
        stdout: Vec::new(),
        stderr: b"Gtk: cannot open display".to_vec(),
    };
    let err = settle_zenity(&failed).expect_err("fails loudly");
    assert!(err.to_string().contains("cannot open display"), "{err}");
}

#[test]
fn request_poll_tracks_open_and_level_triggered_last() {
    let (mut dialog, _, _) = stub();
    // The stub child reports None (still open) until fed.
    dialog.request_open_files(options());
    assert_eq!(dialog.poll_open(), None, "open dialog pends");
    assert_eq!(dialog.poll_open(), None, "still pends");
    // A settled child settles, then re-polls level-triggered.
    let settled = ChildOutcome {
        code: 0,
        stdout: b"/picked/a.txt\n".to_vec(),
        stderr: Vec::new(),
    };
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut dialog = LinuxFileDialog::new(StubRunner {
        calls,
        script: RefCell::new(vec![Some(settled)]),
        fail_spawn: false,
        kills: Rc::new(std::cell::Cell::new(0)),
    });
    dialog.probe = Some(Some(Backend::Zenity));
    dialog.request_open_files(options());
    let first = dialog.poll_open();
    assert_eq!(
        first,
        Some(Ok(vec![PathBuf::from("/picked/a.txt")])),
        "settled paths"
    );
    assert_eq!(dialog.poll_open(), first, "level-triggered repeat");
    // Supersede kills the stale child and spawns anew.
    let (mut dialog, calls, kills) = stub();
    dialog.request_open_files(options());
    dialog.request_open_files(options());
    assert_eq!(calls.borrow().len(), 2, "two spawns");
    assert_eq!(kills.get(), 1, "stale child killed, no orphan dialog");
    assert_eq!(dialog.poll_open(), None);
}

#[test]
fn probe_without_backends_refuses_loudly() {
    // Neither a bus nor a binary on this box shape (the probe
    // runs for real — on a Linux desktop with either present it
    // would select it; here both are absent by construction of
    // the stub runner... except a real bus may exist! So: force
    // the probe through a failing runner and assert the refusal
    // shape, plus assert the real probe never panics).
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut dialog = LinuxFileDialog::new(StubRunner {
        calls,
        script: RefCell::new(Vec::new()),
        fail_spawn: true,
        kills: Rc::new(std::cell::Cell::new(0)),
    });
    // No bus on this box to fake deterministically — instead
    // drive the Unsupported arm directly through a cached probe.
    dialog.probe = Some(None);
    dialog.request_open_files(options());
    assert_eq!(
        dialog.poll_open(),
        Some(Err(PickError::Unsupported("LinuxFileDialog"))),
        "no backend refuses loudly"
    );
    // And the real probe runs without panicking either way.
    let mut dialog2 = LinuxFileDialog::new(StubRunner {
        calls: Rc::new(RefCell::new(Vec::new())),
        script: RefCell::new(Vec::new()),
        fail_spawn: true,
        kills: Rc::new(std::cell::Cell::new(0)),
    });
    let _ = dialog2.backend();
}

#[test]
fn uri_paths_decode_or_refuse() {
    assert_eq!(
        uri_to_path("file:///tmp/a%20b.txt").expect("decodes"),
        PathBuf::from("/tmp/a b.txt")
    );
    assert_eq!(
        uri_to_path("file://localhost/x").expect("localhost"),
        PathBuf::from("/x")
    );
    assert!(uri_to_path("http://h/x").is_err(), "non-file refuses");
    assert!(
        uri_to_path("file://remotehost/x").is_err(),
        "remote host never becomes local"
    );
    assert!(
        uri_to_path("file:///tmp/%zz").is_err(),
        "bad escape refuses"
    );
    assert!(uri_to_path("file://").is_err(), "empty path refuses");
    assert_eq!(
        percent_decode("a+b").expect("plus literal"),
        "a+b",
        "+ stays literal in paths"
    );
}

#[test]
fn portal_options_carry_mode_filters_and_folder() {
    let token = "oppa1_2";
    let dict = portal_options(&options(), false, token);
    let get = |key: &str| {
        dict.iter()
            .find(|(k, _)| k == key)
            .unwrap_or_else(|| panic!("key {key}"))
            .1
            .clone()
    };
    assert_eq!(
        get("handle_token"),
        DVal::Variant(Box::new(DVal::Str(token.to_string())))
    );
    assert_eq!(get("multiple"), DVal::Variant(Box::new(DVal::Bool(true))));
    assert_eq!(get("directory"), DVal::Variant(Box::new(DVal::Bool(false))));
    // Filters marshal through the real encoder (spec shape
    // `a(sa(us))` — glob type 0).
    let DVal::Variant(filters) = get("filters") else {
        panic!("filters is a variant");
    };
    let sig = dbus_variant_sig(&filters).expect("filters sign");
    assert_eq!(sig, "a(sa(us))", "spec shape, got {sig}");
    // Directory mode flips both flags, drops filters.
    let dir = portal_options(&FilePickerOptions::default(), true, token);
    let getd = |key: &str| {
        dir.iter()
            .find(|(k, _)| k == key)
            .unwrap_or_else(|| panic!("key {key}"))
            .1
            .clone()
    };
    assert_eq!(getd("directory"), DVal::Variant(Box::new(DVal::Bool(true))));
    assert_eq!(getd("multiple"), DVal::Variant(Box::new(DVal::Bool(false))));
    assert!(
        dir.iter().all(|(k, _)| k != "filters"),
        "no filters for dirs"
    );
}

/// Test-only signature probe (mirrors the encoder's derivation
/// so the spec-shape assertion above stays honest).
fn dbus_variant_sig(v: &DVal) -> Result<String, String> {
    fn sig_of(v: &DVal) -> Result<String, String> {
        match v {
            DVal::Bool(_) => Ok("b".into()),
            DVal::U32(_) => Ok("u".into()),
            DVal::Str(_) => Ok("s".into()),
            DVal::Path(_) => Ok("o".into()),
            DVal::Sig(_) => Ok("g".into()),
            DVal::Bytes(_) => Ok("ay".into()),
            DVal::Array(items) => {
                let f = items.first().ok_or("empty")?;
                Ok(format!("a{}", sig_of(f)?))
            }
            DVal::Struct(items) => {
                let mut s = String::from("(");
                for i in items {
                    s.push_str(&sig_of(i)?);
                }
                s.push(')');
                Ok(s)
            }
            DVal::Variant(inner) => sig_of(inner),
        }
    }
    sig_of(v)
}

#[test]
fn settle_response_maps_codes() {
    // Success with uris.
    let ok = settle_response(
        0,
        vec![(
            "uris".to_string(),
            DVal::Variant(Box::new(DVal::Array(vec![DVal::Str(
                "file:///a.txt".to_string(),
            )]))),
        )],
    )
    .expect("settles");
    assert_eq!(ok, vec![PathBuf::from("/a.txt")]);
    // Dismissal is data.
    assert_eq!(
        settle_response(1, vec![]).expect("dismisses"),
        Vec::<PathBuf>::new()
    );
    // Anything else is loud.
    assert!(settle_response(2, vec![]).is_err(), "code 2 refuses");
    // Non-string uris refuse (never laundered).
    assert!(
        settle_response(
            0,
            vec![(
                "uris".to_string(),
                DVal::Variant(Box::new(DVal::Array(vec![DVal::U32(1)])))
            )],
        )
        .is_err(),
        "non-string uri refuses"
    );
}

/// Round 16.1 (decision 314): the portal `SaveFile` dict carries
/// the token, the default name, and glob filters — and none of
/// the open-only flags; the method literal pins per kind.
#[test]
fn portal_save_options_carry_name_filters_and_method() {
    use oppa::FileDialogOptions;
    let options = FileDialogOptions {
        title: "Save".to_string(),
        filters: vec![FileFilter {
            name: "Text".to_string(),
            patterns: vec!["*.txt".to_string()],
        }],
        default_name: "report.txt".to_string(),
        initial_dir: Some(PathBuf::from("/tmp")),
    };
    let dict = portal_save_options(&options, "oppa9_9");
    let get = |key: &str| {
        dict.iter()
            .find(|(k, _)| k == key)
            .unwrap_or_else(|| panic!("key {key}"))
            .1
            .clone()
    };
    assert_eq!(
        get("handle_token"),
        DVal::Variant(Box::new(DVal::Str("oppa9_9".to_string())))
    );
    assert_eq!(
        get("current_name"),
        DVal::Variant(Box::new(DVal::Str("report.txt".to_string())))
    );
    let DVal::Variant(filters) = get("filters") else {
        panic!("filters is a variant");
    };
    assert_eq!(
        dbus_variant_sig(&filters).expect("filters sign"),
        "a(sa(us))",
        "spec glob shape"
    );
    assert!(
        dict.iter()
            .all(|(k, _)| k != "multiple" && k != "directory"),
        "save has no open-only flags: {dict:?}"
    );
    assert_eq!(chooser_method(true), "SaveFile");
    assert_eq!(chooser_method(false), "OpenFile");
}

/// Round 16.1: save results collapse to one path (empty
/// dismisses, several refuse — never a silent first-wins on
/// user data).
#[test]
fn single_result_collapses_or_refuses() {
    assert_eq!(single_result(vec![]).expect("dismisses"), None);
    assert_eq!(
        single_result(vec![PathBuf::from("/a")]).expect("picks"),
        Some(PathBuf::from("/a"))
    );
    assert!(
        single_result(vec![PathBuf::from("/a"), PathBuf::from("/b")]).is_err(),
        "never first-wins"
    );
}

/// Round 16.1: the zenity `--save` argv names the suggested
/// destination (dir + default name, no trailing slash — a file,
/// not a folder), carries filters, and never multi/directory
/// flags.
#[test]
fn zenity_save_argv_names_destination_and_filters() {
    use oppa::FileDialogOptions;
    let options = FileDialogOptions {
        title: "Save".to_string(),
        filters: vec![FileFilter {
            name: "Text".to_string(),
            patterns: vec!["*.txt".to_string()],
        }],
        default_name: "report.txt".to_string(),
        initial_dir: Some(PathBuf::from("/tmp")),
    };
    let argv = zenity_save_argv(&options);
    assert!(argv.contains(&"--save".to_string()), "{argv:?}");
    assert!(
        argv.iter().any(|a| a == "--filename=/tmp/report.txt"),
        "dir + default name: {argv:?}"
    );
    assert!(
        argv.iter().any(|a| a == "--file-filter=Text | *.txt"),
        "filters: {argv:?}"
    );
    assert!(
        !argv.iter().any(|a| a == "--multiple" || a == "--directory"),
        "single destination: {argv:?}"
    );
}

/// Round 16.1: blocking save settles the scripted destination
/// (argv proves filters + defaults reached the backend) and
/// dismissal settles `None` without panic.
#[test]
fn blocking_save_settles_argv_and_dismissal() {
    use oppa::{FileDialogOptions, SaveFileDialog};
    let settled = ChildOutcome {
        code: 0,
        stdout: b"/tmp/report.txt\n".to_vec(),
        stderr: Vec::new(),
    };
    let calls: Calls = Rc::new(RefCell::new(Vec::new()));
    let mut dialog = LinuxFileDialog::new(StubRunner {
        calls: calls.clone(),
        script: RefCell::new(vec![Some(settled)]),
        fail_spawn: false,
        kills: Rc::new(std::cell::Cell::new(0)),
    });
    dialog.probe = Some(Some(Backend::Zenity));
    let options = FileDialogOptions {
        title: "Save".to_string(),
        filters: vec![FileFilter {
            name: "Text".to_string(),
            patterns: vec!["*.txt".to_string()],
        }],
        default_name: "report.txt".to_string(),
        initial_dir: Some(PathBuf::from("/tmp")),
    };
    assert_eq!(
        dialog.save(options).expect("saves"),
        Some(PathBuf::from("/tmp/report.txt"))
    );
    let (_, argv) = calls.borrow().last().cloned().expect("spawned");
    assert!(argv.contains(&"--save".to_string()), "{argv:?}");
    assert!(
        argv.iter().any(|a| a == "--filename=/tmp/report.txt"),
        "defaults reach zenity: {argv:?}"
    );
    assert!(
        argv.iter().any(|a| a == "--file-filter=Text | *.txt"),
        "filters reach zenity: {argv:?}"
    );
    // Dismissal (exit 1) settles None, never panics.
    let dismissed = ChildOutcome {
        code: 1,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let mut dialog = LinuxFileDialog::new(StubRunner {
        calls: Rc::new(RefCell::new(Vec::new())),
        script: RefCell::new(vec![Some(dismissed)]),
        fail_spawn: false,
        kills: Rc::new(std::cell::Cell::new(0)),
    });
    dialog.probe = Some(Some(Backend::Zenity));
    assert_eq!(
        dialog
            .save(FileDialogOptions::default())
            .expect("dismisses"),
        None
    );
}

/// Round 16.1: blocking folder pick spawns `--directory`,
/// settles one path, and dismissal settles `None`.
#[test]
fn blocking_folder_settles_single_path_and_dismissal() {
    use oppa::{FolderDialog, FolderDialogOptions};
    let settled = ChildOutcome {
        code: 0,
        stdout: b"/tmp/out\n".to_vec(),
        stderr: Vec::new(),
    };
    let calls: Calls = Rc::new(RefCell::new(Vec::new()));
    let mut dialog = LinuxFileDialog::new(StubRunner {
        calls: calls.clone(),
        script: RefCell::new(vec![Some(settled)]),
        fail_spawn: false,
        kills: Rc::new(std::cell::Cell::new(0)),
    });
    dialog.probe = Some(Some(Backend::Zenity));
    let options = FolderDialogOptions {
        title: "Pick".to_string(),
        initial_dir: Some(PathBuf::from("/tmp")),
    };
    assert_eq!(
        dialog.pick(options).expect("picks"),
        Some(PathBuf::from("/tmp/out"))
    );
    let (_, argv) = calls.borrow().last().cloned().expect("spawned");
    assert!(argv.contains(&"--directory".to_string()), "{argv:?}");
    assert!(
        argv.iter().any(|a| a == "--filename=/tmp/"),
        "start dir reaches zenity: {argv:?}"
    );
    let dismissed = ChildOutcome {
        code: 1,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let mut dialog = LinuxFileDialog::new(StubRunner {
        calls: Rc::new(RefCell::new(Vec::new())),
        script: RefCell::new(vec![Some(dismissed)]),
        fail_spawn: false,
        kills: Rc::new(std::cell::Cell::new(0)),
    });
    dialog.probe = Some(Some(Backend::Zenity));
    assert_eq!(
        dialog
            .pick(FolderDialogOptions::default())
            .expect("dismisses"),
        None
    );
}
