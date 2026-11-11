//! Git facts for the active project, read from `.git` without running `git`.
//! Worktrees and submodules are resolved through `gitdir:` and `commondir`.

use std::fs;
use std::path::{Path, PathBuf};

/// What the presence needs to know about the active project's repository.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GitInfo {
    /// Current branch, or the short SHA when HEAD is detached.
    pub(crate) branch: Option<String>,
    /// `origin` remote converted to an `https` URL.
    pub(crate) remote_url: Option<String>,
}

/// Reads [`GitInfo`] from the project root. `None` fields mean "not a repo"
/// or "nothing readable", never an error.
#[must_use]
pub(crate) fn info(root: &Path) -> GitInfo {
    let Some(git_dir) = git_dir(root) else {
        return GitInfo::default();
    };
    GitInfo {
        branch: branch(&git_dir),
        remote_url: remote_url(&git_dir),
    }
}

/// Resolves `<root>/.git`, which may be a directory or a `gitdir:` file
/// pointing (relative or absolute) at the real git directory.
fn git_dir(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    let metadata = fs::metadata(&dot_git).ok()?;
    if metadata.is_dir() {
        return Some(dot_git);
    }
    if !metadata.is_file() {
        return None;
    }
    let contents = fs::read_to_string(&dot_git).ok()?;
    let target = contents.lines().next()?.strip_prefix("gitdir:")?.trim();
    if target.is_empty() {
        return None;
    }
    let path = Path::new(target);
    Some(if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    })
}

/// `ref: refs/heads/<branch>` becomes the branch; anything else that looks
/// like a commit id becomes its first seven characters (detached HEAD).
fn branch(git_dir: &Path) -> Option<String> {
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        return reference
            .trim()
            .strip_prefix("refs/heads/")
            .map(str::to_owned);
    }
    if head.is_empty() || !head.chars().all(|character| character.is_ascii_hexdigit()) {
        return None;
    }
    Some(head.chars().take(7).collect())
}

/// Where the config lives. In a worktree the git directory is
/// `.git/worktrees/<name>`, and `commondir` points at the shared `.git`.
fn config_path(git_dir: &Path) -> PathBuf {
    let Ok(common) = fs::read_to_string(git_dir.join("commondir")) else {
        return git_dir.join("config");
    };
    let target = common.trim();
    if target.is_empty() {
        return git_dir.join("config");
    }
    let path = Path::new(target);
    let base = if path.is_absolute() {
        path.to_path_buf()
    } else {
        git_dir.join(path)
    };
    base.join("config")
}

fn remote_url(git_dir: &Path) -> Option<String> {
    let config = fs::read_to_string(config_path(git_dir)).ok()?;
    https_url(origin_url(&config)?)
}

/// Reads the first `url` of the `[remote "origin"]` section.
fn origin_url(config: &str) -> Option<&str> {
    let mut in_origin = false;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_origin = is_origin_section(line);
            continue;
        }
        if !in_origin {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("url") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn is_origin_section(line: &str) -> bool {
    let Some(inner) = line
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return false;
    };
    let Some(rest) = inner.trim().strip_prefix("remote") else {
        return false;
    };
    rest.trim() == "\"origin\""
}

/// Accepts `git@host:owner/repo(.git)`, `ssh://git@host/owner/repo(.git)` and
/// already-`https` URLs. Anything else is rejected.
fn https_url(url: &str) -> Option<String> {
    let url = url.trim();
    let converted = if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        format!("https://{host}/{}", path.trim_start_matches('/'))
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let (authority, path) = rest.split_once('/')?;
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let host = host.split(':').next().unwrap_or(host);
        format!("https://{host}/{path}")
    } else if url.starts_with("https://") {
        url.to_owned()
    } else {
        return None;
    };
    normalize_https(&converted)
}

fn normalize_https(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    // Never show embedded credentials in a button anybody can see.
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("https://{host}/{path}"))
}

#[cfg(test)]
mod tests;
