use crate::paths;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const REPO_NAME: &str = "whatsapp-backup-encrypted";

/// Creates a private GitHub repo using gh CLI
pub fn create_github_repo() -> Result<String> {
    let repo_path = paths::github_repo_dir()?;

    // Check if gh is installed
    let gh_check = Command::new("gh").arg("--version").output();
    if gh_check.is_err() {
        anyhow::bail!(
            "GitHub CLI (gh) not found. Install with: brew install gh\n\
             Then authenticate with: gh auth login"
        );
    }

    // Check if repo already exists on GitHub
    let check_output = Command::new("gh")
        .args(["repo", "view", REPO_NAME])
        .output()
        .context("Failed to check GitHub repo")?;

    let repo_url = if check_output.status.success() {
        // Repo exists, get its URL
        let output = Command::new("gh")
            .args(["repo", "view", REPO_NAME, "--json", "sshUrl", "-q", ".sshUrl"])
            .output()
            .context("Failed to get repo URL")?;

        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        // Create new private repo
        let output = Command::new("gh")
            .args([
                "repo",
                "create",
                REPO_NAME,
                "--private",
                "--description",
                "Encrypted WhatsApp Desktop backups",
            ])
            .output()
            .context("Failed to create GitHub repo")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create GitHub repo: {}", stderr);
        }

        // Get the SSH URL
        let url_output = Command::new("gh")
            .args(["repo", "view", REPO_NAME, "--json", "sshUrl", "-q", ".sshUrl"])
            .output()
            .context("Failed to get repo URL")?;

        String::from_utf8_lossy(&url_output.stdout).trim().to_string()
    };

    // Create local directory if needed
    if !repo_path.exists() {
        std::fs::create_dir_all(&repo_path)
            .with_context(|| format!("Failed to create repo dir: {}", repo_path.display()))?;
    }

    // Initialize git repo locally if not already done
    if !repo_path.join(".git").exists() {
        let output = Command::new("git")
            .args(["init"])
            .current_dir(&repo_path)
            .output()
            .context("Failed to init git repo")?;

        if !output.status.success() {
            anyhow::bail!("git init failed");
        }

        // Set default branch to main
        Command::new("git")
            .args(["checkout", "-b", "main"])
            .current_dir(&repo_path)
            .output()
            .ok();

        // Add remote
        Command::new("git")
            .args(["remote", "add", "origin", &repo_url])
            .current_dir(&repo_path)
            .output()
            .context("Failed to add remote")?;
    }

    Ok(repo_url)
}

/// Commits and pushes a backup file using git CLI
pub fn commit_and_push(file_path: &Path, message: &str) -> Result<()> {
    commit_and_push_files(&[file_path.to_path_buf()], message)
}

/// Removes old backup chunks from the repo before pushing new ones
fn remove_old_chunks(repo_dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(repo_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Remove old chunks (.enc.001, etc) and manifests
        if (name.contains(".enc.") && !name.ends_with(".enc"))
            || name.ends_with(".manifest")
        {
            std::fs::remove_file(&path).ok();
        }
    }
    Ok(())
}

/// Commits and pushes multiple files using git CLI
pub fn commit_and_push_files(files: &[PathBuf], message: &str) -> Result<()> {
    let repo_dir = paths::github_repo_dir()?;

    // Remove old chunks before adding new ones
    remove_old_chunks(&repo_dir)?;

    // Copy all files to repo
    let mut file_names = Vec::new();
    for file_path in files {
        let file_name = file_path.file_name().context("Invalid file path")?;
        let dest_path = repo_dir.join(file_name);
        std::fs::copy(file_path, &dest_path).context("Failed to copy file to repo")?;
        file_names.push(file_name.to_string_lossy().to_string());
    }

    // git add all files (use -A to also stage deletions of old chunks)
    let output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo_dir)
        .output()
        .context("Failed to run git add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git add failed: {}", stderr);
    }

    // git commit
    let output = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(&repo_dir)
        .output()
        .context("Failed to run git commit")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "nothing to commit" error
        if !stderr.contains("nothing to commit") {
            anyhow::bail!("git commit failed: {}", stderr);
        }
    }

    // git push
    let output = Command::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(&repo_dir)
        .output()
        .context("Failed to run git push")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git push failed: {}", stderr);
    }

    Ok(())
}

/// Checks if the git repo is set up
pub fn is_repo_initialized() -> bool {
    paths::github_repo_dir()
        .map(|p| p.join(".git").exists())
        .unwrap_or(false)
}
