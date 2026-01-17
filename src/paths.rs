use anyhow::{Context, Result};
use std::path::PathBuf;

const WHATSAPP_CONTAINER: &str =
    "Library/Group Containers/group.net.whatsapp.WhatsApp.shared";

const GOOGLE_DRIVE_PATHS: &[&str] = &[
    "Library/CloudStorage/GoogleDrive-*/My Drive",
    "Google Drive/My Drive",
];

pub fn whatsapp_data_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to detect home directory")?;
    let path = home.join(WHATSAPP_CONTAINER);

    if path.exists() {
        Ok(path)
    } else {
        anyhow::bail!(
            "WhatsApp Desktop data not found at: {}\n\
             Make sure WhatsApp Desktop is installed and has been opened at least once.",
            path.display()
        )
    }
}

pub fn backup_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to detect home directory")?;
    let path = home.join(".whatsapp-backups");

    if !path.exists() {
        std::fs::create_dir_all(&path)
            .with_context(|| format!("Failed to create backup directory: {}", path.display()))?;
    }

    Ok(path)
}

pub fn github_repo_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to detect home directory")?;
    let path = home.join("whatsapp-backup-encrypted");
    Ok(path)
}

pub fn google_drive_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    for pattern in GOOGLE_DRIVE_PATHS {
        if pattern.contains('*') {
            // Handle glob pattern for CloudStorage
            let base = home.join("Library/CloudStorage");
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("GoogleDrive-") {
                        let drive_path = entry.path().join("My Drive");
                        if drive_path.exists() {
                            return Some(drive_path);
                        }
                    }
                }
            }
        } else {
            let path = home.join(pattern);
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

pub fn launchd_plist_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to detect home directory")?;
    Ok(home.join("Library/LaunchAgents/com.user.whatsapp-backup.plist"))
}

pub fn log_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to detect home directory")?;
    let path = home.join("Library/Logs/whatsapp-backup");

    if !path.exists() {
        std::fs::create_dir_all(&path)
            .with_context(|| format!("Failed to create log directory: {}", path.display()))?;
    }

    Ok(path)
}

pub fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to detect home directory")?;
    let path = home.join(".config/whatsapp-backup");

    if !path.exists() {
        std::fs::create_dir_all(&path)
            .with_context(|| format!("Failed to create config directory: {}", path.display()))?;
    }

    Ok(path)
}
