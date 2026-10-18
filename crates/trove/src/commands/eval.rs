use std::{fs, path::PathBuf};

use anyhow::{Context as _, bail};
use serde::Deserialize;
use trove_core::search::SearchRequest;

use super::Context;
use crate::searcher::{Searcher, StageSwitches};

/// How deep each query's results are scored: recall@10 and MRR over the top 10.
const DEPTH: u32 = 10;

#[derive(clap::Args)]
pub struct Args {
    /// A JSON array of `{"query": ..., "note": ...}`, where `note` is the path of the note the query should find,
    /// `/`-separated inside its source
    queries: PathBuf,
    #[command(flatten)]
    stages: StageSwitches,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvalQuery {
    query: String,
    note: String,
}

pub fn run(ctx: &Context, args: Args) -> anyhow::Result<()> {
    let queries = read_queries(&args)
        .with_context(|| format!("failed to read queries from {}", args.queries.display()))?;
    let searcher = Searcher::connect(ctx)?;
    let skip = args.stages.skip();

    let mut ranks: Vec<Option<usize>> = Vec::with_capacity(queries.len());
    for eval_query in &queries {
        let response = searcher
            .search(&SearchRequest {
                query: eval_query.query.clone(),
                limit: Some(DEPTH),
                skip: skip.clone(),
            })
            .with_context(|| format!("failed to search for {:?}", eval_query.query))?;
        let rank = response
            .results
            .iter()
            .position(|result| result.path == eval_query.note)
            .map(|place| place + 1);

        let shown = rank.map_or_else(|| "-".to_owned(), |rank| rank.to_string());
        println!("{shown:>2}  {}", eval_query.query);
        ranks.push(rank);
    }

    let count = ranks.len() as f64;
    let recall_at = |depth: usize| {
        ranks
            .iter()
            .filter(|rank| rank.is_some_and(|rank| rank <= depth))
            .count() as f64
            / count
    };
    let mrr = ranks
        .iter()
        .flatten()
        .map(|&rank| 1.0 / rank as f64)
        .sum::<f64>()
        / count;

    println!();
    println!("recall@5   {:.3}", recall_at(5));
    println!("recall@10  {:.3}", recall_at(10));
    println!("MRR        {mrr:.3}");

    Ok(())
}

fn read_queries(args: &Args) -> anyhow::Result<Vec<EvalQuery>> {
    let queries: Vec<EvalQuery> = serde_json::from_slice(&fs::read(&args.queries)?)?;
    if queries.is_empty() {
        bail!("the file has no queries");
    }
    for (place, eval_query) in queries.iter().enumerate() {
        if eval_query.query.trim().is_empty() || eval_query.note.trim().is_empty() {
            bail!("query {} has an empty `query` or `note`", place + 1);
        }
    }

    Ok(queries)
}
