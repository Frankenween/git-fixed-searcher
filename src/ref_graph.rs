use std::cell::RefCell;
use std::cmp::max;
use std::collections::HashMap;
use git2::{Commit, Error, Oid, Repository};
use log::{debug, info, warn};
use crate::util::{extract_references, get_commit_by_ref_entry, RefType};

pub struct RefGraph {
    referenced_by: Vec<Vec<(usize, RefType)>>,
    hash_to_id: HashMap<Oid, usize>,
    id_to_hash: Vec<Oid>,
    // DFS internal info
    flag: RefCell<u64>,
    visited: RefCell<Vec<u64>>,
}

impl RefGraph {
    // Commits should go in a commit order - from oldest to newest one
    pub fn new(repo: &Repository, commits: impl Iterator<Item=Result<Oid, Error>>) -> RefGraph {
        let mut graph = RefGraph {
            referenced_by: vec![],
            hash_to_id: HashMap::new(),
            id_to_hash: vec![],
            flag: RefCell::new(0),
            visited: RefCell::new(vec![]),
        };
        for oid in commits.flatten() {
            let id = graph.lookup_or_alloc(&oid);
            let mut added_edges: Vec<(usize, RefType)> = vec![];
            for referenced in extract_references(&repo.find_commit(oid).unwrap()) {
                let Some(ref_commit) = get_commit_by_ref_entry(repo, &referenced) else {
                    warn!(
                        "Commit {} references a commit that cannot be found!\n\
                        Hash: {}\n\
                        Title: {}",
                        oid, referenced.hash, referenced.title);
                    continue;
                };
                let Some(ref_id) = graph.lookup(&ref_commit.id()) else {
                    info!(
                        "Commit {} references a commit {}, that has not been observed.\n\
                        It is outside the search region or the commit order is wrong.",
                        oid, ref_commit.id()
                    );
                    continue;
                };
                if let Some(idx) = added_edges
                    .iter()
                    .position(|x| x.0 == ref_id) {
                    added_edges[idx].1 = max(added_edges[idx].1, referenced.ref_type);
                } else {
                    added_edges.push((ref_id, referenced.ref_type));
                }
            }
            for (ref_id, t) in added_edges {
                graph.referenced_by[ref_id].push((id, t));
                debug!("Adding ref: {ref_id} -> {id}, type {:?}", t);
            }
        }
        
        graph
    }

    fn lookup(&self, oid: &Oid) -> Option<usize> {
        self.hash_to_id.get(oid).cloned()
    }

    fn lookup_or_alloc(&mut self, oid: &Oid) -> usize {
        self.lookup(oid).unwrap_or_else(|| {
            let id = self.id_to_hash.len();
            self.referenced_by.push(Vec::new());
            self.id_to_hash.push(*oid);
            self.hash_to_id.insert(*oid, id);
            self.visited.borrow_mut().push(0);
            id
        })
    }
    
    fn dfs(&self, v: usize, no_notices: bool) -> Vec<usize> {
        let mut dfs = vec![v];
        let mut result = vec![];
        let mut flag = self.flag.borrow_mut();
        let mut visited = self.visited.borrow_mut();

        *flag += 1;
        visited[v] = *flag;

        while let Some(v) = dfs.pop() {
            for &(u, t) in &self.referenced_by[v] {
                if visited[u] != *flag && t.should_follow(no_notices) {
                    dfs.push(u);
                    // To ignore the starting node
                    result.push(u);
                    visited[u] = *flag;
                }
            }
        }
        result
    }
    
    fn get_references_by_id(&self, v: usize, no_notices: bool) -> Vec<Oid> {
        self.dfs(v, no_notices)
            .iter()
            .map(|&u| self.id_to_hash[u])
            .collect()
    }

    pub fn get_references(&self, oid: Oid, no_notices: bool) -> Vec<Oid> {
        let Some(v) = self.lookup(&oid) else {
            info!("Commit with hash {} not found, someone may still blame it", oid);
            return vec![];
        };
        self.get_references_by_id(v, no_notices)
    }
    
    pub fn dump_info(&self, repo: &Repository, no_notices: bool) {
        for i in 0..self.referenced_by.len() {
            let oid = &self.id_to_hash[i];
            let referenced_by = self.get_references_by_id(i, no_notices);
            if referenced_by.is_empty() {
                println!("Commit {oid} (\"{}\") is not mentioned anywhere", 
                    repo.find_commit(*oid).unwrap().summary().unwrap_or("<no summary>"));
            } else {
                println!("Found references of commit {oid} (\"{}\")", 
                    repo.find_commit(*oid).unwrap().summary().unwrap_or("<no summary>"));
                for ref_oid in referenced_by {
                    println!("  {ref_oid} (\"{}\")", 
                        repo.find_commit(ref_oid).unwrap().summary().unwrap_or("<no summary>"));
                }
            }
        }
    }
    
    pub fn get_oids(&self) -> &[Oid] {
        &self.id_to_hash
    }
    
    pub fn get_commits<'a>(&self, repo: &'a Repository) -> Vec<Commit<'a>> {
        self.get_oids()
            .iter()
            .flat_map(|&oid| repo.find_commit(oid).ok())
            .collect()
    }
}