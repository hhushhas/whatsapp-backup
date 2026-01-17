use crate::{config::Config, crypto, git, paths};
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::File;
use std::path::{Path, PathBuf};
use tar::Builder;

/// Creates a compressed tar archive of WhatsApp data
fn create_archive(whatsapp_dir: &Path, output: &Path) -> Result<()> {
    let file = File::create(output)
        .with_context(|| format!("Failed to create archive: {}", output.display()))?;

    let encoder = GzEncoder::new(file, Compression::default());
    let mut archive = Builder::new(encoder);

    archive
        .append_dir_all("whatsapp-data", whatsapp_dir)
        .context("Failed to add WhatsApp data to archive")?;

    archive.finish().context("Failed to finalize archive")?;

    Ok(())
}

/// Cleans up old backups beyond retention period
fn cleanup_old_backups(backup_dir: &Path, retention_days: u32) -> Result<()> {
    let cutoff = Utc::now() - Duration::days(retention_days as i64);

    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let path = entry.path();

        if let Some(ext) = path.extension() {
            if ext == "enc" {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let modified_time: chrono::DateTime<Utc> = modified.into();
                        if modified_time < cutoff {
                            std::fs::remove_file(&path).ok();
                            println!("  Removed old backup: {}", path.display());
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Copies backup to Google Drive if available
fn copy_to_google_drive(backup_file: &Path) -> Result<Option<PathBuf>> {
    let Some(drive_dir) = paths::google_drive_dir() else {
        return Ok(None);
    };

    let backup_folder = drive_dir.join("WhatsApp-Backups");
    if !backup_folder.exists() {
        std::fs::create_dir_all(&backup_folder)?;
    }

    let file_name = backup_file.file_name().context("Invalid backup filename")?;
    let dest = backup_folder.join(file_name);

    std::fs::copy(backup_file, &dest)?;

    Ok(Some(dest))
}

/// Main backup function
pub fn run_backup() -> Result<PathBuf> {
    let mut config = Config::load()?;

    if !config.initialized {
        anyhow::bail!(
            "Not initialized. Run 'whatsapp-backup init' first to set up encryption and GitHub."
        );
    }

    // Get passphrase from keychain
    let passphrase = crypto::get_passphrase()?;

    // Check WhatsApp data exists
    println!("Checking WhatsApp data...");
    let whatsapp_dir = paths::whatsapp_data_dir()?;
    println!("  Found: {}", whatsapp_dir.display());

    // Create timestamp for filename
    let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let backup_dir = paths::backup_dir()?;

    // Create temporary archive
    let archive_path = backup_dir.join(format!("{}.tar.gz", timestamp));
    println!("Creating archive...");
    create_archive(&whatsapp_dir, &archive_path)?;
    println!("  Archive created: {}", archive_path.display());

    // Get archive size for reporting
    let archive_size = std::fs::metadata(&archive_path)?.len();
    println!("  Size: {:.2} MB", archive_size as f64 / 1_000_000.0);

    // Encrypt archive
    let encrypted_path = backup_dir.join(format!("{}.enc", timestamp));
    println!("Encrypting...");
    crypto::encrypt_file(&archive_path, &encrypted_path, &passphrase)?;
    println!("  Encrypted: {}", encrypted_path.display());

    // Remove unencrypted archive
    std::fs::remove_file(&archive_path)?;

    // Push to GitHub (skip if file > 100MB due to GitHub limits)
    let encrypted_size = std::fs::metadata(&encrypted_path)?.len();
    const GITHUB_MAX_SIZE: u64 = 100_000_000; // 100MB

    if git::is_repo_initialized() {
        if encrypted_size > GITHUB_MAX_SIZE {
            println!(
                "Skipping GitHub push (file size {:.0} MB exceeds 100MB limit)",
                encrypted_size as f64 / 1_000_000.0
            );
            println!("  Backup saved locally and to Google Drive only");
        } else {
            println!("Pushing to GitHub...");
            let commit_msg = format!("Backup {}", timestamp);
            git::commit_and_push(&encrypted_path, &commit_msg)?;
            println!("  Pushed to GitHub");
        }
    }

    // Copy to Google Drive
    if let Some(drive_path) = copy_to_google_drive(&encrypted_path)? {
        println!("Copied to Google Drive: {}", drive_path.display());
    }

    // Cleanup old backups
    println!("Cleaning up old backups...");
    cleanup_old_backups(&backup_dir, config.retention_days)?;

    // Cleanup old backups in GitHub repo
    if let Ok(repo_dir) = paths::github_repo_dir() {
        cleanup_old_backups(&repo_dir, config.retention_days)?;
    }

    // Update config
    config.update_last_backup()?;

    println!(
        "Backup complete! Size: {:.2} MB",
        encrypted_size as f64 / 1_000_000.0
    );

    Ok(encrypted_path)
}
