//! Command-line interface for `climb-tui`.
//!
//! Every flag is parsed now and carried into [`crate::app::App`]. `--replay`
//! is recorded and surfaced in the status bar; `--tour` starts the scripted
//! walkthrough (see [`crate::tour`]), whose playback rate `--tour-speed`
//! scales.

use std::path::PathBuf;

use clap::Parser;

use crate::theme::ThemeId;
use crate::tour::{MAX_SPEED, MIN_SPEED};

/// CLIMB live-run benchmark dashboard.
///
/// `PartialEq` but not `Eq`: `--tour-speed` is an `f64`.
#[derive(Debug, Clone, PartialEq, Parser)]
#[command(name = "climb-tui", version, about, long_about = None)]
pub struct Cli {
    /// Path to the consolidated results JSON.
    #[arg(long, value_name = "PATH", default_value = "target/climb-results.json")]
    pub results: PathBuf,

    /// Colour theme.
    #[arg(long, value_enum, default_value_t = ThemeId::Dark)]
    pub theme: ThemeId,

    /// Force ASCII-only glyphs (no box-drawing or braille).
    #[arg(long)]
    pub ascii: bool,

    /// Replay a recorded run from `--results` instead of attaching to a live run.
    #[arg(long)]
    pub replay: bool,

    /// Validate the named artifacts, print one line per problem, and exit
    /// without opening the terminal. Exit status is non-zero on any missing,
    /// unreadable or unparseable required artifact.
    #[arg(long)]
    pub check: bool,

    /// Ladder-verification artifact to validate with `--check` (defaults to the
    /// normal live/recorded/embedded search order).
    #[arg(long, value_name = "PATH")]
    pub ladder: Option<PathBuf>,

    /// DNS capture to validate with `--check` (defaults to the normal
    /// live/recorded/embedded search order).
    #[arg(long, value_name = "PATH")]
    pub live: Option<PathBuf>,

    /// Begin with the guided tour enabled.
    #[arg(long)]
    pub tour: bool,

    /// Guided-tour playback speed: 0.5 (half) to 2.0 (double) scripted dwells.
    #[arg(
        long,
        value_name = "X",
        default_value_t = 1.0,
        value_parser = parse_tour_speed,
    )]
    pub tour_speed: f64,

    /// Print one rendered text frame and exit, without needing a terminal.
    /// `TAB` is a tab title (e.g. `overview`) and `SIZE` is `WIDTHxHEIGHT`
    /// (e.g. `120x40`).
    #[arg(long, num_args = 2, value_names = ["TAB", "SIZE"])]
    pub dump_frame: Option<Vec<String>>,

    /// Test hook: panic immediately after entering raw mode, so a PTY test can
    /// prove the panic hook restores the terminal. Debug builds only.
    #[arg(long, hide = true)]
    pub debug_panic: bool,

    /// Test hook: stay in raw mode for this many milliseconds, so a PTY test
    /// can deliver SIGINT/SIGTERM/SIGHUP. Debug builds only.
    #[arg(long, hide = true, value_name = "MS", default_value_t = 0)]
    pub debug_hold_ms: u64,

    /// Whether `--results` was present on the command line. Filled in by
    /// [`Cli::parse_with_provenance`], not by clap.
    #[arg(skip)]
    pub results_explicit: bool,
}

impl Cli {
    /// The defaults a front end without a command line uses.
    ///
    /// The browser has no `argv`; parsing an empty command line gives exactly
    /// the same defaults the terminal gets, so the two front ends cannot drift
    /// on theme, results path or tour speed.
    #[must_use]
    pub fn viewer() -> Self {
        Self::parse_from(["climb-tui"])
    }
    /// Parse the process arguments and record whether the results path was
    /// explicit, so the loader can tell "the user asked for this file" from
    /// "the default target path".
    pub fn parse_with_provenance() -> Self {
        use clap::parser::ValueSource;
        use clap::{CommandFactory, FromArgMatches};

        let matches = Self::command().get_matches();
        let results_explicit = matches.value_source("results") == Some(ValueSource::CommandLine);
        let mut cli = Self::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
        cli.results_explicit = results_explicit;
        cli
    }
}

/// Parse a `--tour-speed` value, rejecting anything outside
/// [`MIN_SPEED`]..=[`MAX_SPEED`]. The range is enforced here rather than by
/// clap's numeric parser so the bound lives in one place.
fn parse_tour_speed(raw: &str) -> Result<f64, String> {
    let speed: f64 = raw
        .parse()
        .map_err(|_| format!("'{raw}' is not a number"))?;
    if !speed.is_finite() || !(MIN_SPEED..=MAX_SPEED).contains(&speed) {
        return Err(format!(
            "{speed} is outside the supported range {MIN_SPEED}..={MAX_SPEED}"
        ));
    }
    Ok(speed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_documented_values() {
        let cli = Cli::parse_from(["climb-tui"]);
        assert_eq!(cli.results, PathBuf::from("target/climb-results.json"));
        assert_eq!(cli.theme, ThemeId::Dark);
        assert!(!cli.ascii);
        assert!(!cli.replay);
        assert!(!cli.check);
        assert!(cli.ladder.is_none());
        assert!(cli.live.is_none());
        assert!(!cli.tour);
        assert_eq!(cli.tour_speed, 1.0);
        assert!(cli.dump_frame.is_none());
        assert!(!cli.debug_panic);
        assert_eq!(cli.debug_hold_ms, 0);
    }

    #[test]
    fn every_flag_parses() {
        let cli = Cli::parse_from([
            "climb-tui",
            "--results",
            "/tmp/r.json",
            "--theme",
            "contrast",
            "--ascii",
            "--replay",
            "--check",
            "--ladder",
            "/tmp/l.json",
            "--live",
            "/tmp/c.json",
            "--tour",
            "--tour-speed",
            "1.5",
            "--dump-frame",
            "overview",
            "80x24",
            "--debug-panic",
            "--debug-hold-ms",
            "2500",
        ]);
        assert_eq!(cli.results, PathBuf::from("/tmp/r.json"));
        assert_eq!(cli.theme, ThemeId::Contrast);
        assert!(cli.ascii);
        assert!(cli.replay);
        assert!(cli.check);
        assert_eq!(cli.ladder, Some(PathBuf::from("/tmp/l.json")));
        assert_eq!(cli.live, Some(PathBuf::from("/tmp/c.json")));
        assert!(cli.tour);
        assert_eq!(cli.tour_speed, 1.5);
        assert_eq!(
            cli.dump_frame,
            Some(vec!["overview".to_string(), "80x24".to_string()])
        );
        assert!(cli.debug_panic);
        assert_eq!(cli.debug_hold_ms, 2500);
    }

    #[test]
    fn tour_speed_accepts_the_documented_range_and_rejects_the_rest() {
        for (arg, expected) in [("0.5", 0.5), ("1", 1.0), ("2.0", 2.0)] {
            let cli = Cli::parse_from(["climb-tui", "--tour-speed", arg]);
            assert_eq!(cli.tour_speed, expected, "--tour-speed {arg}");
        }
        for bad in ["0.4", "2.01", "fast", "-1"] {
            let result = Cli::try_parse_from(["climb-tui", "--tour-speed", bad]);
            assert!(
                result.is_err(),
                "--tour-speed {bad} must be rejected by the 0.5..=2.0 range"
            );
        }
    }

    #[test]
    fn the_viewer_defaults_match_the_terminal_defaults() {
        let viewer = Cli::viewer();
        let parsed = Cli::parse_from(["climb-tui"]);
        assert_eq!(viewer.results, parsed.results);
        assert_eq!(viewer.theme, parsed.theme);
        assert!(!viewer.tour && !viewer.ascii && !viewer.check);
        assert_eq!(viewer.tour_speed, 1.0);
        assert!(!viewer.results_explicit);
    }

    #[test]
    fn theme_accepts_every_variant() {
        for (arg, expected) in [
            ("dark", ThemeId::Dark),
            ("light", ThemeId::Light),
            ("contrast", ThemeId::Contrast),
        ] {
            let cli = Cli::parse_from(["climb-tui", "--theme", arg]);
            assert_eq!(cli.theme, expected);
        }
    }
}
