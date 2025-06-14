mod util;
mod ref_graph;

use std::path::PathBuf;
use clap::Parser;
use git2::{Repository, Revwalk, Sort};
use crate::ref_graph::RefGraph;

#[derive(Parser)]
#[clap(about)]
struct Args {
    /// Git repository path
    #[clap(short, long)]
    repo: PathBuf,

    /// Commit-ish of the first commit to be inspected
    #[clap(long)]
    first_commit: String,

    /// Commit-ish of the last commit to be inspected
    #[clap(long)]
    last_commit: Option<String>,

}

fn configure_walk<'a>(repo: &'a Repository, args: &Args) -> Revwalk<'a> {
    let mut walk = repo.revwalk()
        .unwrap_or_else(|e| panic!("Failed to get revwalk: {}", e));

    if let Some(last) = &args.last_commit {
        walk.push_range(&format!("{}..{}", args.first_commit, last))
            .unwrap_or_else(|e| panic!(
                "Failed to set range {}..{}: {}", args.first_commit, last, e
            ));
    } else {
        walk.push_ref(&args.first_commit)
            .unwrap_or_else(|e| panic!("Failed to push ref {}: {}", args.first_commit, e));
    }

    walk.set_sorting(Sort::REVERSE)
        .unwrap_or_else(|e| panic!("Failed to set sorting: {}", e));
    walk
}

fn main() {
    colog::init();
    let args = Args::parse();
    let repo = Repository::open(args.repo.clone())
        .unwrap_or_else(|e| panic!("Failed to open repository: {}", e));

    let walker = configure_walk(&repo, &args);
    let ref_graph = RefGraph::new(&repo, walker.into_iter());
    ref_graph.dump_info(&repo);
}
