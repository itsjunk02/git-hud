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
    /// GitHub-style handle for display, derived from the commit email (falling back
    /// to the git author name). See [`derive_username`].
    pub username: String,
    pub commits: u32,
    pub files_owned: u32,
    pub reviews: u32,
}

/// Derive a GitHub-style username for display from a commit's author identity.
///
/// Commits only carry a free-form name + email, so we prefer the email as the
/// stable handle: GitHub noreply addresses (`id+login@users.noreply.github.com`
/// or `login@users.noreply.github.com`) embed the real login, and every other
/// address falls back to its local-part. When the email is missing we use the
/// git author name, and as a last resort "unknown" — so a commit authored with
/// `user.name=unknown` still resolves to a real handle via its email.
pub(crate) fn derive_username(name: &str, email: &str) -> String {
    let email = email.trim();
    if !email.is_empty() {
        if let Some(local) = email.strip_suffix("@users.noreply.github.com") {
            // `149812235+itsjunk02` -> `itsjunk02`; plain `login` stays `login`.
            let login = local.rsplit('+').next().unwrap_or(local);
            if !login.is_empty() {
                return login.to_string();
            }
        }
        if let Some((local, _)) = email.split_once('@') {
            if !local.is_empty() {
                return local.to_string();
            }
        }
    }
    let name = name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    "unknown".to_string()
}

/// Aggregate commits per author across all branches (local and remote-tracking).
///
/// Seeds the walk with HEAD and every branch tip so contributors who have only
/// committed on unmerged feature branches (or branches fetched from the remote)
/// are counted, matching what the Timeline shows. Counting only HEAD-reachable
/// history hides a new collaborator until their branch lands on the current branch.
pub fn contributor_stats(path: &str) -> AppResult<Vec<ContributorStat>> {
    let repo = git2::Repository::discover(path)?;
    let mut walk = repo.revwalk()?;
    walk.set_sorting(git2::Sort::TIME)?;
    let _ = walk.push_head();
    if let Ok(branches) = repo.branches(None) {
        for branch in branches.flatten() {
            if let Some(oid) = branch.0.get().target() {
                let _ = walk.push(oid);
            }
        }
    }

    let mut by_email: HashMap<String, ContributorStat> = HashMap::new();
    for oid in walk.flatten() {
        if let Ok(commit) = repo.find_commit(oid) {
            let author = commit.author();
            let email = author.email().unwrap_or_default().to_string();
            let name = author.name().unwrap_or_default().to_string();
            let username = derive_username(&name, &email);
            by_email
                .entry(email.clone())
                .or_insert(ContributorStat {
                    author_name: name,
                    author_email: email,
                    username,
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// A contributor who has only committed on an unmerged branch (not reachable
    /// from HEAD) must still be counted — the Timeline shows them, so Contributors
    /// must too. Regression for contributors missing until their branch lands.
    #[test]
    fn counts_contributors_on_unmerged_branches() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("githud-stats-{unique}"));
        let repo = git2::Repository::init(&dir).unwrap();

        let commit_file = |name: &str, sig: &git2::Signature, r#ref: Option<&str>, parents: &[git2::Oid]| -> git2::Oid {
            std::fs::write(dir.join(name), name).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new(name)).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parent_commits: Vec<git2::Commit> =
                parents.iter().map(|o| repo.find_commit(*o).unwrap()).collect();
            let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();
            repo.commit(r#ref, sig, sig, name, &tree, &parent_refs).unwrap()
        };

        // Author on the current branch (reachable from HEAD).
        let alice = git2::Signature::now("Alice", "alice@example.com").unwrap();
        let base = commit_file("a.txt", &alice, Some("HEAD"), &[]);

        // New collaborator commits only on an unmerged feature branch, never on HEAD.
        let dave = git2::Signature::now("Dave", "dave@example.com").unwrap();
        commit_file("b.txt", &dave, Some("refs/heads/feature"), &[base]);

        let stats = super::contributor_stats(dir.to_str().unwrap()).unwrap();
        let emails: Vec<&str> = stats.iter().map(|s| s.author_email.as_str()).collect();
        assert!(emails.contains(&"alice@example.com"), "HEAD author missing");
        assert!(
            emails.contains(&"dave@example.com"),
            "contributor on unmerged branch was dropped: {emails:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn derives_github_style_usernames_from_email() {
        // GitHub noreply with numeric id prefix -> embedded login.
        assert_eq!(
            super::derive_username("Akhil", "149812235+itsjunk02@users.noreply.github.com"),
            "itsjunk02"
        );
        // Plain GitHub noreply -> login.
        assert_eq!(
            super::derive_username("", "octocat@users.noreply.github.com"),
            "octocat"
        );
        // Ordinary email -> local-part, even when a real git name is present.
        assert_eq!(super::derive_username("iAkhil01", "itsjunking@gmail.com"), "itsjunking");
        // The reported bug: git name literally "unknown" -> email local-part, not "unknown".
        assert_eq!(super::derive_username("unknown", "admingit@proton.me"), "admingit");
        // No email -> fall back to the git name.
        assert_eq!(super::derive_username("Solo Dev", ""), "Solo Dev");
        // Nothing usable -> "unknown".
        assert_eq!(super::derive_username("", ""), "unknown");
    }
}
