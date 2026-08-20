//! Merge-conflict inspection and resolution.
//!
//! A conflict is recorded in the index as up to three stages: stage 1 = ancestor/base,
//! stage 2 = ours, stage 3 = theirs. We read each stage's blob to show the sides, and
//! resolve by writing the chosen side back to the working tree and re-staging it.

use std::path::Path;

use git2::{IndexEntry, Repository};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{AppError, AppResult};

/// A conflicted file with its three-way sides (a side is empty when that stage is absent,
/// e.g. no ancestor in an add/add conflict) plus the current working-tree text, which after
/// a merge contains the `<<<<<<< / ======= / >>>>>>>` markers — the seed for manual editing.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ConflictHunk {
    pub file: String,
    pub ours: String,
    pub theirs: String,
    pub base: String,
    pub merged: String,
}

/// Decode an index stage's blob as text (lossy). Empty when the stage is absent. This is
/// for display only — resolution writes the exact blob bytes, so binary files are safe.
fn blob_text(repo: &Repository, entry: Option<&IndexEntry>) -> String {
    entry
        .and_then(|e| repo.find_blob(e.id).ok())
        .map(|b| String::from_utf8_lossy(b.content()).into_owned())
        .unwrap_or_default()
}

/// Read the current working-tree text for `file` (lossy). Empty if absent.
fn working_text(workdir: Option<&Path>, file: &str) -> String {
    workdir
        .map(|w| w.join(file))
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// The conflicted path from whichever stage is present.
fn conflict_path(conflict: &git2::IndexConflict) -> Option<String> {
    conflict
        .our
        .as_ref()
        .or(conflict.their.as_ref())
        .or(conflict.ancestor.as_ref())
        .and_then(|e| String::from_utf8(e.path.clone()).ok())
}

/// Enumerate conflicted files from the index, with each side's content.
pub fn list_conflicts(path: &str) -> AppResult<Vec<ConflictHunk>> {
    let repo = Repository::discover(path)?;
    let workdir = repo.workdir().map(|p| p.to_path_buf());
    let index = repo.index()?;

    let mut out = Vec::new();
    if let Ok(conflicts) = index.conflicts() {
        for conflict in conflicts.flatten() {
            let Some(file) = conflict_path(&conflict) else {
                continue;
            };
            let merged = working_text(workdir.as_deref(), &file);
            out.push(ConflictHunk {
                ours: blob_text(&repo, conflict.our.as_ref()),
                theirs: blob_text(&repo, conflict.their.as_ref()),
                base: blob_text(&repo, conflict.ancestor.as_ref()),
                merged,
                file,
            });
        }
    }
    Ok(out)
}

/// What to do with the working-tree file when resolving to a side.
enum Resolution {
    /// Write this blob's content and stage it.
    Write(git2::Oid),
    /// The chosen side removed the file — delete it and unstage.
    Delete,
}

/// Resolve a conflicted `file` by taking one whole side (`resolution` is `"ours"` or
/// `"theirs"`): write that side to the working tree, stage it, and clear the conflict.
/// A no-op if `file` isn't currently conflicted.
pub fn resolve_conflict(path: &str, file: &str, resolution: &str) -> AppResult<()> {
    let repo = Repository::discover(path)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Message("cannot resolve conflicts in a bare repository".into()))?
        .to_path_buf();
    let mut index = repo.index()?;

    // Decide the action under the immutable conflicts() borrow, then release it before
    // mutating the index (add_path/remove_path).
    let action = {
        let conflicts = index.conflicts()?;
        let mut action: Option<Resolution> = None;
        for conflict in conflicts.flatten() {
            if conflict_path(&conflict).as_deref() != Some(file) {
                continue;
            }
            let side = match resolution {
                "ours" => conflict.our,
                "theirs" => conflict.their,
                other => {
                    return Err(AppError::Message(format!(
                        "invalid conflict resolution: {other}"
                    )))
                }
            };
            action = Some(match side {
                Some(entry) => Resolution::Write(entry.id),
                None => Resolution::Delete,
            });
            break;
        }
        action
    };

    let Some(action) = action else {
        return Ok(()); // not conflicted (or already resolved) — idempotent
    };

    let target = workdir.join(file);
    match action {
        Resolution::Write(oid) => {
            let content = repo.find_blob(oid)?.content().to_vec();
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, content)?;
            // Staging the working file replaces the conflict stages with a resolved entry.
            index.add_path(Path::new(file))?;
        }
        Resolution::Delete => {
            let _ = std::fs::remove_file(&target); // best-effort
            index.remove_path(Path::new(file))?;
        }
    }
    index.write()?;
    Ok(())
}

/// Save an arbitrary hand-merged `content` for `file`: write it to the working tree and
/// stage it, which clears the conflict. Backs the manual "Save resolution" editor.
pub fn save_resolution(path: &str, file: &str, content: &str) -> AppResult<()> {
    let repo = Repository::discover(path)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| AppError::Message("cannot resolve conflicts in a bare repository".into()))?
        .to_path_buf();

    let target = workdir.join(file);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, content)?;

    let mut index = repo.index()?;
    index.add_path(Path::new(file))?;
    index.write()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// Build a temp repo with two branches editing the same line, merged to produce a real
    /// index conflict on `f.txt` (base "base", ours "ours", theirs "theirs").
    fn conflicted_repo() -> PathBuf {
        // Per-call counter avoids temp-dir collisions between parallel tests (coarse clock
        // resolution can make two `as_nanos()` samples identical).
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = format!(
            "{}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(format!("githud-conflict-{unique}"));
        let repo = git2::Repository::init(&dir).unwrap();
        let sig = git2::Signature::now("Test User", "test@example.com").unwrap();

        let write_commit = |content: &str, msg: &str, parent: Option<git2::Oid>| -> git2::Oid {
            std::fs::write(dir.join("f.txt"), content).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("f.txt")).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parents: Vec<git2::Commit> =
                parent.into_iter().map(|o| repo.find_commit(o).unwrap()).collect();
            let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
            repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parent_refs)
                .unwrap()
        };

        let base = write_commit("base\n", "base", None);
        let default_branch = repo.head().unwrap().shorthand().unwrap().to_string();

        repo.branch("feature", &repo.find_commit(base).unwrap(), false)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        write_commit("theirs\n", "theirs", Some(base));

        repo.set_head(&format!("refs/heads/{default_branch}")).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        write_commit("ours\n", "ours", Some(base));

        let feature = repo
            .find_branch("feature", git2::BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap();
        let annotated = repo.find_annotated_commit(feature.id()).unwrap();
        repo.merge(&[&annotated], None, None).unwrap();
        dir
    }

    #[test]
    fn detects_and_resolves_conflict() {
        let dir = conflicted_repo();
        let path = dir.to_str().unwrap();

        let conflicts = super::list_conflicts(path).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].file, "f.txt");
        assert_eq!(conflicts[0].ours, "ours\n");
        assert_eq!(conflicts[0].theirs, "theirs\n");
        assert_eq!(conflicts[0].base, "base\n");
        // The merged working-tree text carries the conflict markers (seed for manual edit).
        assert!(conflicts[0].merged.contains("<<<<<<<"));
        assert!(conflicts[0].merged.contains(">>>>>>>"));

        super::resolve_conflict(path, "f.txt", "theirs").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "theirs\n");
        assert!(super::list_conflicts(path).unwrap().is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn manual_save_resolution_clears_conflict() {
        let dir = conflicted_repo();
        let path = dir.to_str().unwrap();

        super::save_resolution(path, "f.txt", "hand merged\n").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "hand merged\n");
        assert!(super::list_conflicts(path).unwrap().is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }
}
