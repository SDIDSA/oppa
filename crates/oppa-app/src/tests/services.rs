use super::*;

/// A save backend that always fails (backend-error grace shape).
struct FailingSave;
impl oppa::SaveFileDialog for FailingSave {
    fn save(&mut self, _: oppa::FileDialogOptions) -> Result<Option<PathBuf>, oppa::PickError> {
        Err(oppa::PickError::Backend("boom".to_string()))
    }
}

/// Round 16.1 (decision 314): save prompts return the scripted
/// destination in order (each call reaches the backend — never
/// a cached answer); dismissal and missing backends settle
/// `None` without panic, and backend errors degrade loudly to
/// `None` instead of killing the app.
#[test]
fn save_file_dialog_returns_pick_and_graceful_none() {
    use oppa::{FileDialogOptions, FileFilter, ScriptedSaveDialog};
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    // No backend installed: graceful None (headless/web rule).
    assert_eq!(loop_.save_file_dialog(FileDialogOptions::default()), None);
    let mut scripted = ScriptedSaveDialog::new();
    scripted.push_response(Some(PathBuf::from("/tmp/a.txt")));
    scripted.push_response(Some(PathBuf::from("/tmp/b.txt")));
    loop_.set_save_dialog(Box::new(scripted));
    let options = FileDialogOptions {
        title: "Save".to_string(),
        filters: vec![FileFilter {
            name: "Text".to_string(),
            patterns: vec!["*.txt".to_string()],
        }],
        default_name: "a.txt".to_string(),
        initial_dir: Some(PathBuf::from("/tmp")),
    };
    assert_eq!(
        loop_.save_file_dialog(options),
        Some(PathBuf::from("/tmp/a.txt"))
    );
    assert_eq!(
        loop_.save_file_dialog(FileDialogOptions::default()),
        Some(PathBuf::from("/tmp/b.txt")),
        "second call reaches the backend too"
    );
    // Exhausted script = dismissed.
    assert_eq!(loop_.save_file_dialog(FileDialogOptions::default()), None);
    // Backend failure degrades to None (loud on stderr, no panic).
    loop_.set_save_dialog(Box::new(FailingSave));
    assert_eq!(loop_.save_file_dialog(FileDialogOptions::default()), None);
}

/// Round 16.1 (decision 314): folder prompts return the scripted
/// directory; dismissal and missing backends settle `None`
/// without panic.
#[test]
fn pick_folder_dialog_returns_dir_and_graceful_none() {
    use oppa::{FolderDialogOptions, ScriptedFolderDialog};
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    assert_eq!(
        loop_.pick_folder_dialog(FolderDialogOptions::default()),
        None
    );
    let mut scripted = ScriptedFolderDialog::new();
    scripted.push_response(Some(PathBuf::from("/tmp/out")));
    loop_.set_folder_dialog(Box::new(scripted));
    assert_eq!(
        loop_.pick_folder_dialog(FolderDialogOptions {
            title: "Pick".to_string(),
            initial_dir: Some(PathBuf::from("/tmp")),
        }),
        Some(PathBuf::from("/tmp/out"))
    );
    assert_eq!(
        loop_.pick_folder_dialog(FolderDialogOptions::default()),
        None
    );
}

/// Round 16.2 (decision 315): a scripted OS reading flips the
/// reactive theme in place — `ctx.theme().mode()` follows, the
/// themed fill recolors in the committed plan, and redundant
/// readings stay quiet (no repaint spin).
#[test]
fn sync_system_theme_forwards_os_mode_and_recolors() {
    use oppa::{ScriptedThemeSource, ThemeMode, ThemeTokens};
    #[derive(Clone)]
    struct ThemeProps;
    impl Props for ThemeProps {}
    fn theme_app(ctx: &Ctx, _: &ThemeProps) -> VNode {
        Div("themed")
            .style(Style::new().size(200, 150).bg(ctx.theme().tokens().surface))
            .build()
    }
    let mut loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    loop_.mount("Theme", ThemeProps, theme_app);
    // No source installed: quiet no-op (headless default).
    assert!(!loop_.sync_system_theme().expect("syncs"));
    assert_eq!(loop_.host().theme().mode(), ThemeMode::Light);
    let mut source = ScriptedThemeSource::new();
    source.push_reading(ThemeMode::Dark);
    loop_.set_theme_source(Box::new(source));
    assert!(
        loop_.sync_system_theme().expect("syncs"),
        "dark reading applies"
    );
    assert_eq!(loop_.host().theme().mode(), ThemeMode::Dark);
    let plan = loop_.last_plan().expect("sync repaints");
    let dark = ThemeTokens::dark().surface;
    assert!(
        plan.ops.iter().any(|op| matches!(op,
                oppa::DrawOp::Rect { color, .. } if *color == dark)),
        "themed fill recolors in place"
    );
    // Exhausted script repeats Dark: same mode stays quiet.
    assert!(!loop_.sync_system_theme().expect("syncs"));
    assert_eq!(loop_.host().theme().mode(), ThemeMode::Dark);
}

/// Round 16.3 (decision 316): window-chrome calls forward into
/// the installed control in order, stay quiet with none, and
/// the close veto defaults to closing while a refusing handler
/// suppresses.
#[test]
fn window_chrome_forwards_and_close_veto_decides() {
    use oppa::{ScriptedWindowControl, WindowCall, WindowIcon};
    use std::rc::Rc;
    let loop_ =
        DesktopLoop::new(200, 150, Box::new(FakeText), "Test").expect("headless loop builds");
    // No control installed: every call is a quiet no-op.
    loop_.set_title("Hi");
    loop_.set_min_size(Some((100, 80)));
    loop_.set_max_size(Some((400, 300)));
    loop_.set_fullscreen(true);
    loop_.set_icon(None);
    // No handler installed: closes (pre-16.3 behavior).
    assert!(loop_.close_requested(), "default closes");
    // The scripted double shares its record across clones: the
    // loop owns one behind the trait object, the test reads
    // through its own.
    let control = ScriptedWindowControl::new();
    let mut loop_ = loop_;
    loop_.set_window_control(Box::new(control.clone()));
    loop_.set_title("Hi");
    loop_.set_min_size(Some((100, 80)));
    loop_.set_max_size(None);
    loop_.set_fullscreen(true);
    loop_.set_fullscreen(false);
    let icon = WindowIcon::new(vec![9; 4 * 4 * 4], 4, 4).expect("valid icon builds");
    loop_.set_icon(Some(icon.clone()));
    loop_.set_icon(None);
    assert_eq!(
        control.calls(),
        vec![
            WindowCall::SetTitle("Hi".to_string()),
            WindowCall::SetMinSize(Some((100, 80))),
            WindowCall::SetMaxSize(None),
            WindowCall::SetFullscreen(true),
            WindowCall::SetFullscreen(false),
            WindowCall::SetIcon(Some(icon)),
            WindowCall::SetIcon(None),
        ]
    );
    // Veto: refusing handler suppresses, approving closes.
    loop_.set_close_handler(Rc::new(|| false));
    assert!(!loop_.close_requested(), "refusal suppresses exit");
    loop_.set_close_handler(Rc::new(|| true));
    assert!(loop_.close_requested(), "approval closes");
}

/// Round 21.1 (decision 328): the loop fires due component
/// timers through the wake query + tick path — the timeout flips
/// the signal the body reads (fire → settle → repaint), the
/// re-render re-arms per-render, and quiet ticks settle nothing.
#[test]
fn loop_tick_timers_fires_settles_and_repaints() {
    #[derive(Clone)]
    struct TimerCase {
        fired: Signal<bool>,
    }
    impl Props for TimerCase {}
    fn timer_app(ctx: &Ctx, props: &TimerCase) -> VNode {
        let f = props.fired.clone();
        let f2 = props.fired.clone();
        ctx.use_timeout(100.0, move || {
            f2.set(true);
        });
        Div("root").child(
            Div("box")
                .style(Style::new().size(if f.get() { 10.0 } else { 20.0 }, 10.0))
                .build(),
        )
    }
    let clock = Rc::new(oppa::MockClock::new());
    let mut loop_ = DesktopLoop::with_clock(200, 150, Box::new(FakeText), "Test", clock.clone())
        .expect("headless loop builds");
    let fired = loop_.host().runtime().signal(false);
    loop_.mount(
        "Timer",
        TimerCase {
            fired: fired.clone(),
        },
        timer_app,
    );
    loop_.repaint().expect("paints the base");
    assert_eq!(
        loop_.next_timer_due_ms(),
        Some(100.0),
        "armed timeout names its horizon"
    );
    clock.advance(0.05);
    assert_eq!(loop_.tick_timers().expect("ticks"), 0, "quiet before due");
    assert!(!fired.get());
    clock.advance(0.05);
    assert_eq!(loop_.tick_timers().expect("ticks"), 1, "fires at due");
    assert!(fired.get(), "callback flipped the signal");
    // The fire re-rendered (tracked read) and re-armed
    // per-render: the horizon moved exactly one period out.
    assert_eq!(
        loop_.next_timer_due_ms(),
        Some(200.0),
        "re-render re-arms per-render"
    );
    assert_eq!(
        loop_.tick_timers().expect("ticks"),
        0,
        "no immediate refire"
    );
}
