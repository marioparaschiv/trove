mod fit;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};
use serde::{Deserialize, Serialize};
use trove_core::search::{SearchRequest, SearchResponse};

use super::Context;
use crate::searcher::{Searcher, StageSwitches};

/// How deep each query's results are scored: recall@10 and MRR over the top 10.
const DEPTH: u32 = 10;

#[derive(clap::Args)]
pub struct Args {
    /// A JSON array of `{"query": ..., "kind": ..., "lang": ..., "note": ...}`. `kind` is paraphrase, lookup,
    /// question, vague, typo or no_answer, and `lang` the query's language code. `note` is the path of the note the
    /// query should find, `/`-separated inside its source, and null for a no_answer query, which should find nothing
    queries: PathBuf,
    /// Also write the scores and each query's rank to this file as JSON, to diff runs
    #[arg(long)]
    out: Option<PathBuf>,
    /// Instead, fit the ranking weights on two thirds of the queries and score them against the compiled ones on the
    /// other third and on all
    #[arg(long)]
    fit: bool,
    #[command(flatten)]
    stages: StageSwitches,
}

#[derive(Deserialize)]
struct EvalQuery {
    query: String,
    lang: String,
    #[serde(flatten)]
    expected: Expected,
}

/// What a query should find, by its kind.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Expected {
    /// Describes the note in words it doesn't use.
    Paraphrase { note: String },
    /// Names a rare term of the note: a name, code or number.
    Lookup { note: String },
    /// Asks about the note in a full sentence.
    Question { note: String },
    /// One to three words.
    Vague { note: String },
    /// Misspells one key word of the note.
    Typo { note: String },
    /// Asks about a topic no note covers, so it should find nothing.
    NoAnswer {
        #[expect(
            dead_code,
            reason = "parsed only to require null, so a note here is an error"
        )]
        note: (),
    },
}

impl Expected {
    fn kind(&self) -> &'static str {
        match self {
            Self::Paraphrase { .. } => "paraphrase",
            Self::Lookup { .. } => "lookup",
            Self::Question { .. } => "question",
            Self::Vague { .. } => "vague",
            Self::Typo { .. } => "typo",
            Self::NoAnswer { .. } => "no_answer",
        }
    }

    fn note(&self) -> Option<&str> {
        match self {
            Self::Paraphrase { note }
            | Self::Lookup { note }
            | Self::Question { note }
            | Self::Vague { note }
            | Self::Typo { note } => Some(note),
            Self::NoAnswer { .. } => None,
        }
    }
}

/// The scores of a group of queries that each have a note. No-answer queries never enter one; they are scored only
/// by the share of them that find nothing.
#[derive(Serialize, Default)]
struct Score {
    queries: usize,
    hits_at_1: usize,
    hits_at_10: usize,
    #[serde(skip)]
    reciprocal_ranks: f64,
    r_at_1: f64,
    r_at_10: f64,
    mrr: f64,
}

impl Score {
    fn add(&mut self, rank: Option<usize>) {
        self.queries += 1;
        if let Some(rank) = rank {
            self.hits_at_1 += usize::from(rank == 1);
            self.hits_at_10 += usize::from(rank <= DEPTH as usize);
            self.reciprocal_ranks += 1.0 / rank as f64;
        }

        let count = self.queries as f64;
        self.r_at_1 = self.hits_at_1 as f64 / count;
        self.r_at_10 = self.hits_at_10 as f64 / count;
        self.mrr = self.reciprocal_ranks / count;
    }
}

#[derive(Serialize)]
struct NoAnswerScore {
    queries: usize,
    empty: usize,
    empty_share: f64,
}

#[derive(Serialize)]
struct QueryOutcome<'a> {
    query: &'a str,
    kind: &'static str,
    lang: &'a str,
    note: Option<&'a str>,
    /// The place of the note in the results, from 1; `None` when it is missing or the query has no note.
    rank: Option<usize>,
    results: usize,
}

#[derive(Serialize)]
struct Summary<'a> {
    /// Every query that has a note; `None` when no query has one.
    overall: Option<Score>,
    kinds: BTreeMap<&'static str, Score>,
    langs: BTreeMap<&'a str, Score>,
    no_answer: Option<NoAnswerScore>,
    queries: Vec<QueryOutcome<'a>>,
}

pub fn run(ctx: &Context, args: Args) -> anyhow::Result<()> {
    let queries = read_queries(&args)
        .with_context(|| format!("failed to read queries from {}", args.queries.display()))?;
    let skip = args.stages.skip();
    if args.fit {
        if !skip.is_empty() {
            bail!("--fit searches with every stage");
        }
        return fit::run(ctx, &queries, args.out.as_deref());
    }

    let searcher = Searcher::connect(ctx)?;
    let outcomes = outcomes(&queries, |query| {
        searcher.search(&SearchRequest {
            query: query.to_owned(),
            limit: Some(DEPTH),
            skip: skip.clone(),
        })
    })?;
    for outcome in &outcomes {
        print_outcome(outcome);
    }

    let summary = summarize(outcomes);
    print_summary(&summary);
    if let Some(out) = &args.out {
        write_json(out, &summary)?;
    }

    Ok(())
}

/// Where each query's note is in what `search` finds for it.
fn outcomes<'a>(
    queries: impl IntoIterator<Item = &'a EvalQuery>,
    mut search: impl FnMut(&str) -> anyhow::Result<SearchResponse>,
) -> anyhow::Result<Vec<QueryOutcome<'a>>> {
    queries
        .into_iter()
        .map(|eval_query| {
            let response = search(&eval_query.query)
                .with_context(|| format!("failed to search for {:?}", eval_query.query))?;
            let note = eval_query.expected.note();
            let rank = note.and_then(|note| {
                response
                    .results
                    .iter()
                    .position(|result| result.path == note)
                    .map(|place| place + 1)
            });

            Ok(QueryOutcome {
                query: &eval_query.query,
                kind: eval_query.expected.kind(),
                lang: &eval_query.lang,
                note,
                rank,
                results: response.results.len(),
            })
        })
        .collect()
}

fn print_outcome(outcome: &QueryOutcome<'_>) {
    let shown = match (outcome.note, outcome.rank) {
        (Some(_), Some(rank)) => rank.to_string(),
        (Some(_), None) => "-".to_owned(),
        (None, _) if outcome.results == 0 => "ok".to_owned(),
        (None, _) => format!("+{}", outcome.results),
    };
    println!(
        "{shown:>3}  {:<10} {:<3} {}",
        outcome.kind, outcome.lang, outcome.query
    );
}

fn write_json(out: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    let json = serde_json::to_vec_pretty(value)?;
    fs::write(out, json).with_context(|| format!("failed to write {}", out.display()))
}

fn read_queries(args: &Args) -> anyhow::Result<Vec<EvalQuery>> {
    let queries: Vec<EvalQuery> = serde_json::from_slice(&fs::read(&args.queries)?)?;
    if queries.is_empty() {
        bail!("the file has no queries");
    }
    for (place, eval_query) in queries.iter().enumerate() {
        let blank_note = eval_query
            .expected
            .note()
            .is_some_and(|note| note.trim().is_empty());
        if eval_query.query.trim().is_empty() || eval_query.lang.trim().is_empty() || blank_note {
            bail!("query {} has an empty `query`, `lang` or `note`", place + 1);
        }
    }

    Ok(queries)
}

fn summarize(outcomes: Vec<QueryOutcome<'_>>) -> Summary<'_> {
    let mut overall = Score::default();
    let mut kinds: BTreeMap<&str, Score> = BTreeMap::new();
    let mut langs: BTreeMap<&str, Score> = BTreeMap::new();
    let (mut unanswered, mut empty) = (0, 0);
    for outcome in &outcomes {
        if outcome.note.is_none() {
            unanswered += 1;
            empty += usize::from(outcome.results == 0);
            continue;
        }

        overall.add(outcome.rank);
        kinds.entry(outcome.kind).or_default().add(outcome.rank);
        langs.entry(outcome.lang).or_default().add(outcome.rank);
    }

    Summary {
        overall: (overall.queries > 0).then_some(overall),
        kinds,
        langs,
        no_answer: (unanswered > 0).then(|| NoAnswerScore {
            queries: unanswered,
            empty,
            empty_share: empty as f64 / unanswered as f64,
        }),
        queries: outcomes,
    }
}

fn print_summary(summary: &Summary<'_>) {
    println!();
    print_scores("kind", &summary.kinds);
    if let Some(overall) = &summary.overall {
        print_score_row("overall", overall);
    }

    println!();
    print_scores("lang", &summary.langs);

    if let Some(no_answer) = &summary.no_answer {
        println!();
        println!(
            "no_answer  {:>7}  empty {}/{} = {:.3}",
            no_answer.queries, no_answer.empty, no_answer.queries, no_answer.empty_share
        );
    }
}

fn print_scores(group: &str, scores: &BTreeMap<&str, Score>) {
    println!(
        "{group:<10} {:>7}  {:>5}  {:>5}  {:>5}",
        "queries", "R@1", "R@10", "MRR"
    );
    for (name, score) in scores {
        print_score_row(name, score);
    }
}

fn print_score_row(name: &str, score: &Score) {
    println!(
        "{name:<10} {:>7}  {:.3}  {:.3}  {:.3}",
        score.queries, score.r_at_1, score.r_at_10, score.mrr
    );
}
