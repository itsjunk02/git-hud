//! GitHub review data via the `gh` CLI.
//!
//! Runs `gh` with the repo as the working directory (so it infers owner/repo and reuses the
//! user's login). Best-effort like `remote.rs`: returns empty on a non-GitHub repo, missing
//! `gh`, or any error. `serde_json` (already a dependency) parses the payloads.

use std::collections::HashMap;
use std::process::Command;

use serde::{Deserialize, Serialize};
use specta::Type;

/// Per-reviewer review activity across a repo's pull requests (throughput).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ReviewerStat {
    pub login: String,
    pub reviews: u32,
    pub approvals: u32,
}

/// An open pull request awaiting the current user's review.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ReviewRequest {
    pub number: u32,
    pub title: String,
    pub url: String,
}

/// Run `gh` in the repo dir and return its JSON stdout, or `None` if it can't run / failed.
fn gh_json(path: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("gh").current_dir(path).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Per-reviewer review counts across the repo's PRs. Empty on any failure.
pub fn review_stats(path: &str) -> Vec<ReviewerStat> {
    let Some(raw) = gh_json(
        path,
        &["pr", "list", "--state", "all", "--limit", "100", "--json", "reviews"],
    ) else {
        return Vec::new();
    };
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(v) => aggregate_reviews(&v),
        Err(_) => Vec::new(),
    }
}

/// Aggregate a `gh pr list --json reviews` payload into per-login counts, sorted by review
/// count descending. Pure, so it's unit-tested without a network call.
fn aggregate_reviews(parsed: &serde_json::Value) -> Vec<ReviewerStat> {
    let mut order: Vec<String> = Vec::new();
    let mut counts: HashMap<String, (u32, u32)> = HashMap::new(); // login -> (reviews, approvals)

    if let Some(prs) = parsed.as_array() {
        for pr in prs {
            let Some(reviews) = pr.get("reviews").and_then(|v| v.as_array()) else {
                continue;
            };
            for review in reviews {
                let login = review
                    .get("author")
                    .and_then(|a| a.get("login"))
                    .and_then(|l| l.as_str())
                    .unwrap_or("");
                if login.is_empty() {
                    continue;
                }
                let approved = review
                    .get("state")
                    .and_then(|s| s.as_str())
                    .map(|s| s.eq_ignore_ascii_case("APPROVED"))
                    .unwrap_or(false);
                if !counts.contains_key(login) {
                    order.push(login.to_string());
                }
                let entry = counts.entry(login.to_string()).or_insert((0, 0));
                entry.0 += 1;
                if approved {
                    entry.1 += 1;
                }
            }
        }
    }

    let mut out: Vec<ReviewerStat> = order
        .into_iter()
        .map(|login| {
            let (reviews, approvals) = counts[&login];
            ReviewerStat { login, reviews, approvals }
        })
        .collect();
    out.sort_by(|a, b| b.reviews.cmp(&a.reviews));
    out
}

/// Open PRs where the current user is a requested reviewer. Empty on any failure.
pub fn review_requests(path: &str) -> Vec<ReviewRequest> {
    let Some(raw) = gh_json(
        path,
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--search",
            "review-requested:@me",
            "--json",
            "number,title,url",
            "--limit",
            "50",
        ],
    ) else {
        return Vec::new();
    };
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(v) => parse_requests(&v),
        Err(_) => Vec::new(),
    }
}

fn parse_requests(parsed: &serde_json::Value) -> Vec<ReviewRequest> {
    parsed
        .as_array()
        .map(|prs| {
            prs.iter()
                .filter_map(|pr| {
                    let number = pr.get("number").and_then(|n| n.as_u64())? as u32;
                    let title = pr.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
                    let url = pr.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                    Some(ReviewRequest { number, title, url })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_reviews_per_login() {
        let json = serde_json::json!([
            { "reviews": [
                { "author": { "login": "alice" }, "state": "APPROVED" },
                { "author": { "login": "bob" },   "state": "COMMENTED" }
            ]},
            { "reviews": [
                { "author": { "login": "alice" }, "state": "CHANGES_REQUESTED" }
            ]}
        ]);
        let out = aggregate_reviews(&json);
        assert_eq!(out.len(), 2);
        // Sorted by review count desc → alice (2) first.
        assert_eq!(out[0].login, "alice");
        assert_eq!(out[0].reviews, 2);
        assert_eq!(out[0].approvals, 1);
        assert_eq!(out[1].login, "bob");
        assert_eq!(out[1].reviews, 1);
        assert_eq!(out[1].approvals, 0);
    }

    #[test]
    fn parses_review_requests() {
        let json = serde_json::json!([
            { "number": 12, "title": "Fix auth", "url": "https://github.com/o/r/pull/12" }
        ]);
        let out = parse_requests(&json);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].number, 12);
        assert_eq!(out[0].title, "Fix auth");
    }
}
