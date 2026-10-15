use std::{collections::HashMap, sync::Mutex};

use trove_core::{
    categories::{self, CategoriesResponse, Category},
    index::Index,
};

use super::Context;
use crate::service::Service;

#[derive(clap::Args)]
pub struct Args {
    /// Print the categories as JSON, as the HTTP API returns them
    #[arg(long)]
    json: bool,
}

pub fn run(ctx: &Context, args: Args) -> anyhow::Result<()> {
    let response: CategoriesResponse = match Service::connect(&ctx.data_dir)? {
        Some(service) => service.get("/categories")?,
        None => categories::categories(&Mutex::new(Index::open(&ctx.data_dir)?))?,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    if response.categories.is_empty() {
        println!("no categories");
    }
    let mut children: HashMap<Option<usize>, Vec<&Category>> = HashMap::new();
    for category in &response.categories {
        children.entry(category.parent).or_default().push(category);
    }
    print_level(&children, None, 0);

    Ok(())
}

/// Prints each category under `parent` with its notes, then its subcategories indented below it.
fn print_level(
    children: &HashMap<Option<usize>, Vec<&Category>>,
    parent: Option<usize>,
    depth: usize,
) {
    let indent = "  ".repeat(depth);
    for category in children.get(&parent).into_iter().flatten() {
        println!(
            "{indent}{}  (start here: {})",
            category.name, category.start_here.path
        );
        for note in &category.notes {
            println!("{indent}  - {}", note.path);
        }
        print_level(children, Some(category.id), depth + 1);
    }
}
