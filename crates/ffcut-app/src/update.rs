//! "Is there a newer release?" — one small request to GitHub (`/releases/latest`).
//!
//! The check only tells the user; installing is a separate step they start ([`crate::install`], for
//! the Windows zip). The check runs in a background thread, can be switched off in Settings, and any
//! failure (offline, rate limit, no releases yet) is silent unless the user asked for it explicitly.

use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub notes: String,
    /// The file of this release that this build can install by itself.
    pub asset: Option<Asset>,
}

/// A release file with the checksum GitHub computed for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lower-case hex.
    pub sha256: String,
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

/// End of the name of the release file this build installs: the Windows zip. `FFCUT_UPDATE_ASSET`
/// names another one (for testing the installer elsewhere).
pub fn asset_suffix() -> Option<String> {
    if let Some(s) = std::env::var("FFCUT_UPDATE_ASSET").ok().filter(|s| !s.is_empty()) {
        return Some(s);
    }
    cfg!(windows).then(|| "-windows-x64.zip".to_string())
}

/// Only a file with a sha256 digest is taken: without one a damaged or replaced download could not be told apart.
fn pick_asset(v: &serde_json::Value, suffix: &str) -> Option<Asset> {
    v["assets"].as_array()?.iter().find_map(|a| {
        let name = a["name"].as_str().filter(|n| n.ends_with(suffix))?;
        if a["state"].as_str().is_some_and(|s| s != "uploaded") {
            return None;
        }
        let sha = a["digest"].as_str()?.strip_prefix("sha256:")?;
        if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Asset {
            name: name.to_string(),
            url: a["browser_download_url"].as_str()?.to_string(),
            size: a["size"].as_u64()?,
            sha256: sha.to_ascii_lowercase(),
        })
    })
}

pub fn parse_release(json: &str, current: &str, suffix: Option<&str>) -> Result<Option<Release>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("unexpected answer: {e}"))?;
    let tag = v["tag_name"].as_str().ok_or("the answer has no tag_name (no releases yet?)")?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) || !is_newer(tag, current) {
        return Ok(None);
    }
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        page: v["html_url"].as_str().unwrap_or(REPOSITORY).to_string(),
        notes: v["body"].as_str().unwrap_or("").trim().to_string(),
        asset: suffix.and_then(|s| pick_asset(&v, s)),
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
    parse_release(&body, VERSION, asset_suffix().as_deref())
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
        let r = parse_release(j, "0.2.0", None).unwrap().unwrap();
        assert_eq!((r.version.as_str(), r.notes.as_str(), r.asset.as_ref()), ("0.3.0", "Fixes", None));
        assert_eq!(parse_release(j, "0.3.0", None).unwrap(), None);
        assert_eq!(parse_release(&j.replace("false}", "true}"), "0.2.0", None).unwrap(), None); // prerelease
        assert!(parse_release(r#"{"message":"Not Found"}"#, "0.1.0", None).is_err());
    }

    #[test]
    fn picks_the_installable_file_with_its_checksum() {
        let sha = "D3A220F17C612AA9C8C385D1D3DBB764C9DD25F961FADD953779580E2AA9D5D6";
        let asset = |name: &str, digest: &str| {
            format!(
                r#"{{"name":"{name}","state":"uploaded","size":52053780,"digest":"{digest}","browser_download_url":"https://github.com/o/r/releases/download/v3.0.0/{name}"}}"#
            )
        };
        let j = |assets: &[String]| format!(r#"{{"tag_name":"v3.0.0","html_url":"p","draft":false,"prerelease":false,"assets":[{}]}}"#, assets.join(","));
        let both = j(&[asset("FFcut-3.0.0-x86_64.AppImage", "sha256:00"), asset("FFcut-3.0.0-windows-x64.zip", &format!("sha256:{sha}"))]);
        let a = parse_release(&both, "2.7.1", Some("-windows-x64.zip")).unwrap().unwrap().asset.unwrap();
        assert_eq!((a.name.as_str(), a.size, a.sha256), ("FFcut-3.0.0-windows-x64.zip", 52053780, sha.to_ascii_lowercase()));
        assert!(a.url.ends_with("/v3.0.0/FFcut-3.0.0-windows-x64.zip"));
        assert_eq!(parse_release(&both, "2.7.1", None).unwrap().unwrap().asset, None);
        // no checksum (or not a sha256 one): the file is not offered
        let none = j(&[asset("FFcut-3.0.0-windows-x64.zip", "")]);
        assert_eq!(parse_release(&none, "2.7.1", Some("-windows-x64.zip")).unwrap().unwrap().asset, None);
        let md5 = j(&[asset("FFcut-3.0.0-windows-x64.zip", "md5:0123")]);
        assert_eq!(parse_release(&md5, "2.7.1", Some("-windows-x64.zip")).unwrap().unwrap().asset, None);
    }

    #[test]
    fn placeholder_repository_means_not_configured() {
        // while Cargo.toml says OWNER the check stays off
        if REPOSITORY.contains("OWNER") {
            assert!(slug().is_none());
        }
    }
}
