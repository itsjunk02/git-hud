//! Git write operations via `git2` (libgit2): staging, unstaging, and committing.
//!
//! Pure logic, decoupled from Tauri (like [`git`](super::git)). Per-repo write
//! serialization and the post-write cache/UI reconcile live in the command layer
//! (`lib.rs`); here we only touch the repository. No `git` CLI subprocess, no network.

use std::path::Path;

use git2::{IndexAddOption, Repository};

use crate::error::{AppError, AppResult};

/// Open a repository, discovering the `.git` dir from `path` (walks up if needed).
fn open(path: &str) -> AppResult<Repository> {
    Repository::discover(path).map_err(AppError::from)
}

/// Stage the given paths — mirrors `git add <pathspec>`, so new, modified, **and deleted**
/// files are all staged.
pub fn stage_paths(path: &str, paths: &[String]) -> AppResult<()> {
    let repo = open(path)?;
    let mut index = repo.index()?;
    index.add_all(
        paths.iter().map(String::as_str),
        IndexAddOption::DEFAULT,
        None,
    )?;
    index.write()?;
    Ok(())
}

/// Unstage the given paths — mirrors `git restore --staged <path>` (reset the index entry to
/// HEAD). On an unborn branch (no commits yet) "unstaging" means dropping the index entry.
pub fn unstage_paths(path: &str, paths: &[String]) -> AppResult<()> {
    let repo = open(path)?;
    match repo.head() {
        Ok(head) => {
            let commit = head.peel_to_commit()?;
            repo.reset_default(Some(commit.as_object()), paths.iter().map(String::as_str))?;
        }
        Err(_) => {
            // Unborn branch: there's no HEAD to reset to, so remove the entries.
            let mut index = repo.index()?;
            for p in paths {
                let _ = index.remove_path(Path::new(p));
            }
            index.write()?;
        }
    }
    Ok(())
}

/// Commit the staged tree. When `amend` is set, rewrites HEAD instead — but only if that's
/// safe (HEAD not yet pushed). Finalizes an in-progress merge when `MERGE_HEAD` is present.
/// Returns the new commit's OID.
pub fn commit(path: &str, message: &str, amend: bool) -> AppResult<String> {
    let mut repo = open(path)?;

    // Author/committer from git config (local → global). Clear error if the user has no identity.
    let sig = repo.signature().map_err(|_| {
        AppError::Message("set your git identity first: git config user.name / user.email".into())
    })?;

    // Collect any in-progress merge heads first (needs `&mut repo`, so before other borrows).
    let mut merge_oids: Vec<git2::Oid> = Vec::new();
    let _ = repo.mergehead_foreach(|oid| {
        merge_oids.push(*oid);
        true
    });

    // Build the tree from the current index (the staged state).
    let tree_oid = {
        let mut index = repo.index()?;
        if index.has_conflicts() {
            return Err(AppError::Message("resolve conflicts before committing".into()));
        }
        index.write_tree()?
    };
    let tree = repo.find_tree(tree_oid)?;
    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());

    if amend {
        let head_commit =
            head_commit.ok_or_else(|| AppError::Message("nothing to amend".into()))?;
        // Safety: refuse to rewrite a commit that's already on the upstream.
        // `ahead_behind` → None when there's no upstream (safe); Some((0, _)) means the tip is
        // already pushed; Some((>=1, _)) means it's a local-only commit (safe to amend).
        if matches!(super::git::ahead_behind(&repo), Some((0, _))) {
            return Err(AppError::Message(
                "HEAD is already pushed to its upstream; amend would rewrite published history"
                    .into(),
            ));
        }
        let msg = if message.trim().is_empty() {
            head_commit.message().unwrap_or("").to_string()
        } else {
            message.to_string()
        };
        let oid =
            head_commit.amend(Some("HEAD"), Some(&sig), Some(&sig), None, Some(&msg), Some(&tree))?;
        return Ok(oid.to_string());
    }

    if message.trim().is_empty() {
        return Err(AppError::Message("commit message is empty".into()));
    }

    // Refuse an empty commit (tree unchanged vs. the sole parent), unless finalizing a merge.
    if merge_oids.is_empty() {
        let unchanged = match &head_commit {
            Some(hc) => hc.tree_id() == tree_oid,
            None => tree.is_empty(),
        };
        if unchanged {
            return Err(AppError::Message("nothing staged to commit".into()));
        }
    }

    // Parents: HEAD (if any), then every merge head.
    let merge_commits: Vec<git2::Commit> = merge_oids
        .iter()
        .filter_map(|o| repo.find_commit(*o).ok())
        .collect();
    let mut parents: Vec<&git2::Commit> = Vec::new();
    if let Some(hc) = &head_commit {
        parents.push(hc);
    }
    parents.extend(merge_commits.iter());

    let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;
    let was_merge = !merge_oids.is_empty();
    // Drop the borrows of `repo` before the (possibly `&mut self`) state cleanup.
    drop(parents);
    drop(merge_commits);
    drop(head_commit);
    drop(tree);
    if was_merge {
        let _ = repo.cleanup_state(); // clear MERGE_HEAD / MERGE_MSG after finalizing the merge
    }
    Ok(oid.to_string())
}

#[cfg(test)]
mod tests {
    fn init_repo() -> (git2::Repository, std::path::PathBuf) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("githud-writes-{unique}"));
        let repo = git2::Repository::init(&dir).unwrap();
        {
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", "Test User").unwrap();
            cfg.set_str("user.email", "test@example.com").unwrap();
        }
        (repo, dir)
    }

    #[test]
    fn stage_commit_unstage_cycle_and_both_sides() {
        let (_repo, dir) = init_repo();
        let path = dir.to_str().unwrap();
        let find = |p: &str| {
            crate::engine::git::status(path)
                .unwrap()
                .files
                .into_iter()
                .find(|f| f.path == p)
                .unwrap()
        };

        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        super::stage_paths(path, &["a.txt".to_string()]).unwrap();
        let a = find("a.txt");
        assert_eq!(a.index_status, "new");
        assert_eq!(a.worktree_status, "unmodified");

        let oid = super::commit(path, "add a", false).unwrap();
        assert_eq!(oid.len(), 40);
        // Nothing staged now → empty commit refused.
        assert_eq!(
            super::commit(path, "noop", false).unwrap_err().to_string(),
            "nothing staged to commit"
        );

        // Stage a modification, then edit again → file shows on BOTH sides.
        std::fs::write(dir.join("a.txt"), "hello v2").unwrap();
        super::stage_paths(path, &["a.txt".to_string()]).unwrap();
        std::fs::write(dir.join("a.txt"), "hello v3").unwrap();
        let a = find("a.txt");
        assert_eq!(a.index_status, "modified");
        assert_eq!(a.worktree_status, "modified");

        // Unstage → index side clears, worktree edit remains.
        super::unstage_paths(path, &["a.txt".to_string()]).unwrap();
        let a = find("a.txt");
        assert_eq!(a.index_status, "unmodified");
        assert_eq!(a.worktree_status, "modified");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stages_a_deletion() {
        let (_repo, dir) = init_repo();
        let path = dir.to_str().unwrap();
        std::fs::write(dir.join("f.txt"), "x").unwrap();
        super::stage_paths(path, &["f.txt".to_string()]).unwrap();
        super::commit(path, "add f", false).unwrap();

        std::fs::remove_file(dir.join("f.txt")).unwrap();
        super::stage_paths(path, &["f.txt".to_string()]).unwrap();
        let f = crate::engine::git::status(path)
            .unwrap()
            .files
            .into_iter()
            .find(|f| f.path == "f.txt")
            .unwrap();
        assert_eq!(f.index_status, "deleted");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn amend_blocked_when_tip_is_pushed() {
        let (repo, dir) = init_repo();
        let path = dir.to_str().unwrap();

        std::fs::write(dir.join("a.txt"), "1").unwrap();
        super::stage_paths(path, &["a.txt".to_string()]).unwrap();
        super::commit(path, "c1", false).unwrap();
        std::fs::write(dir.join("b.txt"), "2").unwrap();
        super::stage_paths(path, &["b.txt".to_string()]).unwrap();
        super::commit(path, "c2", false).unwrap();

        let head = repo.head().unwrap().target().unwrap();
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();

        // Pretend the tip is already pushed: origin/<branch> == HEAD → ahead == 0 → refuse.
        repo.reference(&format!("refs/remotes/origin/{branch}"), head, true, "push")
            .unwrap();
        std::fs::write(dir.join("b.txt"), "2b").unwrap();
        super::stage_paths(path, &["b.txt".to_string()]).unwrap();
        assert!(super::commit(path, "amend", true)
            .unwrap_err()
            .to_string()
            .contains("already pushed"));

        // Rewind origin one commit → HEAD is ahead by 1 → amend allowed.
        let parent = repo.find_commit(head).unwrap().parent(0).unwrap().id();
        repo.reference(&format!("refs/remotes/origin/{branch}"), parent, true, "rewind")
            .unwrap();
        let amended = super::commit(path, "c2 amended", true).unwrap();
        assert_eq!(amended.len(), 40);
        assert_ne!(amended, head.to_string());

        std::fs::remove_dir_all(&dir).ok();
    }
}
