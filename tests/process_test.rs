// Copyright (C) 2021 Leandro Lisboa Penz <lpenz@lpenz.org>
// This file is subject to the terms and conditions defined in
// file 'LICENSE', which is part of this source code package.

use std::fs::File;
use std::io::Write;
use std::iter;
use std::process::Command;

use anyhow::ensure;
use anyhow::Result;

use ghaction_version_gen::git;
use ghaction_version_gen::rust;
use ghaction_version_gen::Info;

#[test]
fn basic() -> Result<()> {
    let gitdesc = "1.3.1-20-gc5f7a99";
    let mut info = Info::default();
    info.parse_describe(gitdesc)?;
    assert_eq!(info.git_describe_tags, gitdesc);
    assert_eq!(info.tag_latest, "1.3.1");
    Ok(())
}

struct TmpGit {
    pub repo: tempfile::TempDir,
}

impl TmpGit {
    pub fn new() -> Result<TmpGit> {
        let tmpgit = TmpGit {
            repo: tempfile::tempdir()?,
        };
        tmpgit.run(&["git", "init"])?;
        tmpgit.run(&["git", "config", "--local", "user.name", "username"])?;
        tmpgit.run(&["git", "config", "--local", "user.email", "user@email.net"])?;
        tmpgit.run(&["git", "config", "--local", "commit.gpgsign", "false"])?;
        Ok(tmpgit)
    }

    pub fn run(&self, cmd: &[&str]) -> Result<()> {
        let status = Command::new(cmd[0])
            .current_dir(&self.repo)
            .args(&cmd[1..])
            .status()?;
        ensure!(status.success(), "error running command");
        Ok(())
    }

    pub fn file_write(&self, basename: &str, contents: &str) -> Result<()> {
        let path = self.repo.path().join(basename);
        let mut fd = File::create(path)?;
        fd.write_all(contents.as_bytes())?;
        Ok(())
    }

    fn info_get(&self) -> Result<Info> {
        let mut info = Info::from_workspace(&self.repo, iter::empty())?;
        info.is_push = None;
        info.is_tag = None;
        info.is_main = None;
        info.eval()?;
        Ok(info)
    }
}

#[test]
fn gitrepo() -> Result<()> {
    let repo = TmpGit::new()?;
    repo.file_write("foo.txt", "Hello, world!")?;
    repo.run(&["git", "add", "foo.txt"])?;
    repo.run(&["git", "commit", "-m", "first commit"])?;
    // Check with no tag is present
    let commit1 = git::head_commit(&repo.repo)?;
    let info = repo.info_get()?;
    assert_eq!(info.commit, commit1.as_str());
    assert_eq!(info.tag_latest, "");
    assert_eq!(info.tag_head, None);
    assert_eq!(info.distance, "");
    assert_eq!(info.version_docker_ci, "null");
    // Check tag on HEAD
    repo.run(&["git", "tag", "v1.0.0"])?;
    let mut info = repo.info_get()?;
    info.is_push = Some(true);
    info.is_tag = Some(true);
    info.is_main = Some(true);
    info.eval()?;
    assert_eq!(info.commit, commit1.as_str());
    assert_eq!(info.git_describe_tags, "v1.0.0");
    assert_eq!(info.tag_latest, "v1.0.0");
    assert_eq!(info.distance, "0");
    assert_eq!(info.dash_distance, "-0");
    assert_eq!(info.tag_distance, "v1.0.0-0");
    assert_eq!(info.tag_head, Some("v1.0.0".to_string()));
    assert_eq!(info.tag_latest_ltrimv, "1.0.0");
    assert_eq!(info.tag_distance_ltrimv, "1.0.0-0");
    assert_eq!(info.tag_head_ltrimv, Some("1.0.0".to_string()));
    assert_eq!(info.version_tagged, Some("1.0.0".to_string()));
    assert_eq!(info.version_commit, Some("1.0.0".to_string()));
    assert_eq!(info.version_docker_ci, "1.0.0");
    // Check tag behind HEAD
    repo.file_write("bar.txt", "Hello again!")?;
    repo.run(&["git", "add", "bar.txt"])?;
    repo.run(&["git", "commit", "-m", "second commit"])?;
    let commit2 = git::head_commit(&repo.repo)?;
    let mut info = repo.info_get()?;
    info.is_push = Some(true);
    info.is_tag = Some(false);
    info.is_main = Some(true);
    repo.file_write("Cargo.toml", "[package]\nversion = \"9.7\"\n")?;
    info.parse_files(&repo.repo)?;
    info.eval()?;
    assert_eq!(info.commit, commit2.as_str());
    assert_eq!(info.git_describe_tags, format!("v1.0.0-1-g{}", commit2));
    assert_eq!(info.tag_latest, "v1.0.0");
    assert_eq!(info.distance, "1");
    assert_eq!(info.dash_distance, "-1");
    assert_eq!(info.tag_distance, "v1.0.0-1");
    assert_eq!(info.tag_head, None);
    assert_eq!(info.tag_latest_ltrimv, "1.0.0");
    assert_eq!(info.tag_distance_ltrimv, "1.0.0-1");
    assert_eq!(info.tag_head_ltrimv, None);
    assert_eq!(info.version_tagged, None);
    assert_eq!(info.version_commit, Some("1.0.0-1".to_string()));
    assert_eq!(info.version_docker_ci, "latest");
    assert_eq!(info.rust_crate_version, Some("9.7".to_string()));
    assert_eq!(
        info.version_mismatch,
        Some("file=Cargo.toml::Version mismatch: tag 1.0.0 != 9.7 from Cargo.toml".to_string())
    );
    // Check overrides
    info.parse_env(
        vec![
            ("OVERRIDE_VERSION_TAGGED", "a"),
            ("OVERRIDE_VERSION_COMMIT", "b"),
            ("OVERRIDE_VERSION_DOCKER_CI", "c"),
        ]
        .into_iter()
        .map(|(a, b)| (String::from(a), String::from(b))),
    );
    info.is_tag = Some(true);
    info.eval()?;
    assert_eq!(info.override_version_tagged, Some(String::from("a")));
    assert_eq!(info.override_version_commit, Some(String::from("b")));
    assert_eq!(info.override_version_docker_ci, Some(String::from("c")));
    assert_eq!(info.version_tagged, Some(String::from("a")));
    assert_eq!(info.version_commit, Some(String::from("b")));
    assert_eq!(info.version_docker_ci, String::from("c"));
    // Check new tag, on HEAD
    repo.run(&["git", "tag", "7.5"])?;
    repo.file_write("baz.txt", "Hello again again!")?;
    repo.run(&["git", "add", "baz.txt"])?;
    repo.run(&["git", "commit", "-m", "third commit"])?;
    let commit3 = git::head_commit(&repo.repo)?;
    let info = repo.info_get()?;
    assert_eq!(info.commit, commit3.as_str());
    assert_eq!(info.tag_latest, "7.5");
    assert_eq!(info.tag_latest_ltrimv, "7.5");
    assert_eq!(info.distance, "1");
    assert_eq!(info.version_docker_ci, "null");
    ghaction_version_gen::main()?;
    Ok(())
}

#[test]
fn toml1() -> Result<()> {
    let repo = TmpGit::new()?;
    assert_eq!(rust::crate_version(&repo.repo)?, None);
    repo.file_write("Cargo.toml", "")?;
    assert!(rust::crate_version(&repo.repo).is_err());
    repo.file_write("Cargo.toml", "[package]\n")?;
    assert!(rust::crate_version(&repo.repo).is_err());
    repo.file_write("Cargo.toml", "[package]\nversion = \"1.0\"\n")?;
    assert_eq!(rust::crate_version(&repo.repo)?, Some("1.0".to_string()));
    Ok(())
}
