//! Bounded subprocess execution for the automation commands.
//!
//! Every external process the toolchain starts is a place a benchmark run can
//! hang: a Docker daemon that accepts a connection but never answers, a `dig`
//! pointed at a nameserver that is still starting, or a Criterion run stuck on
//! a deadlock. [`run`] spawns the process with both pipes captured, drains them
//! on their own threads (so a chatty child cannot deadlock on a full pipe),
//! and kills it once the deadline passes. The error always carries the child's
//! stderr, not just an exit status.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

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
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn `{label}`"))?;

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
                let _ = child.kill();
                let _ = child.wait();
                let stderr = stderr_reader.join().unwrap_or_default();
                bail!(
                    "`{label}` could not be polled: {error}{}",
                    stderr_suffix(&stderr)
                );
            }
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let stderr = stderr_reader.join().unwrap_or_default();
            bail!(
                "`{label}` timed out after {} s{}",
                timeout.as_secs_f64(),
                stderr_suffix(&stderr)
            );
        }
        thread::sleep(Duration::from_millis(50));
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
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
        cmd.args(["-c", "sleep 30"]);
        let started = Instant::now();
        let error = run(&mut cmd, Duration::from_millis(200), "sleep").expect_err("must time out");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(format!("{error:#}").contains("timed out"), "{error:#}");
    }
}
