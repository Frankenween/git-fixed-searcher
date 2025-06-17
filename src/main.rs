mod util;
mod ref_graph;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::path::PathBuf;
use clap::Parser;
use git2::{Commit, Oid, Repository, Revwalk, Sort};
use crate::ref_graph::RefGraph;
use crate::util::{parse_commit_description, read_lines_from_bufreader};

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

    /// Print summary results for the listed commits only
    #[clap(short, long, group = "check-mode")]
    check_commits: bool,
    
    /// Read commits from a file instead of stdin
    #[clap(long, requires = "check-mode")]
    commits: Option<PathBuf>,
    
    /// Follow "Fixes:" tags and reverts
    #[clap(long)]
    no_notices: bool,
}

fn read_commits<'a>(args: &Args, repo: &'a Repository, commit_list: &'a [Commit<'a>]) -> Vec<Commit<'a>> {
    let lines = if let Some(path) = &args.commits {
        read_lines_from_bufreader(File::open(path).unwrap())
    } else {
        read_lines_from_bufreader(io::stdin())
    };
    
    let title_mapping = commit_list
        .iter()
        .enumerate()
        .map(|(idx, commit)| (commit.summary().unwrap_or("<no title>"), idx))
        .collect::<HashMap<_, _>>();
    
    lines
        .iter()
        .flat_map(|line| parse_commit_description(line, repo, commit_list, &title_mapping))
        .collect()
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
    
    if args.check_commits {
        let inspected_commits = ref_graph.get_commits(&repo);
        let commits = read_commits(&args, &repo, &inspected_commits);
        let have_commits = commits
            .iter()
            .map(Commit::id)
            .collect::<HashSet<_>>();
        let mut found_new_commits: HashSet<Oid> = HashSet::new();
        
        for commit in &commits {
            let fixed: Vec<Oid> = ref_graph.get_references(commit.id(), args.no_notices)
                .into_iter()
                .filter(|oid| !have_commits.contains(oid))
                .collect();
            if !fixed.is_empty() {
                println!(
                    "Commit {} (\"{}\") has the following references (maybe indirect):",
                    commit.id(),
                    commit.summary().unwrap_or("<no title>")
                );
                for reference in &fixed {
                    println!(
                        "    {} (\"{}\")", 
                        reference, 
                        repo.find_commit(*reference)
                            .unwrap()
                            .summary()
                            .unwrap_or("<no summary>"),
                    )
                }
            }
            found_new_commits.extend(fixed.into_iter());
        }
        println!("Summary: found {} probably missing commits", found_new_commits.len());
    } else {
        ref_graph.dump_info(&repo, args.no_notices);
    }
}
