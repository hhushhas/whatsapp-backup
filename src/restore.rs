use crate::{backup::Manifest, crypto, paths};
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use tar::Archive;

/// Reads manifest file
fn read_manifest(manifest_path: &Path) -> Result<Manifest> {
    let file = File::open(manifest_path)?;
    let manifest: Manifest = serde_json::from_reader(file)?;
    Ok(manifest)
}

/// Reassembles chunks into original encrypted file, verifying SHA256
fn reassemble_chunks(manifest_path: &Path, output_path: &Path) -> Result<()> {
    let manifest = read_manifest(manifest_path)?;
    let parent = manifest_path.parent().context("No parent directory")?;

    let mut output = BufWriter::new(File::create(output_path)?);
    let mut hasher = Sha256::new();

    for chunk_info in &manifest.chunks {
        let chunk_path = parent.join(&chunk_info.name);
        if !chunk_path.exists() {
            anyhow::bail!("Missing chunk: {}", chunk_info.name);
        }

        let mut chunk_file = BufReader::new(File::open(&chunk_path)?);
        let mut buffer = Vec::new();
        chunk_file.read_to_end(&mut buffer)?;

        hasher.update(&buffer);
        output.write_all(&buffer)?;
    }
    output.flush()?;

    // Verify SHA256
    let computed_hash = format!("{:x}", hasher.finalize());
    if computed_hash != manifest.sha256 {
        anyhow::bail!(
            "SHA256 mismatch! Expected {}, got {}",
            manifest.sha256,
            computed_hash
        );
    }

    Ok(())
}

/// Restores a backup to a specified directory
pub fn restore_backup(backup_path: &Path, output_dir: &Path) -> Result<()> {
    if !backup_path.exists() {
        anyhow::bail!("Backup file not found: {}", backup_path.display());
    }

    // Determine if this is a chunked backup
    let file_name = backup_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let encrypted_file = if file_name.ends_with(".manifest") {
        // Chunked backup - reassemble first
        println!("Detected chunked backup, reassembling...");
        let manifest = read_manifest(backup_path)?;
        let reassembled_path = output_dir.join(format!("{}.enc", manifest.timestamp));
        reassemble_chunks(backup_path, &reassembled_path)?;
        println!("  Reassembled {} chunks", manifest.chunks.len());
        reassembled_path
    } else {
        backup_path.to_path_buf()
    };

    // Get passphrase
    let passphrase = crypto::get_passphrase()?;

    // Create temp file for decrypted archive
    let temp_archive = output_dir.join("temp_restore.tar.gz");

    println!("Decrypting backup...");
    crypto::decrypt_file(&encrypted_file, &temp_archive, &passphrase)?;

    // Clean up reassembled file if we created one
    if encrypted_file != backup_path {
        std::fs::remove_file(&encrypted_file).ok();
    }

    // Extract archive
    println!("Extracting...");
    let file = File::open(&temp_archive).context("Failed to open decrypted archive")?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    archive
        .unpack(output_dir)
        .context("Failed to extract backup")?;

    // Remove temp archive
    std::fs::remove_file(&temp_archive)?;

    println!("Restored to: {}", output_dir.display());
    println!(
        "\nNote: The data is extracted to {}/whatsapp-data/",
        output_dir.display()
    );
    println!("To restore to WhatsApp Desktop, quit WhatsApp first, then copy the data:");
    println!(
        "  cp -r {}/whatsapp-data/* ~/Library/Group\\ Containers/group.net.whatsapp.WhatsApp.shared/",
        output_dir.display()
    );

    Ok(())
}

/// Lists available backups (grouping chunks as single entries)
pub fn list_backups() -> Result<Vec<(String, u64, std::time::SystemTime)>> {
    let backup_dir = paths::backup_dir()?;
    let mut backups = Vec::new();
    let mut seen_timestamps: HashMap<String, bool> = HashMap::new();

    for entry in std::fs::read_dir(&backup_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Handle manifest files (chunked backups)
        if name.ends_with(".manifest") {
            if let Ok(manifest) = read_manifest(&path) {
                let timestamp = &manifest.timestamp;
                if !seen_timestamps.contains_key(timestamp) {
                    seen_timestamps.insert(timestamp.clone(), true);
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            // Show manifest name but with total original size
                            backups.push((
                                name.to_string(),
                                manifest.original_size,
                                modified,
                            ));
                        }
                    }
                }
            }
        }
        // Handle regular .enc files (non-chunked)
        else if name.ends_with(".enc") && !name.contains(".enc.") {
            // Extract timestamp from filename (e.g., "2026-01-17_19-41-14.enc")
            let timestamp = name.trim_end_matches(".enc");
            if !seen_timestamps.contains_key(timestamp) {
                seen_timestamps.insert(timestamp.to_string(), true);
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        backups.push((
                            name.to_string(),
                            metadata.len(),
                            modified,
                        ));
                    }
                }
            }
        }
    }

    // Sort by modification time, newest first
    backups.sort_by(|a, b| b.2.cmp(&a.2));

    Ok(backups)
}
