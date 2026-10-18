mod categories;
mod eval;
mod index;
mod openapi;
mod search;
mod serve;
mod setup;
mod sources;
mod status;
mod unload;

use clap::Subcommand;
use trove_core::{config::Config, data_dir::DataDir};

pub struct Context {
    pub data_dir: DataDir,
    pub config: Config,
}

#[derive(Subcommand)]
pub enum Command {
    /// Download and verify the embedding model
    Setup(setup::Args),
    /// Run the service on localhost
    Serve(serve::Args),
    /// Show model state and index size
    Status(status::Args),
    /// Manage the folders Trove indexes
    #[command(subcommand)]
    Sources(sources::Command),
    /// Index changed files in every source
    Index(index::Args),
    /// Search the index
    Search(search::Args),
    /// Unload the model now
    Unload(unload::Args),
    /// Show the notes grouped into categories
    Categories(categories::Args),
    /// Score search against a queries file
    Eval(eval::Args),
    /// Print the HTTP API's OpenAPI spec, which the TypeScript client is generated from
    #[command(hide = true)]
    Openapi(openapi::Args),
}

impl Command {
    pub fn run(self, ctx: &Context) -> anyhow::Result<()> {
        match self {
            Self::Setup(args) => setup::run(ctx, args),
            Self::Serve(args) => serve::run(ctx, args),
            Self::Status(args) => status::run(ctx, args),
            Self::Sources(command) => sources::run(ctx, command),
            Self::Index(args) => index::run(ctx, args),
            Self::Search(args) => search::run(ctx, args),
            Self::Unload(args) => unload::run(ctx, args),
            Self::Categories(args) => categories::run(ctx, args),
            Self::Eval(args) => eval::run(ctx, args),
            Self::Openapi(args) => openapi::run(ctx, args),
        }
    }
}
