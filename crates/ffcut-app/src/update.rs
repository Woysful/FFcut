//! "Is there a newer release?" — one small request to GitHub (`/releases/latest`).
//!
//! It never downloads or installs anything: it only tells the user and opens the release page.
//! The check runs in a background thread, can be switched off in Settings, and any failure
//! (offline, rate limit, no releases yet) is silent unless the user asked for it explicitly.

use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub notes: String,
}

pub type Outcome = Result<Option<Release>, String>;

/// `https://github.com/owner/repo` -> `owner/repo`; `None` while the placeholder is still there.
fn slug() -> Option<String> {
    let s = REPOSITORY.trim_end_matches('/').strip_prefix("https://github.com/")?;
    (s.matches('/').count() == 1 && !s.contains("OWNER")).then(|| s.to_string())
}

pub fn repo_url() -> &'static str {
    REPOSITORY
}

/// API address to ask. `FFCUT_UPDATE_URL` replaces it (for testing against a local server).
fn endpoint() -> Option<String> {
    if let Ok(u) = std::env::var("FFCUT_UPDATE_URL") {
        if !u.is_empty() {
            return Some(u);
        }
    }
    slug().map(|s| format!("https://api.github.com/repos/{s}/releases/latest"))
}

pub fn configured() -> bool {
    endpoint().is_some()
}

/// "v1.2.3" / "1.2" -> [1,2,3]. A "-beta" style suffix is ignored.
fn parse_version(v: &str) -> Option<Vec<u64>> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next()?;
    let parts: Option<Vec<u64>> = core.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty())
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (Some(mut a), Some(mut b)) = (parse_version(candidate), parse_version(current)) else { return false };
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a > b
}

pub fn parse_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("unexpected answer: {e}"))?;
    let tag = v["tag_name"].as_str().ok_or("the answer has no tag_name (no releases yet?)")?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) || !is_newer(tag, current) {
        return Ok(None);
    }
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        page: v["html_url"].as_str().unwrap_or(REPOSITORY).to_string(),
        notes: v["body"].as_str().unwrap_or("").trim().to_string(),
    }))
}

fn fetch() -> Outcome {
    let url = endpoint().ok_or("the update source is not configured (repository in Cargo.toml still says OWNER)")?;
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(8))).build().into();
    let mut resp =
        agent.get(&url).header("User-Agent", &format!("FFcut/{VERSION}")).header("Accept", "application/vnd.github+json").call().map_err(|e| match e {
            ureq::Error::StatusCode(404) => "no releases published yet".to_string(),
            e => format!("{e}"),
        })?;
    let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    parse_release(&body, VERSION)
}

/// Runs the check in a thread; `repaint` wakes the UI when the answer is in.
pub fn check_in_background(repaint: impl Fn() + Send + 'static) -> Receiver<Outcome> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(fetch());
        repaint();
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(is_newer("0.10.0", "0.9.0")); // numbers, not text
        assert!(is_newer("0.1.1", "0.1"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
        assert!(!is_newer("nightly", "0.1.0"));
    }

    #[test]
    fn reads_a_github_release() {
        let j = r#"{"tag_name":"v0.3.0","html_url":"https://github.com/o/r/releases/tag/v0.3.0","body":" Fixes ","draft":false,"prerelease":false}"#;
        let r = parse_release(j, "0.2.0").unwrap().unwrap();
        assert_eq!((r.version.as_str(), r.notes.as_str()), ("0.3.0", "Fixes"));
        assert_eq!(parse_release(j, "0.3.0").unwrap(), None);
        assert_eq!(parse_release(&j.replace("false}", "true}"), "0.2.0").unwrap(), None); // prerelease
        assert!(parse_release(r#"{"message":"Not Found"}"#, "0.1.0").is_err());
    }

    #[test]
    fn placeholder_repository_means_not_configured() {
        // while Cargo.toml says OWNER the check stays off
        if REPOSITORY.contains("OWNER") {
            assert!(slug().is_none());
        }
    }
}
