use trove_core::search::{Candidate, Pipeline, Query, SearchError, Stage};

struct AppendChunk(i64);

impl Stage for AppendChunk {
    fn run(
        &self,
        _query: &Query,
        mut candidates: Vec<Candidate>,
    ) -> Result<Vec<Candidate>, SearchError> {
        candidates.push(Candidate {
            chunk_id: self.0,
            score: 1.0,
        });

        Ok(candidates)
    }
}

#[test]
fn stages_run_in_order_each_receiving_the_previous_output() {
    let pipeline = Pipeline::new(vec![Box::new(AppendChunk(1)), Box::new(AppendChunk(2))]);

    let candidates = pipeline
        .run(&Query {
            text: "retry backoff".to_owned(),
        })
        .unwrap();

    let chunk_ids: Vec<i64> = candidates
        .iter()
        .map(|candidate| candidate.chunk_id)
        .collect();
    assert_eq!(chunk_ids, [1, 2]);
}
