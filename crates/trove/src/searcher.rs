use std::{collections::BTreeSet, sync::Mutex};

use trove_core::{
    embedder::Embedder,
    index::Index,
    search::{self, SearchRequest, SearchResponse, Signal},
};

use crate::{commands::Context, service::Service};

/// Switches that turn search stages off, so the eval can measure what each adds.
#[derive(clap::Args)]
pub struct StageSwitches {
    /// Leave out search by meaning
    #[arg(long)]
    no_meaning: bool,
    /// Leave out search by words
    #[arg(long)]
    no_words: bool,
}

impl StageSwitches {
    pub fn skip(&self) -> BTreeSet<Signal> {
        [
            (self.no_meaning, Signal::Meaning),
            (self.no_words, Signal::Words),
        ]
        .into_iter()
        .filter_map(|(off, signal)| off.then_some(signal))
        .collect()
    }
}

/// Runs searches through the running service, or else on the index directly with one model kept loaded across them.
pub enum Searcher {
    Service(Service),
    Local {
        index: Mutex<Index>,
        embedder: Embedder,
    },
}

impl Searcher {
    pub fn connect(ctx: &Context) -> anyhow::Result<Self> {
        Ok(match Service::connect(&ctx.data_dir)? {
            Some(service) => Self::Service(service),
            None => Self::Local {
                index: Mutex::new(Index::open(&ctx.data_dir)?),
                embedder: Embedder::from_config(&ctx.data_dir, &ctx.config),
            },
        })
    }

    pub fn search(&self, request: &SearchRequest) -> anyhow::Result<SearchResponse> {
        Ok(match self {
            Self::Service(service) => service.post("/search", request)?,
            Self::Local { index, embedder } => search::search(index, embedder, request)?,
        })
    }
}
