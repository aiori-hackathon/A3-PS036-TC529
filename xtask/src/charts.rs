use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;

use climb_report::{charts::generate_charts, ClimbResults};

#[derive(Args)]
pub(crate) struct ChartsArgs {
    /// Consolidated results JSON to render charts from.
    #[arg(long, default_value = "target/climb-results.json")]
    results: PathBuf,

    /// Directory to write the five SVG charts into.
    #[arg(long, default_value = "target/climb-report/charts")]
    out: PathBuf,
}

pub(crate) fn cmd_charts(args: ChartsArgs) -> Result<()> {
    let s = std::fs::read_to_string(&args.results)
        .with_context(|| format!("failed to read {}", args.results.display()))?;
    let results: ClimbResults = serde_json::from_str(&s)
        .with_context(|| format!("failed to parse {}", args.results.display()))?;

    let written =
        generate_charts(&results, &args.out).with_context(|| "failed to render charts")?;

    for path in &written {
        println!("wrote {}", path.display());
    }
    Ok(())
}
