use crate::{config::Config, crypto, git, paths};
use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tar::Builder;

/// 90MB chunks (under GitHub's 100MB limit)
const CHUNK_SIZE: u64 = 90_000_000;

#[derive(Serialize, Deserialize)]
pub struct ChunkInfo {
    pub name: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub version: u8,
    pub timestamp: String,
    pub original_size: u64,
    pub chunk_size: u64,
    pub chunks: Vec<ChunkInfo>,
    pub sha256: String,
}

/// Computes SHA256 hash of a file
fn compute_sha256(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Splits a file into chunks, returns paths to chunks and manifest
fn split_into_chunks(file: &Path, timestamp: &str) -> Result<(Vec<PathBuf>, PathBuf)> {
    let parent = file.parent().context("No parent directory")?;
    let original_size = std::fs::metadata(file)?.len();
    let sha256 = compute_sha256(file)?;

    let mut input = BufReader::new(File::open(file)?);
    let mut chunks = Vec::new();
    let mut chunk_infos = Vec::new();
    let mut chunk_num = 1u32;
    let mut buffer = vec![0u8; CHUNK_SIZE as usize];

    loop {
        let bytes_read = input.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        let chunk_name = format!("{}.enc.{:03}", timestamp, chunk_num);
        let chunk_path = parent.join(&chunk_name);

        let mut output = BufWriter::new(File::create(&chunk_path)?);
        output.write_all(&buffer[..bytes_read])?;
        output.flush()?;

        chunk_infos.push(ChunkInfo {
            name: chunk_name,
            size: bytes_read as u64,
        });
        chunks.push(chunk_path);
        chunk_num += 1;
    }

    // Create manifest
    let manifest = Manifest {
        version: 1,
        timestamp: timestamp.to_string(),
        original_size,
        chunk_size: CHUNK_SIZE,
        chunks: chunk_infos,
        sha256,
    };

    let manifest_path = parent.join(format!("{}.enc.manifest", timestamp));
    let manifest_file = File::create(&manifest_path)?;
    serde_json::to_writer_pretty(manifest_file, &manifest)?;

    Ok((chunks, manifest_path))
}

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

/// Cleans up old backups beyond retention period (including chunks and manifests)
fn cleanup_old_backups(backup_dir: &Path, retention_days: u32) -> Result<()> {
    let cutoff = Utc::now() - Duration::days(retention_days as i64);

    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Match .enc files, chunk files (.enc.001, etc), and manifests
        let is_backup_file = name.ends_with(".enc")
            || name.contains(".enc.")
            || name.ends_with(".manifest");

        if is_backup_file {
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

    // Push to GitHub - chunk if needed
    let encrypted_size = std::fs::metadata(&encrypted_path)?.len();

    if git::is_repo_initialized() {
        let commit_msg = format!("Backup {}", timestamp);

        if encrypted_size > CHUNK_SIZE {
            // Split into chunks for GitHub
            println!(
                "Splitting into chunks ({:.0} MB > {:.0} MB limit)...",
                encrypted_size as f64 / 1_000_000.0,
                CHUNK_SIZE as f64 / 1_000_000.0
            );
            let (chunks, manifest) = split_into_chunks(&encrypted_path, &timestamp.to_string())?;
            println!("  Created {} chunks + manifest", chunks.len());

            // Collect all files to push
            let mut files_to_push: Vec<PathBuf> = chunks;
            files_to_push.push(manifest);

            println!("Pushing to GitHub...");
            git::commit_and_push_files(&files_to_push, &commit_msg)?;
            println!("  Pushed {} files to GitHub", files_to_push.len());

            // Clean up chunk files from local backup dir (keep original .enc)
            for file in &files_to_push {
                std::fs::remove_file(file).ok();
            }
        } else {
            println!("Pushing to GitHub...");
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
