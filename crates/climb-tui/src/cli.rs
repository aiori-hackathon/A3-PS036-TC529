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

    /// Whether `--results` was present on the command line. Filled in by
    /// [`Cli::parse_with_provenance`], not by clap.
    #[arg(skip)]
    pub results_explicit: bool,
}

impl Cli {
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
        assert!(!cli.tour);
        assert_eq!(cli.tour_speed, 1.0);
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
            "--tour",
            "--tour-speed",
            "1.5",
        ]);
        assert_eq!(cli.results, PathBuf::from("/tmp/r.json"));
        assert_eq!(cli.theme, ThemeId::Contrast);
        assert!(cli.ascii);
        assert!(cli.replay);
        assert!(cli.tour);
        assert_eq!(cli.tour_speed, 1.5);
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
