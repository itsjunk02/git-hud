//! Merge-conflict inspection and resolution.
//!
//! A conflict is recorded in the index as up to three stages: stage 1 = ancestor/base,
//! stage 2 = ours, stage 3 = theirs. We read each stage's blob to show the sides, and
//! resolve by writing the chosen side back to the working tree and re-staging it.
//!
//! We also attribute *who* is in conflict with *whom*: each `<<<<<<< / ======= / >>>>>>>`
//! region's ours-lines are blamed against `HEAD` and its theirs-lines against the
//! incoming tip (`MERGE_HEAD`/`CHERRY_PICK_HEAD`/`REBASE_HEAD`). A conflicting region can
//! be authored by *several* people per side, so attribution is a set of authors per side,
//! aggregated up into merge-wide who-vs-whom pairs.

use std::collections::BTreeMap;
use std::path::Path;

use git2::{BlameOptions, IndexEntry, Oid, Repository};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::engine::stats::derive_username;
use crate::error::{AppError, AppResult};

/// An author who owns some of the conflicting lines on one side, with the line count
/// they contributed (used to rank and to show the dominant owner).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ConflictAuthor {
    pub username: String,
    pub name: String,
    pub email: String,
    pub lines: u32,
}

/// One `<<<<<<< / ======= / >>>>>>>` region within a conflicted file, with the distinct
/// authors of each side's lines (from git blame). `approximate` is set when the region's
/// block text could not be located in a side's blob and attribution fell back to the
/// whole-side author set.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ConflictRegion {
    pub index: u32,
    pub ours_start: u32,
    pub ours_end: u32,
    pub theirs_start: u32,
    pub theirs_end: u32,
    pub ours_authors: Vec<ConflictAuthor>,
    pub theirs_authors: Vec<ConflictAuthor>,
    pub approximate: bool,
}

/// A conflicted file with its three-way sides (a side is empty when that stage is absent,
/// e.g. no ancestor in an add/add conflict) plus the current working-tree text, which after
/// a merge contains the `<<<<<<< / ======= / >>>>>>>` markers — the seed for manual editing.
/// `ours_authors`/`theirs_authors` are the file-level union across all regions.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ConflictHunk {
    pub file: String,
    pub ours: String,
    pub theirs: String,
    pub base: String,
    pub merged: String,
    pub ours_authors: Vec<ConflictAuthor>,
    pub theirs_authors: Vec<ConflictAuthor>,
    pub regions: Vec<ConflictRegion>,
}

/// A who-vs-whom pairing aggregated across the whole in-progress merge: `ours` (an author
/// on our side) conflicts with `theirs` (an author on the incoming side) across `regions`
/// conflict regions.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ConflictPair {
    pub ours: String,
    pub theirs: String,
    pub regions: u32,
}

/// Merge-level summary for the Conflicts banner: which branches, the two tip authors, and
/// the aggregated conflict pairs.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MergeConflictSummary {
    pub in_merge: bool,
    pub ours_branch: Option<String>,
    pub incoming_branch: Option<String>,
    pub ours_tip: Option<ConflictAuthor>,
    pub theirs_tip: Option<ConflictAuthor>,
    pub pairs: Vec<ConflictPair>,
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

/// Split blob text into lines the way git counts them: a single trailing newline does not
/// create an extra empty line, so line N here lines up with blame line N.
fn split_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    if lines.last().map(String::is_empty).unwrap_or(false) {
        lines.pop();
    }
    lines
}

/// A side's blob lines plus, for each line, its (name, email) author from blame.
struct SideBlame {
    lines: Vec<String>,
    authors: Vec<(String, String)>, // authors[i] = author of line i (0-based)
}

/// Blame `file` as of `newest` (HEAD for ours, the incoming tip for theirs) and pair each
/// line of `blob` with its author. Lines stay unattributed (empty author) when there is no
/// commit to blame against (e.g. an unborn HEAD or an add/add side).
fn side_blame(repo: &Repository, file: &str, newest: Option<Oid>, blob: &str) -> SideBlame {
    let lines = split_lines(blob);
    let mut authors = vec![(String::new(), String::new()); lines.len()];
    if let Some(oid) = newest {
        let mut opts = BlameOptions::new();
        opts.newest_commit(oid);
        if let Ok(blame) = repo.blame_file(Path::new(file), Some(&mut opts)) {
            for (i, slot) in authors.iter_mut().enumerate() {
                if let Some(hunk) = blame.get_line(i + 1) {
                    if let Some(sig) = hunk.final_signature() {
                        *slot = (
                            sig.name().unwrap_or_default().to_string(),
                            sig.email().unwrap_or_default().to_string(),
                        );
                    }
                }
            }
        }
    }
    SideBlame { lines, authors }
}

/// A conflict region parsed straight out of the merged working-tree text: the ours/theirs
/// block lines plus their 1-based line spans in that merged file (0 when a block is empty).
struct ParsedRegion {
    ours_block: Vec<String>,
    theirs_block: Vec<String>,
    ours_start: u32,
    ours_end: u32,
    theirs_start: u32,
    theirs_end: u32,
}

/// Parse `<<<<<<< / |||||||  / ======= / >>>>>>>` regions from merged conflict text. The
/// optional `|||||||` base block (diff3 style) is skipped.
fn parse_conflict_regions(merged: &str) -> Vec<ParsedRegion> {
    #[derive(PartialEq)]
    enum S {
        None,
        Ours,
        Base,
        Theirs,
    }
    let mut regions = Vec::new();
    let mut state = S::None;
    let mut cur: Option<ParsedRegion> = None;
    for (idx, line) in merged.split('\n').enumerate() {
        let lineno = (idx + 1) as u32;
        if line.starts_with("<<<<<<<") {
            cur = Some(ParsedRegion {
                ours_block: Vec::new(),
                theirs_block: Vec::new(),
                ours_start: 0,
                ours_end: 0,
                theirs_start: 0,
                theirs_end: 0,
            });
            state = S::Ours;
        } else if line.starts_with("|||||||") && cur.is_some() {
            state = S::Base;
        } else if line.starts_with("=======") && cur.is_some() {
            state = S::Theirs;
        } else if line.starts_with(">>>>>>>") && cur.is_some() {
            if let Some(r) = cur.take() {
                regions.push(r);
            }
            state = S::None;
        } else if let Some(r) = cur.as_mut() {
            match state {
                S::Ours => {
                    if r.ours_start == 0 {
                        r.ours_start = lineno;
                    }
                    r.ours_end = lineno;
                    r.ours_block.push(line.to_string());
                }
                S::Theirs => {
                    if r.theirs_start == 0 {
                        r.theirs_start = lineno;
                    }
                    r.theirs_end = lineno;
                    r.theirs_block.push(line.to_string());
                }
                _ => {}
            }
        }
    }
    regions
}

/// Find `block` as a contiguous run of lines in `blob`, searching forward from `cursor`
/// first (regions and blocks appear in the same order, so a forward cursor disambiguates
/// duplicate text) then from the start. Returns the 0-based inclusive span.
fn locate(block: &[String], blob: &[String], cursor: &mut usize) -> Option<(usize, usize)> {
    if block.is_empty() || blob.len() < block.len() {
        return None;
    }
    let last = blob.len() - block.len();
    let find_from = |from: usize| (from..=last).find(|&s| &blob[s..s + block.len()] == block);
    let start = find_from((*cursor).min(last)).or_else(|| find_from(0))?;
    *cursor = start + block.len();
    Some((start, start + block.len() - 1))
}

/// Tally the distinct authors over a 0-based inclusive `span` of a side, ranked by lines.
fn tally(span: Option<(usize, usize)>, side: &SideBlame) -> Vec<ConflictAuthor> {
    let Some((start, end)) = span else {
        return Vec::new();
    };
    let end = end.min(side.authors.len().saturating_sub(1));
    let mut acc: BTreeMap<String, (String, u32)> = BTreeMap::new(); // email -> (name, lines)
    for (name, email) in side.authors.get(start..=end).unwrap_or_default() {
        let entry = acc.entry(email.clone()).or_insert_with(|| (name.clone(), 0));
        if entry.0.is_empty() && !name.is_empty() {
            entry.0 = name.clone();
        }
        entry.1 += 1;
    }
    let mut out: Vec<ConflictAuthor> = acc
        .into_iter()
        .map(|(email, (name, lines))| ConflictAuthor {
            username: derive_username(&name, &email),
            name,
            email,
            lines,
        })
        .collect();
    out.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.username.cmp(&b.username)));
    out
}

/// Every distinct author of a side's whole blob — the fallback when a region's block text
/// can't be located exactly.
fn tally_all(side: &SideBlame) -> Vec<ConflictAuthor> {
    if side.authors.is_empty() {
        return Vec::new();
    }
    tally(Some((0, side.authors.len() - 1)), side)
}

/// Merge per-region author lists into one file-level list, summing line counts by email.
fn union_authors<'a>(lists: impl Iterator<Item = &'a Vec<ConflictAuthor>>) -> Vec<ConflictAuthor> {
    let mut acc: BTreeMap<String, ConflictAuthor> = BTreeMap::new();
    for list in lists {
        for a in list {
            acc.entry(a.email.clone())
                .and_modify(|e| e.lines += a.lines)
                .or_insert_with(|| a.clone());
        }
    }
    let mut out: Vec<ConflictAuthor> = acc.into_values().collect();
    out.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.username.cmp(&b.username)));
    out
}

/// The incoming tip oid during any in-progress operation that can conflict: a merge, a
/// cherry-pick, or a rebase. These are loose files in the git dir.
fn incoming_head(repo: &Repository) -> Option<Oid> {
    ["MERGE_HEAD", "CHERRY_PICK_HEAD", "REBASE_HEAD"]
        .into_iter()
        .find_map(|name| {
            let text = std::fs::read_to_string(repo.path().join(name)).ok()?;
            Oid::from_str(text.trim()).ok()
        })
}

/// The incoming branch name from `MERGE_MSG` (e.g. `origin/feature-intern` out of
/// "Merge remote-tracking branch 'origin/feature-intern'").
fn incoming_branch(repo: &Repository) -> Option<String> {
    let msg = std::fs::read_to_string(repo.path().join("MERGE_MSG")).ok()?;
    let first = msg.lines().next()?;
    match (first.find('\''), first.rfind('\'')) {
        (Some(a), Some(b)) if b > a => Some(first[a + 1..b].to_string()),
        _ => Some(first.to_string()),
    }
}

/// The author of a tip commit, as a `ConflictAuthor` (with `lines` unused).
fn tip_author(repo: &Repository, oid: Oid) -> Option<ConflictAuthor> {
    let commit = repo.find_commit(oid).ok()?;
    let author = commit.author();
    let name = author.name().unwrap_or_default().to_string();
    let email = author.email().unwrap_or_default().to_string();
    Some(ConflictAuthor {
        username: derive_username(&name, &email),
        name,
        email,
        lines: 0,
    })
}

/// Enumerate conflicted files from the index, each with its sides, regions, and per-side
/// author attribution from blame.
fn collect_conflicts(repo: &Repository) -> AppResult<Vec<ConflictHunk>> {
    let workdir = repo.workdir().map(|p| p.to_path_buf());
    let head_oid = repo.head().ok().and_then(|h| h.target());
    let theirs_oid = incoming_head(repo);
    let index = repo.index()?;

    let mut out = Vec::new();
    let Ok(conflicts) = index.conflicts() else {
        return Ok(out);
    };
    for conflict in conflicts.flatten() {
        let Some(file) = conflict_path(&conflict) else {
            continue;
        };
        let ours_text = blob_text(repo, conflict.our.as_ref());
        let theirs_text = blob_text(repo, conflict.their.as_ref());
        let base = blob_text(repo, conflict.ancestor.as_ref());
        let merged = working_text(workdir.as_deref(), &file);

        let ours_side = side_blame(repo, &file, head_oid, &ours_text);
        let theirs_side = side_blame(repo, &file, theirs_oid, &theirs_text);

        let mut ours_cursor = 0usize;
        let mut theirs_cursor = 0usize;
        let mut regions = Vec::new();
        for (i, pr) in parse_conflict_regions(&merged).iter().enumerate() {
            let ours_span = locate(&pr.ours_block, &ours_side.lines, &mut ours_cursor);
            let theirs_span = locate(&pr.theirs_block, &theirs_side.lines, &mut theirs_cursor);
            let ours_missing = !pr.ours_block.is_empty() && ours_span.is_none();
            let theirs_missing = !pr.theirs_block.is_empty() && theirs_span.is_none();
            regions.push(ConflictRegion {
                index: (i + 1) as u32,
                ours_start: pr.ours_start,
                ours_end: pr.ours_end,
                theirs_start: pr.theirs_start,
                theirs_end: pr.theirs_end,
                ours_authors: if ours_missing {
                    tally_all(&ours_side)
                } else {
                    tally(ours_span, &ours_side)
                },
                theirs_authors: if theirs_missing {
                    tally_all(&theirs_side)
                } else {
                    tally(theirs_span, &theirs_side)
                },
                approximate: ours_missing || theirs_missing,
            });
        }

        // No conflict markers in the working tree — an add/add conflict, or the file was
        // already hand-edited without staging. The index stages still hold both sides, so
        // attribute the whole conflict by blaming each side's blob in full.
        if regions.is_empty() && (!ours_side.lines.is_empty() || !theirs_side.lines.is_empty()) {
            regions.push(ConflictRegion {
                index: 1,
                ours_start: if ours_side.lines.is_empty() { 0 } else { 1 },
                ours_end: ours_side.lines.len() as u32,
                theirs_start: if theirs_side.lines.is_empty() { 0 } else { 1 },
                theirs_end: theirs_side.lines.len() as u32,
                ours_authors: tally_all(&ours_side),
                theirs_authors: tally_all(&theirs_side),
                approximate: true,
            });
        }

        let ours_authors = union_authors(regions.iter().map(|r| &r.ours_authors));
        let theirs_authors = union_authors(regions.iter().map(|r| &r.theirs_authors));
        out.push(ConflictHunk {
            file,
            ours: ours_text,
            theirs: theirs_text,
            base,
            merged,
            ours_authors,
            theirs_authors,
            regions,
        });
    }
    Ok(out)
}

/// Enumerate conflicted files from the index, with each side's content and attribution.
pub fn list_conflicts(path: &str) -> AppResult<Vec<ConflictHunk>> {
    let repo = Repository::discover(path)?;
    collect_conflicts(&repo)
}

/// Merge-level "who is in conflict with whom": the branches, the two tip authors, and the
/// aggregated conflict pairs across every region of every conflicted file.
pub fn conflict_summary(path: &str) -> AppResult<MergeConflictSummary> {
    let repo = Repository::discover(path)?;
    let in_merge = repo.state() != git2::RepositoryState::Clean;
    let ours_branch = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().ok().map(str::to_string));
    let ours_tip = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .and_then(|oid| tip_author(&repo, oid));
    let theirs_tip = incoming_head(&repo).and_then(|oid| tip_author(&repo, oid));

    // Aggregate (ours-author, theirs-author) pairs across regions. Each region contributes
    // at most one count per distinct pair.
    let mut counts: BTreeMap<(String, String), u32> = BTreeMap::new();
    for hunk in collect_conflicts(&repo)? {
        for region in &hunk.regions {
            for o in &region.ours_authors {
                for t in &region.theirs_authors {
                    *counts
                        .entry((o.username.clone(), t.username.clone()))
                        .or_insert(0) += 1;
                }
            }
        }
    }
    let mut pairs: Vec<ConflictPair> = counts
        .into_iter()
        .map(|((ours, theirs), regions)| ConflictPair {
            ours,
            theirs,
            regions,
        })
        .collect();
    pairs.sort_by(|a, b| {
        b.regions
            .cmp(&a.regions)
            .then_with(|| a.ours.cmp(&b.ours))
            .then_with(|| a.theirs.cmp(&b.theirs))
    });

    Ok(MergeConflictSummary {
        in_merge,
        ours_branch,
        incoming_branch: incoming_branch(&repo),
        ours_tip,
        theirs_tip,
        pairs,
    })
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

    /// Build a conflict where the OURS side of a single region was written by *two*
    /// different authors (Alice line 1, Bob line 2) and the THEIRS side by a third
    /// (Dave). This is the "2+ users" case: attribution must be a set per side.
    fn multi_author_conflict_repo() -> PathBuf {
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
        let dir = std::env::temp_dir().join(format!("githud-multiauthor-{unique}"));
        let repo = git2::Repository::init(&dir).unwrap();

        let commit = |content: &str, msg: &str, sig: &git2::Signature, parent: Option<git2::Oid>| -> git2::Oid {
            std::fs::write(dir.join("f.txt"), content).unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("f.txt")).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let parents: Vec<git2::Commit> =
                parent.into_iter().map(|o| repo.find_commit(o).unwrap()).collect();
            let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
            repo.commit(Some("HEAD"), sig, sig, msg, &tree, &parent_refs).unwrap()
        };

        let alice = git2::Signature::now("Alice", "alice@example.com").unwrap();
        let bob = git2::Signature::now("Bob", "bob@example.com").unwrap();
        let dave = git2::Signature::now("Dave", "dave@example.com").unwrap();

        let base = commit("L1\nL2\n", "base", &alice, None);
        let default_branch = repo.head().unwrap().shorthand().unwrap().to_string();

        // Incoming branch: Dave rewrites both lines.
        repo.branch("feature", &repo.find_commit(base).unwrap(), false).unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        commit("dave-1\ndave-2\n", "dave edits", &dave, Some(base));

        // Our branch: Alice edits line 1, then Bob edits line 2 — two authors, adjacent
        // lines, so the merge produces a single region owned by both.
        repo.set_head(&format!("refs/heads/{default_branch}")).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        let a1 = commit("alice-1\nL2\n", "alice edits L1", &alice, Some(base));
        commit("alice-1\nbob-2\n", "bob edits L2", &bob, Some(a1));

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
    fn attributes_conflict_authors_per_side() {
        let dir = multi_author_conflict_repo();
        let path = dir.to_str().unwrap();

        let conflicts = super::list_conflicts(path).unwrap();
        assert_eq!(conflicts.len(), 1);
        let hunk = &conflicts[0];
        assert_eq!(hunk.regions.len(), 1, "adjacent edits should form one region");
        let region = &hunk.regions[0];
        assert!(!region.approximate, "blame line-matching should have succeeded");

        let names = |authors: &[super::ConflictAuthor]| -> Vec<String> {
            let mut v: Vec<String> = authors.iter().map(|a| a.username.clone()).collect();
            v.sort();
            v
        };
        // Ours side owned by TWO people; theirs by one.
        assert_eq!(names(&region.ours_authors), vec!["alice", "bob"]);
        assert_eq!(names(&region.theirs_authors), vec!["dave"]);
        assert_eq!(names(&hunk.ours_authors), vec!["alice", "bob"]);

        // Merge-wide summary: two who-vs-whom pairs, both against Dave.
        let summary = super::conflict_summary(path).unwrap();
        assert!(summary.in_merge);
        assert_eq!(summary.theirs_tip.as_ref().unwrap().username, "dave");
        assert_eq!(summary.ours_tip.as_ref().unwrap().username, "bob"); // HEAD tip = last commit
        let mut pairs: Vec<(String, String)> = summary
            .pairs
            .iter()
            .map(|p| (p.ours.clone(), p.theirs.clone()))
            .collect();
        pairs.sort();
        assert_eq!(
            pairs,
            vec![
                ("alice".to_string(), "dave".to_string()),
                ("bob".to_string(), "dave".to_string()),
            ]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn attributes_whole_file_when_no_markers() {
        let dir = multi_author_conflict_repo();
        let path = dir.to_str().unwrap();
        // Simulate a hand-edited (or add/add) working tree with NO conflict markers: the
        // index still records the conflict, but there is nothing for the region parser to
        // find. Attribution must fall back to blaming each side's whole blob.
        std::fs::write(dir.join("f.txt"), "hand written, no markers\n").unwrap();

        let conflicts = super::list_conflicts(path).unwrap();
        assert_eq!(conflicts.len(), 1);
        let hunk = &conflicts[0];
        assert_eq!(hunk.regions.len(), 1, "whole-file fallback region");
        assert!(hunk.regions[0].approximate);

        let names = |authors: &[super::ConflictAuthor]| {
            let mut v: Vec<String> = authors.iter().map(|a| a.username.clone()).collect();
            v.sort();
            v
        };
        assert_eq!(names(&hunk.ours_authors), vec!["alice", "bob"]);
        assert_eq!(names(&hunk.theirs_authors), vec!["dave"]);

        std::fs::remove_dir_all(&dir).ok();
    }
}
