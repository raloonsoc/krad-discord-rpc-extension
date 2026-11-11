//! Tests for the parent module.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = format!(
            "krad-discord-rpc-{}-{}-{label}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent dir");
        }
        fs::write(path, contents).expect("write file");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_branch_head_resolves_to_the_branch_name() {
    let dir = TempDir::new("branch");
    dir.write(".git/HEAD", "ref: refs/heads/main\n");
    assert_eq!(info(dir.path()).branch.as_deref(), Some("main"));
}

#[test]
fn branch_names_may_contain_slashes() {
    let dir = TempDir::new("slashes");
    dir.write(".git/HEAD", "ref: refs/heads/feature/discord\n");
    assert_eq!(info(dir.path()).branch.as_deref(), Some("feature/discord"));
}

#[test]
fn a_detached_head_shows_the_short_sha() {
    let dir = TempDir::new("detached");
    dir.write(".git/HEAD", "0123456789abcdef0123456789abcdef01234567\n");
    assert_eq!(info(dir.path()).branch.as_deref(), Some("0123456"));
}

#[test]
fn a_relative_gitdir_file_is_resolved_against_the_root() {
    let dir = TempDir::new("relative-gitdir");
    dir.write("project/.git", "gitdir: ../gitstore/worktrees/wt\n");
    dir.write(
        "gitstore/worktrees/wt/HEAD",
        "ref: refs/heads/worktree-branch\n",
    );
    assert_eq!(
        info(&dir.path().join("project")).branch.as_deref(),
        Some("worktree-branch")
    );
}

#[test]
fn an_absolute_gitdir_file_is_used_as_is() {
    let dir = TempDir::new("absolute-gitdir");
    let git_dir = dir.path().join("somewhere/else");
    dir.write("somewhere/else/HEAD", "ref: refs/heads/absolute\n");
    dir.write("project/.git", &format!("gitdir: {}\n", git_dir.display()));
    assert_eq!(
        info(&dir.path().join("project")).branch.as_deref(),
        Some("absolute")
    );
}

#[test]
fn without_a_repository_everything_is_empty() {
    let dir = TempDir::new("no-repo");
    assert_eq!(info(dir.path()), GitInfo::default());
}

#[test]
fn the_origin_remote_comes_from_the_config() {
    let dir = TempDir::new("remote");
    dir.write(".git/HEAD", "ref: refs/heads/main\n");
    dir.write(
        ".git/config",
        "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@github.com:joordih/krad-ade.git\n",
    );
    let info = info(dir.path());
    assert_eq!(
        info.remote_url.as_deref(),
        Some("https://github.com/joordih/krad-ade")
    );
}

#[test]
fn a_worktree_reads_the_config_from_its_commondir() {
    let dir = TempDir::new("worktree-config");
    dir.write("project/.git", "gitdir: ../gitstore/worktrees/wt\n");
    dir.write("gitstore/worktrees/wt/HEAD", "ref: refs/heads/feature\n");
    dir.write("gitstore/worktrees/wt/commondir", "../..\n");
    dir.write(
        "gitstore/config",
        "[remote \"origin\"]\n\turl = ssh://git@gitlab.com/owner/repo.git\n",
    );
    let info = info(&dir.path().join("project"));
    assert_eq!(info.branch.as_deref(), Some("feature"));
    assert_eq!(
        info.remote_url.as_deref(),
        Some("https://gitlab.com/owner/repo")
    );
}

#[test]
fn remote_urls_are_converted_to_https() {
    let cases = [
        (
            "git@github.com:owner/repo.git",
            Some("https://github.com/owner/repo"),
        ),
        (
            "git@github.com:owner/repo",
            Some("https://github.com/owner/repo"),
        ),
        (
            "ssh://git@github.com/owner/repo.git",
            Some("https://github.com/owner/repo"),
        ),
        (
            "ssh://git@github.com:2222/owner/repo",
            Some("https://github.com/owner/repo"),
        ),
        (
            "https://github.com/owner/repo.git",
            Some("https://github.com/owner/repo"),
        ),
        (
            "https://github.com/owner/repo",
            Some("https://github.com/owner/repo"),
        ),
        (
            "https://user:token@github.com/owner/repo",
            Some("https://github.com/owner/repo"),
        ),
        ("http://github.com/owner/repo", None),
        ("git://github.com/owner/repo", None),
        ("file:///tmp/repo", None),
        ("https://github.com", None),
        ("", None),
    ];
    for (input, expected) in cases {
        assert_eq!(https_url(input).as_deref(), expected, "{input}");
    }
}

#[test]
fn parsing_the_config_only_uses_the_origin_section() {
    let config = "\
[remote \"upstream\"]\n\turl = git@github.com:upstream/repo.git\n\
[remote \"origin\"]\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n\
\turl = ssh://git@github.com:owner/repo.git\n\
[remote \"other\"]\n\turl = git@github.com:other/repo.git\n";
    assert_eq!(
        origin_url(config),
        Some("ssh://git@github.com:owner/repo.git")
    );
}

#[test]
fn a_trailing_git_and_slashes_are_removed() {
    assert_eq!(
        https_url("git@github.com:owner/repo.git/").as_deref(),
        Some("https://github.com/owner/repo")
    );
}
