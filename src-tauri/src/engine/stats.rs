//! "Who Did What" collaboration metrics.
//!
//! Commit counts per author are computed for real from history. Ownership (`files_owned`)
//! and review throughput (`reviews`) are placeholders in the scaffold — a later pass can
//! populate them from blame/attribution and the remote review provider without changing
//! the IPC contract.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ContributorStat {
    pub author_name: String,
    pub author_email: String,
    pub commits: u32,
    pub files_owned: u32,
    pub reviews: u32,
}

/// Aggregate commits per author across reachable history from HEAD.
pub fn contributor_stats(path: &str) -> AppResult<Vec<ContributorStat>> {
    let repo = git2::Repository::discover(path)?;
    let mut walk = repo.revwalk()?;
    walk.set_sorting(git2::Sort::TIME)?;
    let _ = walk.push_head();

    let mut by_email: HashMap<String, ContributorStat> = HashMap::new();
    for oid in walk.flatten() {
        if let Ok(commit) = repo.find_commit(oid) {
            let author = commit.author();
            let email = author.email().unwrap_or_default().to_string();
            let name = author.name().unwrap_or_default().to_string();
            by_email
                .entry(email.clone())
                .or_insert(ContributorStat {
                    author_name: name,
                    author_email: email,
                    commits: 0,
                    files_owned: 0, // TODO: derive from blame attribution
                    reviews: 0,     // TODO: derive from review provider
                })
                .commits += 1;
        }
    }

    let mut stats: Vec<ContributorStat> = by_email.into_values().collect();
    stats.sort_by(|a, b| b.commits.cmp(&a.commits));
    Ok(stats)
}
