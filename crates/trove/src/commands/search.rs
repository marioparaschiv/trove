use trove_core::search::{DEFAULT_LIMIT, Location, Match, SearchRequest};

use super::Context;
use crate::searcher::{Searcher, StageSwitches};

#[derive(clap::Args)]
pub struct Args {
    query: String,
    /// The most notes to show
    #[arg(long, default_value_t = DEFAULT_LIMIT, value_parser = clap::value_parser!(u32).range(1..))]
    limit: u32,
    /// Print the results as JSON, as the HTTP API returns them
    #[arg(long)]
    json: bool,
    #[command(flatten)]
    stages: StageSwitches,
}

pub fn run(ctx: &Context, args: Args) -> anyhow::Result<()> {
    let response = Searcher::connect(ctx)?.search(&SearchRequest {
        query: args.query,
        limit: Some(args.limit),
        skip: args.stages.skip(),
    })?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    if response.results.is_empty() {
        println!("no matches");
    }
    for result in &response.results {
        let why: Vec<String> = result.why.iter().map(ToString::to_string).collect();
        let mut matches = result.matches.iter();
        let best = matches
            .next()
            .map_or_else(|| result.path.clone(), |best| target(&result.path, best));
        println!("{:.4}  {best}  ({})", result.score, why.join(", "));
        for other in matches {
            println!("        {}", target(&result.path, other));
        }
    }

    Ok(())
}

/// Where a match is: `path:first-last line > heading > heading`.
fn target(path: &str, found: &Match) -> String {
    let mut target = path.to_owned();
    if let Location::Text {
        line_start: Some(start),
        line_end: Some(end),
        ..
    } = found.location
    {
        target.push_str(&if start == end {
            format!(":{start}")
        } else {
            format!(":{start}-{end}")
        });
    }
    for heading in &found.heading_path {
        target.push_str(" > ");
        target.push_str(heading);
    }

    target
}
