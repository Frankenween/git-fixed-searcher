use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
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
    static ref config_hash_and_msg: Regex =
        Regex::new(r#"([0-9a-f]{8,}) ?\("([^"]+)"\)""#).unwrap();
    static ref config_hash: Regex =
        Regex::new(r"[0-9a-f]{8,}").unwrap();
}

#[derive(Eq, PartialEq, Debug, Copy, Clone, Ord, PartialOrd)]
pub enum RefType {
    Note,
    Fix,
    Revert,
}

pub struct RefEntry {
    pub hash: String,
    pub title: String,
    pub ref_type: RefType,
}

pub fn extract_revert(commit: &Commit) -> Option<RefEntry> {
    let header_capture = revert_header.captures(commit.summary()?)?;
    let hash_capture = revert_hash.captures(commit.message()?)?;
    Some(RefEntry {
        hash: hash_capture[1].to_string(),
        title: header_capture[1].to_string(),
        ref_type: RefType::Revert,
    })
}

pub fn extract_references(commit: &Commit) -> Vec<RefEntry> {
    // New lines can happen inside commit refs
    let flatten_msg = commit.message().unwrap().replace("\n", " ");
    commit_ref_regex
        .captures_iter(&flatten_msg)
        .map(|captured| {
            let ref_type = if captured.get(1).is_some() {
                RefType::Fix
            } else {
                RefType::Note
            };
            let hash = captured[2].to_string();
            let title = captured[3].to_string();
            RefEntry { hash, title, ref_type }
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
            ", commit.id(), ref_entry.hash, commit.summary().unwrap_or("<no summary>"), ref_entry.title);
        }
    })
}

pub fn read_lines_from_bufreader(reader: impl Read) -> Vec<String> {
    BufReader::new(reader)
        .lines()
        .map_while(Result::ok)
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>()
}

/// Check that two titles are sort of the same
/// Return true if `real_title` is a substring of `got_title` or vice versa
/// If they are not equal, log a warning
fn check_commit_titles(real_title: &str, got_title: &str, verbose: bool) -> bool {
    if real_title == got_title {
        true
    } else if got_title.contains(real_title) || real_title.contains(got_title) {
        if verbose {
            warn!(
                "Titles look similar, but not equal\n\
                Commit title: {}\n\
                Checking title: {}",
                real_title, got_title
            );
        }
        true
    } else {
        if verbose {
            warn!(
                "Huge title mismatch\n\
                Commit title: {}\n\
                Checking title: {}",
                real_title, got_title
            );
        }
        false
    }
}

pub fn parse_commit_description<'a>(
    line: &str, 
    repo: &'a Repository, 
    commit_list: &[Commit<'a>],
    title_mapping: &HashMap<&'a str, usize>,
) -> Option<Commit<'a>> {
    if let Some(cap) = config_hash_and_msg.captures(line) {
        let hash = cap.get(1).unwrap().as_str();
        let title = cap.get(2).unwrap().as_str();
        // Verify that hash is valid
        if let Ok(commit) = repo.find_commit_by_prefix(hash) {
            // Verify that message is sane
            check_commit_titles(commit.summary().unwrap_or("<no summary>"), title, true);
            Some(commit)
        } else {
            warn!("Commit with hash {hash} was not found in repository");
            None
        }
    } else if config_hash.is_match(line) {
        if let Ok(commit) = repo.find_commit_by_prefix(line) {
            Some(commit)
        } else {
            warn!("Commit with hash {line} was not found in repository");
            None
        }
    } else {
        // It is a commit description, check if there is exactly the same title
        // If no - iterate over all and check(Aho-Corasick algorithm would be nice here)
        if let Some(idx) = title_mapping.get(line) {
            return Some(commit_list[*idx].clone());
        }
        commit_list
            .iter()
            .find_map(|commit|
                if commit.summary().is_some_and(|s| 
                    check_commit_titles(s, line, false) || check_commit_titles(line, s, false)
                ) {
                    Some(commit.clone())
                } else {
                    None
                }
            )
    }
}
