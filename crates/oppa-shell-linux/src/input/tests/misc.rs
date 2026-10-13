use super::*;

/// Round 2.3 (decision 258): the shell lends its file dialog
/// through the seam (Win32 parity — backends probe lazily, so
/// construction stays infallible and headless).
#[test]
fn shell_file_dialog_seam_lends_some() {
    use oppa::shell::PlatformShell;
    let mut shell = shell();
    assert!(
        shell.file_dialog().is_some(),
        "LinuxShell lends its dialog (OQ-G12-1 closed)"
    );
}

/// Round 2.4 (decision 259): scale factors classify to
/// runner-matched density commands (the `ScaleFactorChanged`
/// event itself is unconstructible headless — the writer — so
/// the event table below the translate wall is what is proven).
#[test]
fn density_classifies_and_counts() {
    let mut shell = shell();
    shell.push_event(LinuxEvent::Density { scale: 2.0 });
    let (cmds, errors) = pump(&mut shell);
    assert!(errors.is_empty());
    assert_eq!(cmds, vec![LinuxCmd::Density { scale: 2.0 }]);
    assert_eq!(shell.stats.density, 1);
}

#[test]
fn density_rebases_and_refuses_garbage() {
    let mut shell = shell();
    shell.set_density(2.0);
    assert_eq!(shell.density(), 2.0);
}

#[test]
#[should_panic(expected = "refused")]
fn zero_density_refuses_loudly() {
    shell().set_density(0.0);
}

#[test]
#[should_panic(expected = "refused")]
fn nan_density_refuses_loudly() {
    shell().set_density(f32::NAN);
}

#[test]
#[should_panic(expected = "must match Density first")]
fn density_refuses_pipeline_mapping_loudly() {
    LinuxCmd::Density { scale: 2.0 }.to_input_event(Modifiers::NONE);
}
