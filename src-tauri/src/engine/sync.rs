//! Remote sync via the system `git` CLI.
//!
//! git-hud's git2 engine is compiled without network features (offline by design), so
//! fetching delegates to the user's installed `git`. That reuses whatever credentials the
//! user already has configured (SSH agent, credential helper, `gh`), so git-hud never
//! handles a token or key itself. We only ever `fetch` — never `merge`/`pull`: a fetch
//! updates remote-tracking refs (`origin/*`) without touching the working tree, the local
//! branch, or HEAD, so it can never clobber uncommitted work.

use std::process::Command;

/// Result of a fetch attempt.
pub struct FetchOutcome {
    pub ok: bool,
    /// Empty on success; a short reason (first line of git's stderr) on failure.
    pub message: String,
}

/// Arguments for the hardened fetch. `ext`/`fd` transports are disabled and (together with
/// `GIT_PROTOCOL_FROM_USER=0` set on the command) block code-execution via a malicious repo
/// config (e.g. an `ext::` remote), while `https`/`ssh`/`git` remotes still work.
fn fetch_args(path: &str) -> Vec<String> {
    vec![
        "-c".into(),
        "protocol.ext.allow=never".into(),
        "-c".into(),
        "protocol.fd.allow=never".into(),
        "-C".into(),
        path.into(),
        "fetch".into(),
        "--all".into(),
        "--prune".into(),
        "--quiet".into(),
    ]
}

/// Run a hardened `git fetch --all --prune` in `path`. Non-destructive. Best-effort: the
/// caller decides how to surface failures (offline, auth not configured, not a repo, ...).
pub fn git_fetch(path: &str) -> FetchOutcome {
    match Command::new("git")
        // Treat this automated fetch as NOT user-initiated, so `user`-policy protocols
        // (ext, file, …) are refused even if a repo's config configures them.
        .env("GIT_PROTOCOL_FROM_USER", "0")
        .args(fetch_args(path))
        .output()
    {
        Ok(out) if out.status.success() => FetchOutcome {
            ok: true,
            message: String::new(),
        },
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let msg = stderr.lines().next().unwrap_or("").trim();
            FetchOutcome {
                ok: false,
                message: if msg.is_empty() {
                    "git fetch failed".to_string()
                } else {
                    msg.to_string()
                },
            }
        }
        Err(e) => FetchOutcome {
            ok: false,
            message: format!("git not available: {e}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::fetch_args;

    #[test]
    fn fetch_is_hardened_against_ext_protocol() {
        let args = fetch_args("/some/repo");
        let joined = args.join(" ");
        assert!(joined.contains("protocol.ext.allow=never"), "ext disabled");
        assert!(joined.contains("protocol.fd.allow=never"), "fd disabled");
        // The hardening `-c` flags must precede the `fetch` subcommand to take effect.
        let fetch_pos = args.iter().position(|a| a == "fetch").unwrap();
        let ext_pos = args.iter().position(|a| a == "protocol.ext.allow=never").unwrap();
        assert!(ext_pos < fetch_pos, "-c config must come before the subcommand");
        assert_eq!(args.iter().filter(|a| *a == "/some/repo").count(), 1);
    }
}
