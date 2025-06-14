use std::cell::RefCell;
use std::cmp::max;
use std::collections::HashMap;
use git2::{Error, Oid, Repository};
use log::{error, info, warn};
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
                    error!(
                        "Commit {} references a commit that cannot be found!\n
                        Hash: {}\n
                        Title: {}",
                        oid, referenced.hash, referenced.title);
                    continue;
                };
                let Some(ref_id) = graph.lookup(&ref_commit.id()) else {
                    warn!(
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
                info!("Adding ref: {ref_id} -> {id}, type {:?}", t);
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
    
    fn dfs(&self, v: usize) -> Vec<usize> {
        let mut dfs = vec![v];
        let mut result = vec![];
        let mut flag = self.flag.borrow_mut();
        let mut visited = self.visited.borrow_mut();

        *flag += 1;
        visited[v] = *flag;

        while let Some(v) = dfs.pop() {
            for &(u, _) in &self.referenced_by[v] {
                if visited[u] != *flag {
                    dfs.push(u);
                    // To ignore the starting node
                    result.push(u);
                    visited[u] = *flag;
                }
            }
        }
        result
    }
    
    fn get_references_by_id(&self, v: usize) -> Vec<Oid> {
        self.dfs(v)
            .iter()
            .map(|&u| self.id_to_hash[u])
            .collect()
    }

    pub fn get_references(&self, oid: Oid) -> Vec<Oid> {
        let Some(v) = self.lookup(&oid) else {
            warn!("Commit with hash {} not found, someone may still blame it", oid);
            return vec![];
        };
        self.get_references_by_id(v)
    }
    
    pub fn dump_info(&self, repo: &Repository) {
        for i in 0..self.referenced_by.len() {
            let oid = &self.id_to_hash[i];
            let referenced_by = self.get_references_by_id(i);
            if referenced_by.is_empty() {
                info!("Commit {oid} (\"{}\") is not mentioned anywhere", 
                    repo.find_commit(*oid).unwrap().summary().unwrap());
            } else {
                info!("Found references of commit {oid} (\"{}\")", 
                    repo.find_commit(*oid).unwrap().summary().unwrap());
                for ref_oid in referenced_by {
                    warn!("  {ref_oid} (\"{}\")", 
                        repo.find_commit(ref_oid).unwrap().summary().unwrap());
                }
            }
        }
    }
}