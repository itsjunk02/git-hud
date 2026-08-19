//! Remote CI/CD + compliance status.
//!
//! Two real sources, no fabrication:
//! - **DCO** is computed locally from commit `Signed-off-by:` trailers (git2) — offline.
//! - **build/test** pipeline status is pulled from the GitHub Checks API via the `gh`
//!   CLI (`gh api …/check-runs`), reusing the user's existing GitHub login. git-hud never
//!   handles a raw token and adds no HTTP-client crate; `gh` does auth + transport and
//!   `serde_json` (already a dependency) parses the response.
//!
//! Every path is best-effort: when a source is unavailable (no remote, non-GitHub host,
//! `gh` missing/unauthenticated, network down) the badge is reported as `unknown` rather
//! than a misleading green. `updated_at` is a real epoch so the UI can show staleness.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::engine::git;

/// How many commits back to scan for DCO signoffs.
const DCO_SCAN_LIMIT: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CiStatus {
    pub pipeline: String,
    /// Canonical state the UI keys its icon off: `success`, `failed`, `running`,
    /// `verified`, `unsigned`, or `unknown`.
    pub status: String,
    /// Short badge label surfaced in the UI, e.g. `passing`, `compliant`, `3/4 signed`.
    pub badge: String,
    /// Unix epoch seconds of when this status was computed.
    pub updated_at: f64,
}

impl CiStatus {
    fn new(pipeline: &str, status: &str, badge: impl Into<String>, now: f64) -> Self {
        Self {
            pipeline: pipeline.to_string(),
            status: status.to_string(),
            badge: badge.into(),
            updated_at: now,
        }
    }
}

/// Poll every status source for the repo at `path`. Ordering matches the dashboard:
/// pipeline checks first, then the DCO/CLA compliance card.
pub fn poll_ci(path: &str) -> Vec<CiStatus> {
    let now = now_epoch();
    let mut out = github_checks(path, now);
    out.push(dco_status(path, now));
    out
}

/// Local DCO compliance from commit trailers. Always resolves to a real answer.
fn dco_status(path: &str, now: f64) -> CiStatus {
    match git::dco_report(path, DCO_SCAN_LIMIT) {
        Ok(r) if r.is_compliant() => {
            CiStatus::new("DCO / CLA", "verified", "compliant", now)
        }
        Ok(r) if r.checked == 0 => {
            CiStatus::new("DCO / CLA", "unknown", "no commits", now)
        }
        Ok(r) => CiStatus::new(
            "DCO / CLA",
            "unsigned",
            format!("{}/{} signed", r.signed, r.checked),
            now,
        ),
        Err(_) => CiStatus::new("DCO / CLA", "unknown", "unavailable", now),
    }
}

/// build/test pipeline status from the GitHub Checks API for HEAD. Returns one card per
/// check run, or a single `unknown` card explaining why nothing could be fetched.
fn github_checks(path: &str, now: f64) -> Vec<CiStatus> {
    let unknown = |badge: &str| vec![CiStatus::new("CI", "unknown", badge.to_string(), now)];

    let url = match git::remote_url(path, "origin") {
        Ok(Some(u)) => u,
        Ok(None) => return unknown("no remote"),
        Err(_) => return unknown("unavailable"),
    };
    let (owner, repo) = match parse_github_slug(&url) {
        Some(slug) => slug,
        None => return unknown("non-GitHub remote"),
    };
    let sha = match git::head_sha(path) {
        Ok(s) => s,
        Err(_) => return unknown("no HEAD"),
    };

    // `gh` ran (stdout captured even on non-2xx); `None` only means gh couldn't spawn.
    let raw = match gh_check_runs(&owner, &repo, &sha) {
        Some(json) => json,
        None => return unknown("gh not installed"),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return unknown("unavailable"),
    };

    // Happy path: a check_runs array. Empty means the commit has no checks configured.
    if let Some(runs) = parsed.get("check_runs").and_then(|v| v.as_array()) {
        if runs.is_empty() {
            return unknown("no checks");
        }
        return runs
            .iter()
            .map(|run| {
                let name = run.get("name").and_then(|v| v.as_str()).unwrap_or("check");
                let run_status = run.get("status").and_then(|v| v.as_str()).unwrap_or("");
                let conclusion = run.get("conclusion").and_then(|v| v.as_str()).unwrap_or("");
                let (status, badge) = classify_check(run_status, conclusion);
                CiStatus::new(name, status, badge, now)
            })
            .collect();
    }

    // Error object, e.g. 404 "Not Found" when HEAD isn't pushed to the remote. Surface the
    // real reason instead of a generic failure, so the gray badge tells the user why.
    if let Some(msg) = parsed.get("message").and_then(|v| v.as_str()) {
        return unknown(&msg.to_lowercase());
    }
    unknown("unavailable")
}

/// Map a GitHub check run's (status, conclusion) to git-hud's (status, badge).
/// See https://docs.github.com/rest/checks/runs for the enum values.
fn classify_check(run_status: &str, conclusion: &str) -> (&'static str, &'static str) {
    if run_status != "completed" {
        // queued | in_progress | waiting | pending | requested
        return ("running", "in progress");
    }
    match conclusion {
        "success" => ("success", "passing"),
        "neutral" | "skipped" => ("success", conclusion_label(conclusion)),
        "failure" | "timed_out" | "cancelled" | "action_required" | "stale" | "startup_failure" => {
            ("failed", conclusion_label(conclusion))
        }
        _ => ("unknown", "unknown"),
    }
}

fn conclusion_label(conclusion: &str) -> &'static str {
    match conclusion {
        "neutral" => "neutral",
        "skipped" => "skipped",
        "failure" => "failing",
        "timed_out" => "timed out",
        "cancelled" => "cancelled",
        "action_required" => "action required",
        "stale" => "stale",
        "startup_failure" => "startup failure",
        _ => "unknown",
    }
}

/// Invoke `gh api` for the check runs of a commit and return its stdout (which is JSON on
/// both success and API errors, so the caller can read the reason). Returns `None` only
/// when `gh` cannot be spawned at all (not installed).
fn gh_check_runs(owner: &str, repo: &str, sha: &str) -> Option<String> {
    let endpoint = format!("repos/{owner}/{repo}/commits/{sha}/check-runs");
    let output = Command::new("gh")
        .args(["api", "-H", "Accept: application/vnd.github+json", &endpoint])
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()
}

/// Parse `(owner, repo)` from a GitHub remote URL. Handles the common HTTPS and SSH forms;
/// returns `None` for non-GitHub hosts.
fn parse_github_slug(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    // Locate the part after the github.com host, for either `:` (scp-like) or `/` forms.
    let after_host = if let Some(rest) = url.strip_prefix("git@github.com:") {
        rest
    } else if let Some(idx) = url.find("github.com/") {
        &url[idx + "github.com/".len()..]
    } else {
        return None;
    };

    let path = after_host.trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.splitn(2, '/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    // Guard against trailing path segments (e.g. .../repo/tree/main).
    let repo = repo.split('/').next().unwrap_or(repo);
    Some((owner.to_string(), repo.to_string()))
}

fn now_epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as f64)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_and_https_slugs() {
        let cases = [
            "git@github.com:itsjunk02/Gitadsys.git",
            "https://github.com/itsjunk02/Gitadsys.git",
            "https://github.com/itsjunk02/Gitadsys",
            "ssh://git@github.com/itsjunk02/Gitadsys.git",
        ];
        for url in cases {
            let (owner, repo) = parse_github_slug(url).unwrap_or_else(|| panic!("failed: {url}"));
            assert_eq!(owner, "itsjunk02", "owner for {url}");
            assert_eq!(repo, "Gitadsys", "repo for {url}");
        }
    }

    #[test]
    fn rejects_non_github_remotes() {
        assert!(parse_github_slug("git@gitlab.com:group/proj.git").is_none());
        assert!(parse_github_slug("https://bitbucket.org/team/repo.git").is_none());
        assert!(parse_github_slug("not a url").is_none());
    }

    #[test]
    fn classifies_check_conclusions() {
        assert_eq!(classify_check("completed", "success"), ("success", "passing"));
        assert_eq!(classify_check("completed", "failure").0, "failed");
        assert_eq!(classify_check("in_progress", ""), ("running", "in progress"));
        assert_eq!(classify_check("completed", "weird_new_value").0, "unknown");
    }
}
