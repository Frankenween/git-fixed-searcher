use git2::{Commit, Repository};
use lazy_static::lazy_static;
use log::warn;
use regex::Regex;

lazy_static! {
    // NOTE: commit messages contain all symbols except '"'.
    // NOTE: Need this because the commit message is flattened in one line.
    static ref commit_ref_regex: Regex =
        Regex::new(r#"(Fixes: )?([0-9a-f]{8,}) ?\("([^"]+)"\)"#).unwrap();
    static ref revert_header: Regex =
        Regex::new(r#"Revert "([^"]+)""#).unwrap();
    static ref revert_hash: Regex =
        Regex::new(r#"This reverts commit ([0-9a-f]+)"#).unwrap();
}

pub struct RefEntry {
    pub hash: String,
    pub title: String,
    pub blame: bool,
}

pub fn extract_revert(commit: &Commit) -> Option<RefEntry> {
    let header_capture = revert_header.captures(commit.summary()?)?;
    let hash_capture = revert_hash.captures(commit.message()?)?;
    Some(RefEntry {
        hash: hash_capture[1].to_string(),
        title: header_capture[1].to_string(),
        blame: true,
    })
}

pub fn extract_references(commit: &Commit) -> Vec<RefEntry> {
    // TODO: return unique references
    // New lines can happen inside commit refs
    let flatten_msg = commit.message().unwrap().replace("\n", " ");
    commit_ref_regex
        .captures_iter(&flatten_msg)
        .map(|captured| {
            let fixed = captured.get(1).is_some();
            let hash = captured[2].to_string();
            let title = captured[3].to_string();
            RefEntry { hash, title, blame: fixed }
        })
        .chain(extract_revert(commit))
        .collect()
}

pub fn get_commit_by_ref_entry<'a>(repo: &'a Repository, ref_entry: &RefEntry) -> Option<Commit<'a>> {
    let found = repo.find_commit_by_prefix(&ref_entry.hash).ok();
    found.inspect(|commit| {
        if commit.summary().is_none() || ref_entry.title != commit.summary().unwrap() {
            warn!("\
            Found commit with same hash but different title!\n\
            Real hash: {}, entry hash {}\n\
            Real title: {}\n\
            Entry title: {}\
            ", commit.id(), ref_entry.hash, commit.summary().unwrap_or("None"), ref_entry.title);
        }
    })
}
