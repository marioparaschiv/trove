mod commands;

use clap::Parser;
use trove_core::{config::Config, data_dir::DataDir};

#[derive(Parser)]
#[command(
    version,
    about = "Search an Obsidian vault by meaning, words and links"
)]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let data_dir = DataDir::resolve()?;
    let config = Config::load(&data_dir)?;

    cli.command.run(&commands::Context { data_dir, config })
}
