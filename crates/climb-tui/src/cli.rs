//! Command-line interface for `climb-tui` (phase 18.2).
//!
//! Every flag is parsed now and carried into [`crate::app::App`]. The behaviour behind
//! `--replay` and `--tour` is wired in later phases; at this stage they are recorded and
//! surfaced in the status bar.

use std::path::PathBuf;

use clap::Parser;

use crate::theme::ThemeId;

/// CLIMB live-run benchmark dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
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
        ]);
        assert_eq!(cli.results, PathBuf::from("/tmp/r.json"));
        assert_eq!(cli.theme, ThemeId::Contrast);
        assert!(cli.ascii);
        assert!(cli.replay);
        assert!(cli.tour);
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
