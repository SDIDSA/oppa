use std::path::PathBuf;

use oppa::{FileDialogOptions, FilePickerOptions, PickError};

// ---------------------------------------------------------------------------
// Process seam (zenity + probing)
// ---------------------------------------------------------------------------

/// One spawned helper process (zenity): non-blocking exit poll
/// plus output capture. Killed on supersede (see
/// [`CommandChild::kill`]).
pub trait CommandChild {
    /// Non-blocking poll: `None` = still running; `Some` = reaped
    /// with exit code + captured stdout/stderr.
    fn try_wait(&mut self) -> Result<Option<ChildOutcome>, PickError>;
    /// Best-effort kill (supersede path — errors are advisory, the
    /// old dialog going away is what matters, and the next poll
    /// reaps whatever is left).
    fn kill(&mut self);
}

/// A reaped helper process.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildOutcome {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Spawns helper processes (`zenity`, probes). The production impl
/// is [`StdRunner`]; tests script canned outcomes (same argv
/// parsing + state machine either way).
pub trait CommandRunner {
    type Child: CommandChild;
    fn spawn(&mut self, program: &str, args: &[String]) -> Result<Self::Child, PickError>;
}

/// [`CommandRunner`] over `std::process` (production).
pub struct StdRunner;

/// One spawned helper process, as returned by [`StdRunner`]
/// (opaque handle — drive it through [`CommandChild`]).
pub struct StdChild {
    child: Option<std::process::Child>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    done: bool,
}

impl CommandChild for StdChild {
    fn try_wait(&mut self) -> Result<Option<ChildOutcome>, PickError> {
        if self.done {
            return Ok(None);
        }
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        match child.try_wait() {
            Ok(None) => Ok(None),
            Ok(Some(status)) => {
                self.done = true;
                // Dialog outputs are path lists (bytes, never
                // megabytes): take the pipes after exit, when no
                // writer can block us. Order: stdout, then stderr,
                // then wait (reap) — all post-exit, none blocking.
                if let Some(mut out) = child.stdout.take() {
                    use std::io::Read;
                    let _ = out.read_to_end(&mut self.stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    let _ = err.read_to_end(&mut self.stderr);
                }
                let _ = child.wait();
                Ok(Some(ChildOutcome {
                    code: status.code().unwrap_or(-1),
                    stdout: std::mem::take(&mut self.stdout),
                    stderr: std::mem::take(&mut self.stderr),
                }))
            }
            Err(e) => Err(PickError::Backend(format!("zenity wait failed: {e}"))),
        }
    }

    fn kill(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }
}

impl CommandRunner for StdRunner {
    type Child = StdChild;

    fn spawn(&mut self, program: &str, args: &[String]) -> Result<Self::Child, PickError> {
        std::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map(|child| StdChild {
                child: Some(child),
                stdout: Vec::new(),
                stderr: Vec::new(),
                done: false,
            })
            .map_err(|e| PickError::Backend(format!("spawn {program}: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Pure shapes (argv building, output parsing — headless-tested)
// ---------------------------------------------------------------------------

/// Builds the `zenity --file-selection` argv for `options`
/// (`directory` selects the `--directory` shape, always
/// single-select — stated). Newline-joined multi output (real `\n`
/// — passed as argv, never through a shell); one
/// `--file-filter` per filter (`NAME | p1 p2`); `--filename`
/// carries `initial_dir` (trailing slash hints folders).
pub fn zenity_argv(options: &FilePickerOptions, directory: bool) -> Vec<String> {
    let mut argv = vec!["--file-selection".to_string()];
    if directory {
        argv.push("--directory".to_string());
    } else if options.multiple {
        argv.push("--multiple".to_string());
        argv.push("--separator=\n".to_string());
    }
    if !options.title.is_empty() {
        argv.push(format!("--title={}", options.title));
    }
    if let Some(dir) = &options.initial_dir {
        let mut start = dir.to_string_lossy().into_owned();
        if !start.ends_with('/') {
            start.push('/');
        }
        argv.push(format!("--filename={start}"));
    }
    if !directory {
        for f in &options.filters {
            argv.push(format!(
                "--file-filter={} | {}",
                f.name,
                f.patterns.join(" ")
            ));
        }
    }
    argv
}

/// Builds the `zenity --file-selection --save` argv for `options`
/// (Round 16.1 — single destination: never `--multiple` or
/// `--directory`). `--filename` carries the suggested destination
/// (`initial_dir` + `default_name`, no trailing slash — it names a
/// file, not a folder); filters ride the same `--file-filter`
/// shape as open. Pure (headless-tested).
pub fn zenity_save_argv(options: &FileDialogOptions) -> Vec<String> {
    let mut argv = vec!["--file-selection".to_string(), "--save".to_string()];
    if !options.title.is_empty() {
        argv.push(format!("--title={}", options.title));
    }
    let mut start = options
        .initial_dir
        .as_ref()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !start.is_empty() && !start.ends_with('/') {
        start.push('/');
    }
    start.push_str(&options.default_name);
    if !start.is_empty() {
        argv.push(format!("--filename={start}"));
    }
    for f in &options.filters {
        argv.push(format!(
            "--file-filter={} | {}",
            f.name,
            f.patterns.join(" ")
        ));
    }
    argv
}

/// Parses zenity stdout (newline-joined by our `--separator`;
/// trailing newline dropped). Empty output reads as dismissed
/// (`Ok(vec![])` — the decision-230 dismissal rule, never an
/// error). Filenames containing newlines cannot round-trip
/// (essentially nonexistent — stated bound).
pub(crate) fn parse_zenity_output(out: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(out)
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Settles a reaped zenity run: exit 0 parses paths, exit 1 is
/// dismissal (`Ok(vec![])` — window-X and Cancel share it), anything
/// else is a loud `Backend` carrying stderr.
pub(crate) fn settle_zenity(outcome: &ChildOutcome) -> Result<Vec<PathBuf>, PickError> {
    match outcome.code {
        0 => Ok(parse_zenity_output(&outcome.stdout)),
        1 => Ok(Vec::new()),
        code => Err(PickError::Backend(format!(
            "zenity failed (exit {code}): {}",
            String::from_utf8_lossy(&outcome.stderr).trim()
        ))),
    }
}

/// True when `zenity --version` exits 0 (probe, not a dialog).
pub(crate) fn zenity_present<R: CommandRunner>(runner: &mut R) -> bool {
    let mut child = match runner.spawn("zenity", &["--version".to_string()]) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Spin briefly (local exec — milliseconds; bounded 2 s so a
    // wedged helper never hangs the probe, loudly false past it).
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(out)) => return out.code == 0,
            Ok(None) => {}
            Err(_) => return false,
        }
        if start.elapsed() > std::time::Duration::from_secs(2) {
            child.kill();
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
