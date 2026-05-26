//! GitOps and YAML linting logic for Homelab Client.
use anyhow::Result;
use git2::{Repository, IndexAddOption, PushOptions, Cred, RemoteCallbacks};
use std::path::Path;
use serde_yaml;

/// Pre-flight check: parse the given YAML file and return an error if invalid.
pub fn pre_flight_check(file_path: &str) -> Result<()> {
    let content = std::fs::read_to_string(file_path)?;
    serde_yaml::from_str::<serde_yaml::Value>(&content)?;
    Ok(())
}

/// Commit and push all changes in the repo to origin/main, using SSH agent for credentials.
pub fn commit_and_push(repo_path: &str, commit_message: &str) -> Result<()> {
    let repo = Repository::open(repo_path)?;
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let oid = index.write_tree()?;
    let signature = repo.signature()?;
    let parent_commit = repo.head()?.peel_to_commit()?;
    let tree = repo.find_tree(oid)?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        commit_message,
        &tree,
        &[&parent_commit],
    )?;
    let mut cb = RemoteCallbacks::new();
    cb.credentials(|_url, username_from_url, _allowed_types| {
        let username = username_from_url.unwrap_or("git");
        Cred::ssh_key_from_agent(username)
    });
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(cb);
    let mut remote = repo.find_remote("origin")?;
    remote.push(&["refs/heads/main:refs/heads/main"], Some(&mut push_opts))?;
    Ok(())
}
