use crate::{crypto, paths};
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs::File;
use std::path::Path;
use tar::Archive;

/// Restores a backup to a specified directory
pub fn restore_backup(encrypted_file: &Path, output_dir: &Path) -> Result<()> {
    if !encrypted_file.exists() {
        anyhow::bail!("Backup file not found: {}", encrypted_file.display());
    }

    // Get passphrase
    let passphrase = crypto::get_passphrase()?;

    // Create temp file for decrypted archive
    let temp_archive = output_dir.join("temp_restore.tar.gz");

    println!("Decrypting backup...");
    crypto::decrypt_file(encrypted_file, &temp_archive, &passphrase)?;

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

/// Lists available backups
pub fn list_backups() -> Result<Vec<(String, u64, std::time::SystemTime)>> {
    let backup_dir = paths::backup_dir()?;
    let mut backups = Vec::new();

    for entry in std::fs::read_dir(backup_dir)? {
        let entry = entry?;
        let path = entry.path();

        if let Some(ext) = path.extension() {
            if ext == "enc" {
                if let (Some(name), Ok(metadata)) =
                    (path.file_name(), entry.metadata())
                {
                    if let Ok(modified) = metadata.modified() {
                        backups.push((
                            name.to_string_lossy().to_string(),
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
