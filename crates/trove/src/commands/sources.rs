use anyhow::bail;

use super::Context;

#[derive(clap::Subcommand)]
pub enum Command {
    /// Register a folder as a source
    Add,
    /// Stop indexing a source
    Remove,
    /// List registered sources
    List,
}

pub fn run(_ctx: &Context, _command: Command) -> anyhow::Result<()> {
    bail!("`trove sources` is not implemented yet")
}
