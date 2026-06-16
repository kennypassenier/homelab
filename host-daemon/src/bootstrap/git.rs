// Git sparse checkout and SSH key setup inside a newly created LXC.

use super::container::pct_exec;

/// Clone the homelab repo with a sparse checkout restricted to the given stack.
pub fn setup_git_sparse_checkout(vmid: u32, stack_name: &str) -> Result<(), String> {
    let pat = std::env::var("GITHUB_PAT")
        .or_else(|_| std::env::var("GITOPS_REPO_TOKEN"))
        .map_err(|_| "GITHUB_PAT or GITOPS_REPO_TOKEN not set on HOST".to_string())?;

    let repo_url = std::env::var("GITOPS_REPO_URL")
        .unwrap_or_else(|_| "https://github.com/kennypassenier/homelab.git".to_string());

    let auth_url = if repo_url.starts_with("https://") {
        format!("https://{}@{}", pat, &repo_url["https://".len()..])
    } else {
        repo_url.clone()
    };

    let script = format!(
        r#"
set -euo pipefail
GITOPS_DIR="/opt/gitops"
rm -rf "$GITOPS_DIR"
git clone --filter=blob:none --no-checkout "{auth_url}" "$GITOPS_DIR"
cd "$GITOPS_DIR"
git sparse-checkout init --cone
git sparse-checkout set "stacks/{stack}"
git checkout main
echo "{auth_url}" > ~/.git-credentials
chmod 600 ~/.git-credentials
git config credential.helper store
echo "Sparse checkout done for stack: {stack}"
"#,
        auth_url = auth_url,
        stack = stack_name,
    );

    pct_exec(vmid, &script)?;
    Ok(())
}

/// Fetch SSH public keys from GitHub and place them in /root/.ssh/authorized_keys.
pub fn install_ssh_keys(vmid: u32, github_username: &str) -> Result<(), String> {
    let script = format!(
        r#"
set -euo pipefail
mkdir -p /root/.ssh && chmod 700 /root/.ssh
curl -fsSL "https://github.com/{user}.keys" > /root/.ssh/authorized_keys
chmod 600 /root/.ssh/authorized_keys
echo "SSH keys installed for GitHub user: {user}"
"#,
        user = github_username,
    );

    pct_exec(vmid, &script)?;
    Ok(())
}
