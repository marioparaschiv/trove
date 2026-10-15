use trove_core::search::{DEFAULT_LIMIT, SearchRequest};

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
        let mut target = result.path.clone();
        for heading in &result.heading_path {
            target.push_str(" > ");
            target.push_str(heading);
        }
        let why: Vec<String> = result.why.iter().map(ToString::to_string).collect();
        println!("{:.4}  {target}  ({})", result.score, why.join(", "));
    }

    Ok(())
}
