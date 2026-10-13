//! Intake tests: shell/pump rigs plus per-area suites.

use super::*;

fn shell() -> LinuxShell {
    LinuxShell::new(ShellConfig {
        title: "t".to_string(),
        density: 2.0,
        width_dp: 400.0,
        height_dp: 300.0,
    })
}

fn pump(shell: &mut LinuxShell) -> (Vec<LinuxCmd>, Vec<LinuxShellError>) {
    use oppa::shell::PlatformShell;
    let _ = shell.pump_events();
    (shell.take_cmds(), shell.take_errors())
}

mod ime;
mod keys;
mod misc;
mod pointer;
mod wheel;
