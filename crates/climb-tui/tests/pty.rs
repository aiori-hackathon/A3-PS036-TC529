//! Terminal-restoration tests on a real pseudo-terminal.
//!
//! A TUI that dies with the terminal still in raw mode and the cursor hidden
//! leaves the user's shell unusable until they run `reset`. These tests run the
//! real binary under `script(1)` (a pty) and compare `stty -g` before and after
//! a forced panic and after SIGINT/SIGTERM/SIGHUP. When `script` or `stty` is
//! unavailable the test returns without failing, and says so on stderr.

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_climb-tui")
}

fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

fn tools_available() -> bool {
    for tool in ["script", "stty"] {
        match Command::new(tool).arg("--version").output() {
            Ok(output) if output.status.success() => {}
            _ => {
                eprintln!("skipping PTY test: {tool} is not available");
                return false;
            }
        }
    }
    true
}

/// Run a shell script inside a pty and return the transcript.
fn run_in_pty(script: &str) -> Option<String> {
    let output = Command::new("script")
        .arg("-qec")
        .arg(script)
        .arg("/dev/null")
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Pull one `KEY:value` line out of the pty transcript. The pty translates
/// newlines to CRLF, so the carriage return is trimmed.
fn field(transcript: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    transcript.lines().find_map(|line| {
        let line = line.trim_end_matches('\r');
        // The dashboard writes escape sequences straight into the pty, so a
        // marker can share a line with them; find it rather than requiring it
        // at the start of the line.
        line.find(&prefix)
            .map(|index| line[index + prefix.len()..].trim().to_string())
    })
}

fn restoration_script(body: &str) -> String {
    // `script -c` runs the command through the user's shell, which may be zsh.
    // `status` is a read-only variable there, so use a neutral name.
    format!(
        "set +e\n\
         before=$(stty -g)\n\
         {body}\n\
         after=$(stty -g)\n\
         printf 'BEFORE:%s\\n' \"$before\"\n\
         printf 'AFTER:%s\\n' \"$after\"\n\
         printf 'STATUS:%s\\n' \"$rc\"\n\
         exit 0\n"
    )
}

#[test]
fn a_panic_restores_raw_mode_and_the_screen() {
    if !tools_available() {
        return;
    }
    let script = restoration_script(&format!("{} --debug-panic\nrc=$?", shell_quote(bin())));
    let transcript = run_in_pty(&script).expect("the pty runs");

    let before = field(&transcript, "BEFORE").expect("before state printed");
    let after = field(&transcript, "AFTER").expect("after state printed");
    let status = field(&transcript, "STATUS").expect("status printed");

    assert_ne!(status, "0", "the forced panic must fail the process");
    assert_eq!(
        after, before,
        "the terminal settings changed across the panic\n{transcript}"
    );
    assert!(
        transcript.contains("\u{1b}[?1049h"),
        "the test never entered the alternate screen, so it proves nothing\n{transcript}"
    );
    assert!(
        transcript.contains("\u{1b}[?1049l"),
        "the panic hook did not leave the alternate screen\n{transcript}"
    );
}

#[test]
fn every_fatal_signal_restores_the_terminal() {
    if !tools_available() {
        return;
    }
    for (name, expected_status) in [("INT", "130"), ("TERM", "143"), ("HUP", "129")] {
        let script = format!(
            "set +e\n\
             before=$(stty -g)\n\
             {bin} --debug-hold-ms 5000 </dev/tty &\n\
             pid=$!\n\
             sleep 0.6\n\
             during=$(stty -g)\n\
             kill -{name} $pid\n\
             wait $pid\n\
             rc=$?\n\
             after=$(stty -g)\n\
             printf 'BEFORE:%s\\n' \"$before\"\n\
             printf 'DURING:%s\\n' \"$during\"\n\
             printf 'AFTER:%s\\n' \"$after\"\n\
             printf 'STATUS:%s\\n' \"$rc\"\n\
             exit 0\n",
            bin = shell_quote(bin()),
        );
        let transcript = run_in_pty(&script).expect("the pty runs");

        let before = field(&transcript, "BEFORE").expect("before state printed");
        let during = field(&transcript, "DURING").expect("during state printed");
        let after = field(&transcript, "AFTER").expect("after state printed");
        let status = field(&transcript, "STATUS").expect("status printed");

        assert_eq!(
            status, expected_status,
            "SIG{name} must exit with 128+signal\n{transcript}"
        );
        assert_ne!(
            during, before,
            "SIG{name}: the dashboard never held raw mode, so restoration is unproven\n{transcript}"
        );
        assert_eq!(
            after, before,
            "SIG{name} left the terminal in raw mode\n{transcript}"
        );
    }
}
