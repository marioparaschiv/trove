//! `trove eval --fit`: fits the ranking weights offline, on a fixed two thirds of the queries, and scores them against
//! the compiled ones on the other third, which the fit never sees. Search only ever ranks with the compiled weights.

use std::{path::Path, sync::Mutex};

use anyhow::bail;
use serde::Serialize;
use trove_core::{
    embedder::Embedder,
    index::Index,
    search::{
        self, SearchRequest, SearchResponse, Signals, WEIGHTS, Weights,
        features::{QueryFeatures, query_features},
    },
};

use super::{
    DEPTH, EvalQuery, Summary, outcomes, print_outcome, print_summary, summarize, write_json,
};
use crate::commands::Context;

/// One of the [`Weights`].
type Field = fn(&mut Weights) -> &mut f32;

/// The weights a linear score supports, fitted on the training queries' matching notes. Each is read from
/// [`Weights::score`] with it alone at 1, so a lookup's meaning and words are features of their own.
const LINEAR: [(&str, Field); 6] = [
    ("meaning", |weights| &mut weights.meaning),
    ("words", |weights| &mut weights.words),
    ("plain_words", |weights| &mut weights.plain_words),
    ("title", |weights| &mut weights.title),
    ("lookup_meaning", |weights| &mut weights.lookup_meaning),
    ("lookup_words", |weights| &mut weights.lookup_words),
];
const NO_WEIGHTS: Weights = Weights {
    meaning: 0.0,
    words: 0.0,
    plain_words: 0.0,
    title: 0.0,
    lookup_meaning: 0.0,
    lookup_words: 0.0,
    next_chunk_share: 0.0,
    graph_share: 0.0,
    feedback: 0.0,
};

/// The weights that act outside the linear score or inside one of its features are searched on this grid instead,
/// each point scored by the training queries' MRR through the whole search. The graph share stays above 0: the graph
/// rerank stays whatever the eval says, as it finds linked notes the eval doesn't ask for.
const NEXT_CHUNK_SHARES: [f32; 5] = [0.0, 0.05, 0.1, 0.15, 0.2];
const GRAPH_SHARES: [f32; 3] = [0.025, 0.05, 0.1];
const FEEDBACKS: [f32; 3] = [0.1, 0.2, 0.3];

/// The L2 penalty of the fit, on each weight's distance from the compiled one. The answer often beats every other note
/// on some weighting, and without a penalty the weights would grow without end; pulling towards the compiled weights
/// rather than 0 leaves a weight the training queries say little about (a lookup's, as most lookups find only their
/// note) near where it was.
const PENALTY: f64 = 0.01;
const MAX_STEPS: usize = 100;
/// Fitted weights are rounded to this many decimals, so the weights scored are the ones written into the source.
const DECIMALS: i32 = 2;

/// A training query whose note matches: its note's features and the other matching notes'.
struct Example {
    lookup: bool,
    answer: Signals,
    others: Vec<Signals>,
}

#[derive(Serialize)]
struct Comparison<'a> {
    compiled: Summary<'a>,
    fitted: Summary<'a>,
}

#[derive(Serialize)]
struct Report<'a> {
    compiled: Weights,
    fitted: Weights,
    /// Training queries with a note that matches and at least one other matching note, which the linear fit learns
    /// from.
    examples: usize,
    /// Of them, lookups ([`Weights::lookup_meaning`]).
    lookup_examples: usize,
    held_out: Comparison<'a>,
    all: Comparison<'a>,
    /// Whether the fitted weights keep the keep rule against the compiled ones on the held-out queries.
    kept: bool,
}

pub fn run(ctx: &Context, queries: &[EvalQuery], out: Option<&Path>) -> anyhow::Result<()> {
    let index = Mutex::new(Index::open(&ctx.data_dir)?);
    let embedder = Embedder::from_config(&ctx.data_dir, &ctx.config);
    let (held_out, training): (Vec<&EvalQuery>, Vec<&EvalQuery>) =
        queries.iter().partition(|query| is_held_out(&query.query));

    let mut examples = Vec::new();
    for query in &training {
        let Some(note) = query.expected.note() else {
            continue;
        };
        let features = query_features(&index, &embedder, &query.query, WEIGHTS)?;
        examples.extend(example(features, note));
    }

    let search_with = |weights| {
        let (index, embedder) = (&index, &embedder);
        move |query: &str| -> anyhow::Result<_> {
            let request = SearchRequest {
                query: query.to_owned(),
                limit: Some(DEPTH),
                skip: Default::default(),
            };
            Ok(search::search_with(index, embedder, &request, weights)?)
        }
    };

    let mut best: Option<(f64, Weights)> = None;
    for next_chunk_share in NEXT_CHUNK_SHARES {
        let linear = fit(&examples, next_chunk_share)?;
        for graph_share in GRAPH_SHARES {
            for feedback in FEEDBACKS {
                let weights = Weights {
                    next_chunk_share,
                    graph_share,
                    feedback,
                    ..linear
                };
                let summary = summarize(outcomes(training.iter().copied(), search_with(weights))?);
                let mrr = summary.overall.as_ref().map_or(0.0, |overall| overall.mrr);
                println!("train MRR {mrr:.3}  {weights:?}");
                if best.is_none_or(|(best_mrr, _)| mrr > best_mrr) {
                    best = Some((mrr, weights));
                }
            }
        }
    }
    let (_, fitted) = best.expect("the grid has points");

    let held_out = compare(&held_out, search_with, fitted)?;
    let report = Report {
        compiled: WEIGHTS,
        fitted,
        examples: examples.len(),
        lookup_examples: examples.iter().filter(|example| example.lookup).count(),
        kept: keeps(&held_out.compiled, &held_out.fitted),
        held_out,
        all: compare(&queries.iter().collect::<Vec<_>>(), search_with, fitted)?,
    };

    print_report(&report);
    if let Some(out) = out {
        write_json(out, &report)?;
    }

    Ok(())
}

/// `queries` scored with the compiled and the fitted weights.
fn compare<'a, Search>(
    queries: &[&'a EvalQuery],
    search_with: impl Fn(Weights) -> Search,
    fitted: Weights,
) -> anyhow::Result<Comparison<'a>>
where
    Search: FnMut(&str) -> anyhow::Result<SearchResponse>,
{
    Ok(Comparison {
        compiled: summarize(outcomes(queries.iter().copied(), search_with(WEIGHTS))?),
        fitted: summarize(outcomes(queries.iter().copied(), search_with(fitted))?),
    })
}

/// Whether a query is in the held-out third: by a hash of its text, so the split stays when queries are added or
/// moved, and spreads every kind over both parts.
fn is_held_out(query: &str) -> bool {
    fnv1a(query.as_bytes()).is_multiple_of(3)
}

/// 64-bit FNV-1a, which unlike std's hasher is fixed across Rust versions.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The query's example, when its note matches along with others.
fn example(features: QueryFeatures, note: &str) -> Option<Example> {
    let (answers, others): (Vec<_>, Vec<_>) = features
        .notes
        .into_iter()
        .partition(|matched| matched.path == note);
    let answer = answers.first()?.signals;
    if others.is_empty() {
        return None;
    }

    Some(Example {
        lookup: features.lookup,
        answer,
        others: others.into_iter().map(|other| other.signals).collect(),
    })
}

/// The [`LINEAR`] weights by pairwise logistic regression (RankNet's loss): for each example, the answer should score
/// above each other note, each example counting alike however many notes it has. Newton's method from the compiled
/// weights, so the same examples give the same weights. Scaled so meaning weighs 1, as only ratios rank.
fn fit(examples: &[Example], next_chunk_share: f32) -> anyhow::Result<Weights> {
    const N: usize = LINEAR.len();
    let pairs: Vec<(f64, Vec<[f64; N]>)> = examples
        .iter()
        .map(|example| {
            let answer = features(&example.answer, example.lookup, next_chunk_share);
            let differences = example
                .others
                .iter()
                .map(|other| {
                    let other = features(other, example.lookup, next_chunk_share);
                    std::array::from_fn(|i| answer[i] - other[i])
                })
                .collect();
            (1.0 / example.others.len() as f64, differences)
        })
        .collect();
    let examples_count = examples.len().max(1) as f64;

    let compiled = LINEAR.map(|(_, field)| {
        let mut compiled = WEIGHTS;
        f64::from(*field(&mut compiled))
    });
    let mut weights = compiled;
    for _ in 0..MAX_STEPS {
        let mut gradient: [f64; N] = std::array::from_fn(|i| PENALTY * (weights[i] - compiled[i]));
        let mut hessian: [[f64; N]; N] =
            std::array::from_fn(|i| std::array::from_fn(|j| if i == j { PENALTY } else { 0.0 }));
        for (share, differences) in &pairs {
            let share = share / examples_count;
            for difference in differences {
                let margin: f64 = (0..N).map(|i| weights[i] * difference[i]).sum();
                let wrong = 1.0 / (1.0 + margin.exp());
                for i in 0..N {
                    gradient[i] -= share * wrong * difference[i];
                    for j in 0..N {
                        hessian[i][j] +=
                            share * wrong * (1.0 - wrong) * difference[i] * difference[j];
                    }
                }
            }
        }

        let step = solve(hessian, gradient);
        for i in 0..N {
            weights[i] -= step[i];
        }
        if step.iter().all(|value| value.abs() < 1e-12) {
            break;
        }
    }

    let meaning = weights[0];
    if meaning <= 0.0 {
        bail!(
            "the fit gives meaning no weight ({meaning}), so there is no scale to put the weights on"
        );
    }
    let mut fitted = Weights {
        next_chunk_share,
        ..NO_WEIGHTS
    };
    let scale = 10_f64.powi(DECIMALS);
    for ((_, field), weight) in LINEAR.iter().zip(weights) {
        // + 0.0 turns a -0.0 from rounding into 0.0.
        *field(&mut fitted) = ((weight / meaning * scale).round() / scale + 0.0) as f32;
    }

    Ok(fitted)
}

/// Each [`LINEAR`] weight's term of a note's score.
fn features(signals: &Signals, lookup: bool, next_chunk_share: f32) -> [f64; LINEAR.len()] {
    LINEAR.map(|(_, field)| {
        let mut weights = Weights {
            next_chunk_share,
            ..NO_WEIGHTS
        };
        *field(&mut weights) = 1.0;
        f64::from(weights.score(signals, lookup))
    })
}

/// `x` with `matrix × x = vector`, by Gaussian elimination with partial pivoting. `matrix` is positive definite here:
/// the penalty adds to its diagonal.
fn solve<const N: usize>(mut matrix: [[f64; N]; N], mut vector: [f64; N]) -> [f64; N] {
    for column in 0..N {
        let pivot = (column..N)
            .max_by(|&a, &b| matrix[a][column].abs().total_cmp(&matrix[b][column].abs()))
            .expect("the column has rows");
        matrix.swap(column, pivot);
        vector.swap(column, pivot);
        for row in column + 1..N {
            let factor = matrix[row][column] / matrix[column][column];
            let pivot_row = matrix[column];
            for (cell, pivot_cell) in matrix[row][column..].iter_mut().zip(&pivot_row[column..]) {
                *cell -= factor * pivot_cell;
            }
            vector[row] -= factor * vector[column];
        }
    }

    let mut x = [0.0; N];
    for row in (0..N).rev() {
        let known: f64 = (row + 1..N).map(|k| matrix[row][k] * x[k]).sum();
        x[row] = (vector[row] - known) / matrix[row][row];
    }

    x
}

/// The keep rule: overall MRR rises, no kind loses more than one query at rank 1, and no fewer no-answer queries find
/// nothing.
fn keeps(compiled: &Summary<'_>, fitted: &Summary<'_>) -> bool {
    let mrr = |summary: &Summary<'_>| summary.overall.as_ref().map_or(0.0, |overall| overall.mrr);
    let empty_share = |summary: &Summary<'_>| {
        summary
            .no_answer
            .as_ref()
            .map_or(0.0, |no_answer| no_answer.empty_share)
    };
    let kinds_hold = compiled.kinds.iter().all(|(kind, before)| {
        let after = fitted.kinds.get(kind).map_or(0, |score| score.hits_at_1);
        after + 1 >= before.hits_at_1
    });

    mrr(fitted) > mrr(compiled) && kinds_hold && empty_share(fitted) >= empty_share(compiled)
}

fn print_report(report: &Report<'_>) {
    println!();
    println!("weight            compiled  fitted");
    for (name, field) in LINEAR {
        let (mut compiled, mut fitted) = (report.compiled, report.fitted);
        println!(
            "{name:<17} {:>8.3}  {:>6.3}",
            *field(&mut compiled),
            *field(&mut fitted)
        );
    }
    for (name, compiled, fitted) in [
        (
            "next_chunk_share",
            report.compiled.next_chunk_share,
            report.fitted.next_chunk_share,
        ),
        (
            "graph_share",
            report.compiled.graph_share,
            report.fitted.graph_share,
        ),
        ("feedback", report.compiled.feedback, report.fitted.feedback),
    ] {
        println!("{name:<17} {compiled:>8.3}  {fitted:>6.3}");
    }
    println!(
        "fitted on {} training queries, {} of them lookups",
        report.examples, report.lookup_examples
    );

    for (part, comparison) in [("held-out", &report.held_out), ("all", &report.all)] {
        for (weights, summary) in [
            ("compiled", &comparison.compiled),
            ("fitted", &comparison.fitted),
        ] {
            println!();
            println!("== {part} queries, {weights} weights");
            for outcome in &summary.queries {
                print_outcome(outcome);
            }
            print_summary(summary);
        }
    }

    println!();
    println!(
        "fitted weights {} the keep rule on the held-out queries",
        if report.kept { "keep" } else { "fail" }
    );
    println!("{:?}", report.fitted);
}

#[cfg(test)]
mod tests {
    use trove_core::search::Coverage;

    use super::*;

    fn signals(best_z: f32, weighted: f32) -> Signals {
        Signals {
            best_z,
            next_z: 0.0,
            coverage: Coverage {
                share: weighted,
                weighted,
                in_title: 0.0,
            },
        }
    }

    /// Answers that win by their rare words but lose by meaning to notes with fewer of them.
    fn examples() -> Vec<Example> {
        [
            (3.0, 1.0, 4.0, 0.5),
            (2.5, 0.9, 3.5, 0.4),
            (4.0, 0.8, 4.5, 0.2),
        ]
        .into_iter()
        .map(|(z, words, other_z, other_words)| Example {
            lookup: false,
            answer: signals(z, words),
            others: vec![signals(other_z, other_words), signals(1.0, 0.1)],
        })
        .collect()
    }

    #[test]
    fn the_fit_ranks_each_answer_first() {
        let fitted = fit(&examples(), 0.1).unwrap();

        assert_eq!(fitted.meaning, 1.0);
        for example in examples() {
            let answer = fitted.score(&example.answer, false);
            assert!(
                example
                    .others
                    .iter()
                    .all(|other| fitted.score(other, false) < answer)
            );
        }
    }

    #[test]
    fn the_split_hashes_with_fnv1a() {
        // The published 64-bit FNV-1a test vectors.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
