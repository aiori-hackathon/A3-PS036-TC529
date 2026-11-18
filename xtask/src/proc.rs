//! Bounded subprocess execution for the automation commands.
//!
//! Every external process the toolchain starts is a place a benchmark run can
//! hang: a Docker daemon that accepts a connection but never answers, a `dig`
//! pointed at a nameserver that is still starting, or a Criterion run stuck on
//! a deadlock. [`run`] spawns the process in a process group of its own, with
//! both pipes captured and drained on their own threads (so a chatty child
//! cannot deadlock on a full pipe), and kills the whole group once the deadline
//! passes. A shell's own children inherit the pipes, so killing only the direct
//! child would leave the readers waiting for a `sleep` that the shell started.
//! The error always carries the child's stderr, not just an exit status.

use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

/// How long a reader thread may wait for end-of-file after the child was
/// stopped. A process that escaped the child's process group can hold a pipe
/// open indefinitely, so the wait is bounded and a deadline stays a deadline.
const PIPE_GRACE: Duration = Duration::from_secs(5);

/// Run `command` to completion or kill it after `timeout`.
///
/// `label` is the human-readable name used in every error this function
/// returns, so a failed step names the process rather than only its exit code.
pub(crate) fn run(command: &mut Command, timeout: Duration, label: &str) -> Result<Output> {
    let output = run_raw(command, timeout, label)?;
    if !output.status.success() {
        bail!(
            "`{label}` exited with {}{}",
            output.status,
            stderr_suffix(&output.stderr)
        );
    }
    Ok(output)
}

/// [`run`] without treating a non-zero exit as an error, for callers that need
/// the status while still enforcing the deadline and capturing both pipes.
pub(crate) fn run_raw(command: &mut Command, timeout: Duration, label: &str) -> Result<Output> {
    let mut child = spawn_in_own_group(command, label)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("`{label}` produced no stdout pipe"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("`{label}` produced no stderr pipe"))?;
    let stdout_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut stdout = stdout;
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let stderr_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut stderr = stderr;
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                stop(&mut child);
                let stderr = join_reader(stderr_reader);
                bail!(
                    "`{label}` could not be polled: {error}{}",
                    stderr_suffix(&stderr)
                );
            }
        }
        if started.elapsed() >= timeout {
            stop(&mut child);
            let stderr = join_reader(stderr_reader);
            bail!(
                "`{label}` timed out after {} s{}",
                timeout.as_secs_f64(),
                stderr_suffix(&stderr)
            );
        }
        thread::sleep(Duration::from_millis(50));
    };

    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Spawn `command` with both pipes captured in a process group of its own.
fn spawn_in_own_group(command: &mut Command, label: &str) -> Result<Child> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // The group id equals the child's pid, which is what `stop` signals.
        command.process_group(0);
    }
    command
        .spawn()
        .with_context(|| format!("failed to spawn `{label}`"))
}

/// Stop the child and everything it started, then reap it.
fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;
        // The child was spawned with `process_group(0)`, so its process group
        // id is its pid, and the pid is still live because it has not been
        // reaped, so this signals exactly that group. A failure (the group is
        // already gone) is ignored, as `Child::kill` ignores one.
        let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Join a reader thread, but never wait for it longer than [`PIPE_GRACE`].
fn join_reader(handle: thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    let started = Instant::now();
    while !handle.is_finished() && started.elapsed() < PIPE_GRACE {
        thread::sleep(Duration::from_millis(10));
    }
    if handle.is_finished() {
        handle.join().unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// The trailing stderr of a failed child, formatted for one-line errors.
fn stderr_suffix(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        let flat = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
        format!("; stderr: {flat}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_successful_command_captures_stdout_and_stderr() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf out; printf err >&2"]);
        let output = run(&mut cmd, Duration::from_secs(10), "printf").expect("command");
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
    }

    #[test]
    fn a_failing_command_reports_its_stderr() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf 'specific failure' >&2; exit 7"]);
        let error = run(&mut cmd, Duration::from_secs(10), "failing").expect_err("must fail");
        let message = format!("{error:#}");
        assert!(message.contains("exited with"), "{message}");
        assert!(message.contains("specific failure"), "{message}");
    }

    #[test]
    fn a_hung_command_is_killed_at_the_deadline() {
        let mut cmd = Command::new("sh");
        // The trailing command stops the shell from replacing itself with
        // `sleep`, so the grandchild is what holds the output pipes open: the
        // deadline has to stop the group, not only the direct child.
        cmd.args(["-c", "sleep 30; printf done"]);
        let started = Instant::now();
        let error = run(&mut cmd, Duration::from_millis(200), "sleep").expect_err("must time out");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(format!("{error:#}").contains("timed out"), "{error:#}");
    }
}
