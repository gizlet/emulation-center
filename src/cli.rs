//! The `gizlet` command, through its documented subcommands: `target` for
//! the packages this device takes, `list` for what is installed, `install`
//! for a downloaded package. `GIZLET_CLI` names another program (tests).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub fn program() -> String {
    std::env::var("GIZLET_CLI").unwrap_or_else(|_| "gizlet".into())
}

fn run(args: &[&std::ffi::OsStr]) -> Result<String, String> {
    let out = Command::new(program()).args(args).output().map_err(|e| format!("cannot run {}: {e}", program()))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let err = err.trim().strip_prefix("gizlet: ").unwrap_or(err.trim());
    Err(if err.is_empty() { format!("gizlet exited with {}", out.status) } else { err.to_string() })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub target: String,
    pub tier: String,
}

/// `gizlet target` prints the target triple and tier, separated by a space.
pub fn parse_target(out: &str) -> Option<Target> {
    let mut words = out.split_whitespace();
    let target = words.next()?.to_string();
    let tier = words.next()?.to_string();
    Some(Target { target, tier })
}

pub fn target() -> Result<Target, String> {
    let out = run(&["target".as_ref()])?;
    parse_target(&out).ok_or_else(|| format!("unexpected output from gizlet target: {out:?}"))
}

/// `gizlet list` prints one installed app per line: ID, version and name,
/// separated by tabs.
pub fn parse_list(out: &str) -> HashMap<String, String> {
    out.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            Some((fields.next()?.trim().to_string(), fields.next()?.trim().to_string()))
        })
        .filter(|(id, version)| !id.is_empty() && !version.is_empty())
        .collect()
}

pub fn installed() -> Result<HashMap<String, String>, String> {
    run(&["list".as_ref()]).map(|out| parse_list(&out))
}

/// Installs or updates from a `.gzl` file. The command verifies the package
/// and refuses one for another device, or an app that is running.
pub fn install(file: &Path) -> Result<(), String> {
    run(&["install".as_ref(), file.as_os_str()]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_commands_output() {
        assert_eq!(
            parse_target("aarch64-unknown-linux-gnu lite\n"),
            Some(Target { target: "aarch64-unknown-linux-gnu".into(), tier: "lite".into() })
        );
        assert_eq!(parse_target(""), None);
        let list = parse_list("io.gizlet.retroarch\t1.22.2.1\tRetroArch\nio.gizlet.browser\t0.1.2\tBrowser\n\n");
        assert_eq!(list.get("io.gizlet.retroarch").map(String::as_str), Some("1.22.2.1"));
        assert_eq!(list.len(), 2);
    }
}
