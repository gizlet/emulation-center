//! Gizlet Store lookups: which release of a gizlet to install on this
//! device, at `GET /v1/gizlets/github.com/<owner>/<repo>/install`. The API is
//! `GIZLET_STORE_API`, or https://api.gizlet.io. Requests go through curl,
//! which Gizlet OS includes with its CA certificates.

use crate::cli::Target;
use serde::Deserialize;
use std::process::Command;

pub const DEFAULT_API: &str = "https://api.gizlet.io";

pub fn api() -> String {
    std::env::var("GIZLET_STORE_API").unwrap_or_else(|_| DEFAULT_API.into()).trim_end_matches('/').to_string()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lookup {
    Ready(Release),
    /// Published, but the Store is still checking it.
    NotReady,
    /// No release for this device yet.
    Missing,
    Error(String),
}

#[derive(Deserialize)]
struct InstallBody {
    version: String,
    package: PackageBody,
}

#[derive(Deserialize)]
struct PackageBody {
    file: String,
    size: Option<u64>,
    sha256: Option<String>,
    url: Option<String>,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorInfo,
}

#[derive(Deserialize)]
struct ErrorInfo {
    code: String,
    message: String,
}

pub fn install_url(api: &str, repo: &str, target: &Target) -> String {
    format!("{api}/v1/gizlets/{repo}/install?target={}&tier={}", target.target, target.tier)
}

pub fn parse(status: u16, body: &str) -> Lookup {
    if status == 200 {
        let Ok(body) = serde_json::from_str::<InstallBody>(body) else {
            return Lookup::Error("the Store sent an unexpected answer".into());
        };
        let p = body.package;
        return match (p.size, p.sha256, p.url) {
            (Some(size), Some(sha256), Some(url))
                if sha256.len() == 64
                    && sha256.bytes().all(|b| b.is_ascii_hexdigit())
                    && p.file.ends_with(".gzl")
                    && !p.file.contains('/')
                    && url.starts_with("http") =>
            {
                Lookup::Ready(Release {
                    version: body.version,
                    file: p.file,
                    size,
                    sha256: sha256.to_ascii_lowercase(),
                    url,
                })
            }
            _ => Lookup::NotReady,
        };
    }
    let error = serde_json::from_str::<ErrorBody>(body).ok().map(|b| b.error);
    match (status, error) {
        (409, _) => Lookup::NotReady,
        (404, _) => Lookup::Missing,
        (_, Some(e)) => Lookup::Error(format!("{} ({})", e.message, e.code)),
        _ => Lookup::Error(format!("the Store answered {status}")),
    }
}

pub fn lookup(api: &str, repo: &str, target: &Target) -> Lookup {
    let out = Command::new("curl")
        .args(["--silent", "--show-error", "--location", "--max-time", "30", "--retry", "2"])
        .args(["--header", "Accept: application/json", "--write-out", "\n%{http_code}"])
        .arg(install_url(api, repo, target))
        .output();
    let out = match out {
        Ok(out) => out,
        Err(e) => return Lookup::Error(format!("cannot run curl: {e}")),
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Lookup::Error(err.trim().trim_start_matches("curl: ").to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    parse(status.trim().parse().unwrap_or(0), body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "69ce797d651ddd3ad1969605890be8ebb49a701972688db7d7e16ea06644a47a";

    #[test]
    fn reads_an_installable_release() {
        let body = format!(
            r#"{{"repo":"github.com/gizlet/retroarch","id":"io.gizlet.retroarch","version":"1.22.2.1","tag":"v1.22.2.1",
            "package":{{"file":"io.gizlet.retroarch-1.22.2.1-aarch64-unknown-linux-gnu-lite.gzl","target":"aarch64-unknown-linux-gnu",
            "tier":"lite","entry":"bin/launch","size":123,"sha256":"{SHA}","url":"https://dl.gizlet.io/x.gzl?sha256={SHA}"}}}}"#
        );
        let Lookup::Ready(r) = parse(200, &body) else { panic!("not ready") };
        assert_eq!(r.version, "1.22.2.1");
        assert_eq!(r.size, 123);
        assert_eq!(r.sha256, SHA);
    }

    #[test]
    fn reads_unfinished_and_missing_releases() {
        assert_eq!(parse(409, r#"{"error":{"code":"not_ready","message":"checks are running"}}"#), Lookup::NotReady);
        assert_eq!(parse(404, r#"{"error":{"code":"not_found","message":"no release"}}"#), Lookup::Missing);
        let unmirrored = r#"{"version":"1","package":{"file":"a.gzl","size":null,"sha256":null,"url":null}}"#;
        assert_eq!(parse(200, unmirrored), Lookup::NotReady);
        let escaping =
            format!(r#"{{"version":"1","package":{{"file":"../a.gzl","size":1,"sha256":"{SHA}","url":"https://x"}}}}"#);
        assert_eq!(parse(200, &escaping), Lookup::NotReady);
        assert!(matches!(parse(500, "oops"), Lookup::Error(_)));
    }

    #[test]
    fn asks_for_this_device() {
        let target = Target { target: "aarch64-unknown-linux-gnu".into(), tier: "lite".into() };
        assert_eq!(
            install_url("https://api.gizlet.io", "github.com/gizlet/ppsspp", &target),
            "https://api.gizlet.io/v1/gizlets/github.com/gizlet/ppsspp/install?target=aarch64-unknown-linux-gnu&tier=lite"
        );
    }
}
