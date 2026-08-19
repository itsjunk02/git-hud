//! Real Git repository parsing via `git2` (libgit2). All reads are performed directly
//! against the on-disk object database — there are no `git` CLI subprocess calls.

use git2::{BranchType, Repository, Sort};

use super::model::{BranchInfo, CommitInfo, FileStatus, RepoStatus, RepoSummary};
use crate::error::{AppError, AppResult};

/// Open a repository, discovering the `.git` dir from `path` (walks up if needed).
fn open(path: &str) -> AppResult<Repository> {
    Repository::discover(path).map_err(AppError::from)
}

/// Cheap summary used right after opening a repo (HEAD branch + rough counts).
pub fn summarize(path: &str) -> AppResult<RepoSummary> {
    let repo = open(path)?;
    let head_branch = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(String::from).ok());

    let mut commit_count = 0u32;
    if let Ok(mut walk) = repo.revwalk() {
        if walk.push_head().is_ok() {
            commit_count = walk.count() as u32;
        }
    }

    let branch_count = repo
        .branches(Some(BranchType::Local))
        .map(|b| b.count() as u32)
        .unwrap_or(0);

    Ok(RepoSummary {
        path: path.to_string(),
        head_branch,
        commit_count,
        branch_count,
    })
}

/// Walk history (HEAD + all local branches) and assign each commit a graph lane.
///
/// The lane algorithm is a compact single-pass allocator: each lane tracks the OID it
/// expects to render next. A commit takes the lane reserved for it (or a fresh one);
/// its first parent continues that lane, and additional parents reserve new lanes. This
/// yields a stable, readable braid for the Canvas timeline without a heavy layout pass.
pub fn list_commits(path: &str, limit: usize) -> AppResult<Vec<CommitInfo>> {
    let repo = open(path)?;
    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    // Seed the walk with HEAD and every local branch tip so parallel branches are visible.
    let _ = walk.push_head();
    if let Ok(branches) = repo.branches(Some(BranchType::Local)) {
        for branch in branches.flatten() {
            if let Some(oid) = branch.0.get().target() {
                let _ = walk.push(oid);
            }
        }
    }

    // Each slot holds the OID currently expected to appear next in that lane.
    let mut lanes: Vec<Option<git2::Oid>> = Vec::new();
    let mut out: Vec<CommitInfo> = Vec::new();

    for oid in walk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;

        // Find the lane reserved for this commit, or allocate one.
        let lane = match lanes.iter().position(|slot| *slot == Some(oid)) {
            Some(idx) => idx,
            None => allocate_lane(&mut lanes, oid),
        };

        let parents: Vec<git2::Oid> = commit.parent_ids().collect();

        // First parent continues in this lane; the lane frees up if the commit is a root.
        lanes[lane] = parents.first().copied();

        // Extra parents (merges) each reserve their own lane if not already tracked.
        for &parent in parents.iter().skip(1) {
            if !lanes.iter().any(|slot| *slot == Some(parent)) {
                allocate_lane(&mut lanes, parent);
            }
        }

        let author = commit.author();
        let id_hex = oid.to_string();
        out.push(CommitInfo {
            short_id: id_hex.chars().take(8).collect(),
            id: id_hex,
            summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
            body: commit.body().ok().flatten().unwrap_or("").to_string(),
            author_name: author.name().unwrap_or("").to_string(),
            author_email: author.email().unwrap_or("").to_string(),
            timestamp: commit.time().seconds() as f64,
            parent_ids: parents.iter().map(git2::Oid::to_string).collect(),
            lane: lane as u32,
        });

        if out.len() >= limit {
            break;
        }
    }

    Ok(out)
}

/// Reserve a lane for `oid`, reusing a freed slot when possible.
fn allocate_lane(lanes: &mut Vec<Option<git2::Oid>>, oid: git2::Oid) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(idx) => {
            lanes[idx] = Some(oid);
            idx
        }
        None => {
            lanes.push(Some(oid));
            lanes.len() - 1
        }
    }
}

/// List local and remote branches.
pub fn list_branches(path: &str) -> AppResult<Vec<BranchInfo>> {
    let repo = open(path)?;
    let mut out = Vec::new();
    for entry in repo.branches(None)? {
        let (branch, kind) = entry?;
        let name = branch.name()?.unwrap_or("").to_string();
        out.push(BranchInfo {
            is_head: branch.is_head(),
            is_remote: matches!(kind, BranchType::Remote),
            target: branch.get().target().map(|o| o.to_string()),
            name,
        });
    }
    Ok(out)
}

/// Working-tree + HEAD status, including conflict detection.
pub fn status(path: &str) -> AppResult<RepoStatus> {
    let repo = open(path)?;
    let head_branch = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().map(String::from).ok());

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut opts))?;

    let mut files = Vec::new();
    let mut has_conflicts = false;
    for entry in statuses.iter() {
        let s = entry.status();
        if s.is_conflicted() {
            has_conflicts = true;
        }
        let (label, staged) = describe_status(s);
        files.push(FileStatus {
            path: entry.path().unwrap_or_default().to_string(),
            status: label,
            staged,
        });
    }

    Ok(RepoStatus {
        head_branch,
        files,
        ahead: 0,
        behind: 0,
        has_conflicts,
    })
}

/// Map a `git2::Status` bitset to a human label and a "staged?" flag.
fn describe_status(s: git2::Status) -> (String, bool) {
    use git2::Status as St;
    let staged = s.intersects(
        St::INDEX_NEW
            | St::INDEX_MODIFIED
            | St::INDEX_DELETED
            | St::INDEX_RENAMED
            | St::INDEX_TYPECHANGE,
    );
    let label = if s.is_conflicted() {
        "conflicted"
    } else if s.intersects(St::INDEX_NEW | St::WT_NEW) {
        "new"
    } else if s.intersects(St::INDEX_DELETED | St::WT_DELETED) {
        "deleted"
    } else if s.intersects(St::INDEX_RENAMED | St::WT_RENAMED) {
        "renamed"
    } else if s.intersects(St::INDEX_TYPECHANGE | St::WT_TYPECHANGE) {
        "typechange"
    } else if s.intersects(St::INDEX_MODIFIED | St::WT_MODIFIED) {
        "modified"
    } else if s.is_ignored() {
        "ignored"
    } else {
        "unmodified"
    };
    (label.to_string(), staged)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Build a throwaway repo with two commits and exercise the real read path.
    #[test]
    fn reads_real_repository() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("githud-test-{unique}"));
        let repo = git2::Repository::init(&dir).unwrap();
        let sig = git2::Signature::now("Test User", "test@example.com").unwrap();

        let commit_file = |name: &str, msg: &str, parents: &[git2::Oid]| -> git2::Oid {
            std::fs::write(dir.join(name), name).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new(name)).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parent_commits: Vec<git2::Commit> =
                parents.iter().map(|o| repo.find_commit(*o).unwrap()).collect();
            let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();
            repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parent_refs)
                .unwrap()
        };

        let first = commit_file("a.txt", "first commit", &[]);
        commit_file("b.txt", "second commit", &[first]);

        let path = dir.to_str().unwrap();

        let commits = super::list_commits(path, 100).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].summary, "second commit");
        assert_eq!(commits[0].parent_ids.len(), 1);

        let summary = super::summarize(path).unwrap();
        assert_eq!(summary.commit_count, 2);
        assert!(summary.head_branch.is_some());

        let branches = super::list_branches(path).unwrap();
        assert!(branches.iter().any(|b| b.is_head));

        let status = super::status(path).unwrap();
        assert!(!status.has_conflicts);

        std::fs::remove_dir_all(&dir).ok();
    }
}
