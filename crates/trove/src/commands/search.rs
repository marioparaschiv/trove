use std::sync::Mutex;

use trove_core::{
    embedder::Embedder,
    index::Index,
    search::{self, DEFAULT_LIMIT, SearchRequest, SearchResponse},
};

use super::Context;
use crate::service::Service;

#[derive(clap::Args)]
pub struct Args {
    query: String,
    /// The most notes to show
    #[arg(long, default_value_t = DEFAULT_LIMIT, value_parser = clap::value_parser!(u32).range(1..))]
    limit: u32,
    /// Print the results as JSON, as the HTTP API returns them
    #[arg(long)]
    json: bool,
}

pub fn run(ctx: &Context, args: Args) -> anyhow::Result<()> {
    let request = SearchRequest {
        query: args.query,
        limit: Some(args.limit),
        skip: Default::default(),
    };
    let response: SearchResponse = match Service::connect(&ctx.data_dir)? {
        Some(service) => service.post("/search", &request)?,
        None => search::search(
            &Mutex::new(Index::open(&ctx.data_dir)?),
            &Embedder::from_config(&ctx.data_dir, &ctx.config),
            &request,
        )?,
    };

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
        println!("{:>6.2}  {target}", result.score);
    }

    Ok(())
}
