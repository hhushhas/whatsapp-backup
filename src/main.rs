mod backup;
mod config;
mod crypto;
mod git;
mod paths;
mod restore;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "whatsapp-backup")]
#[command(about = "Automated encrypted backup of WhatsApp Desktop chats")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize encryption key and GitHub repo
    Init,
    /// Run backup now
    Backup,
    /// Restore from a backup file
    Restore {
        /// Path to encrypted backup file
        file: PathBuf,
        /// Output directory (default: current directory)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// List available backups
    List,
    /// Install launchd schedule (runs every 6 hours)
    Install,
    /// Remove launchd schedule
    Uninstall,
    /// Show backup status and schedule info
    Status,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init => cmd_init(),
        Commands::Backup => cmd_backup(),
        Commands::Restore { file, output } => cmd_restore(file, output),
        Commands::List => cmd_list(),
        Commands::Install => cmd_install(),
        Commands::Uninstall => cmd_uninstall(),
        Commands::Status => cmd_status(),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn cmd_init() -> Result<()> {
    println!("WhatsApp Backup - Initial Setup\n");

    // Check if already initialized
    let config = Config::load()?;
    if config.initialized && crypto::has_passphrase() {
        println!("Already initialized. Use 'status' to check current configuration.");
        return Ok(());
    }

    // Verify WhatsApp data exists
    println!("Checking WhatsApp Desktop installation...");
    match paths::whatsapp_data_dir() {
        Ok(path) => println!("  Found: {}", path.display()),
        Err(e) => {
            eprintln!("  Warning: {}", e);
            eprintln!("  Continuing setup anyway - you can add WhatsApp later.\n");
        }
    }

    // Get passphrase
    println!("\nEnter a passphrase for encrypting your backups.");
    println!("This will be stored securely in your macOS Keychain.");
    println!("IMPORTANT: Remember this passphrase - you'll need it to restore backups!\n");

    print!("Passphrase: ");
    io::stdout().flush()?;
    let passphrase = rpassword_fallback()?;

    if passphrase.len() < 8 {
        anyhow::bail!("Passphrase must be at least 8 characters");
    }

    print!("Confirm passphrase: ");
    io::stdout().flush()?;
    let confirm = rpassword_fallback()?;

    if passphrase != confirm {
        anyhow::bail!("Passphrases don't match");
    }

    // Store passphrase in keychain
    crypto::store_passphrase(&passphrase)?;
    println!("\nPassphrase stored in Keychain");

    // Create GitHub repo
    println!("\nSetting up GitHub repository...");
    let repo_url = git::create_github_repo()?;
    println!("  Repository: {}", repo_url);

    // Save config
    let mut config = Config::load()?;
    config.set_initialized(Some(repo_url))?;

    // Create backup directory
    let backup_dir = paths::backup_dir()?;
    println!("\nLocal backup directory: {}", backup_dir.display());

    // Check Google Drive
    if let Some(drive) = paths::google_drive_dir() {
        println!("Google Drive detected: {}", drive.display());
    } else {
        println!("Google Drive not detected (optional)");
    }

    println!("\n Setup complete!");
    println!("Run 'whatsapp-backup backup' to create your first backup.");
    println!("Run 'whatsapp-backup install' to schedule automatic backups.\n");

    Ok(())
}

fn rpassword_fallback() -> Result<String> {
    // Simple stdin read for passphrase (terminal echo disabled would be better)
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn cmd_backup() -> Result<()> {
    println!("Starting WhatsApp backup...\n");
    let backup_path = backup::run_backup()?;
    println!("\nBackup saved: {}", backup_path.display());
    Ok(())
}

fn cmd_restore(file: PathBuf, output: Option<PathBuf>) -> Result<()> {
    let output_dir = output.unwrap_or_else(|| PathBuf::from("."));

    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir)?;
    }

    restore::restore_backup(&file, &output_dir)?;
    Ok(())
}

fn cmd_list() -> Result<()> {
    let backups = restore::list_backups()?;

    if backups.is_empty() {
        println!("No backups found.");
        println!("Run 'whatsapp-backup backup' to create one.");
        return Ok(());
    }

    println!("Available backups:\n");
    for (name, size, _modified) in backups {
        println!("  {} ({:.2} MB)", name, size as f64 / 1_000_000.0);
    }

    let backup_dir = paths::backup_dir()?;
    println!("\nBackup directory: {}", backup_dir.display());

    Ok(())
}

fn cmd_install() -> Result<()> {
    let config = Config::load()?;
    if !config.initialized {
        anyhow::bail!("Not initialized. Run 'whatsapp-backup init' first.");
    }

    let plist_path = paths::launchd_plist_path()?;
    let log_dir = paths::log_dir()?;

    // Get path to binary
    let binary_path = std::env::current_exe()?;

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.user.whatsapp-backup</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>backup</string>
    </array>
    <key>StartInterval</key>
    <integer>21600</integer>
    <key>StandardOutPath</key>
    <string>{}/stdout.log</string>
    <key>StandardErrorPath</key>
    <string>{}/stderr.log</string>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
        binary_path.display(),
        log_dir.display(),
        log_dir.display()
    );

    std::fs::write(&plist_path, plist_content)?;

    // Load the plist
    let output = Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If already loaded, unload first then reload
        if stderr.contains("already loaded") {
            Command::new("launchctl")
                .args(["unload", &plist_path.to_string_lossy()])
                .output()?;
            Command::new("launchctl")
                .args(["load", &plist_path.to_string_lossy()])
                .output()?;
        }
    }

    println!("Installed launchd schedule");
    println!("  Plist: {}", plist_path.display());
    println!("  Logs: {}", log_dir.display());
    println!("  Interval: Every 6 hours");
    println!("\nBackups will run automatically. Check status with 'whatsapp-backup status'");

    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    let plist_path = paths::launchd_plist_path()?;

    if plist_path.exists() {
        Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()])
            .output()?;

        std::fs::remove_file(&plist_path)?;
        println!("Removed launchd schedule");
    } else {
        println!("No schedule installed");
    }

    Ok(())
}

fn cmd_status() -> Result<()> {
    let config = Config::load()?;

    println!("WhatsApp Backup Status\n");

    // Initialization status
    if config.initialized {
        println!("Initialized: Yes");
    } else {
        println!("Initialized: No (run 'whatsapp-backup init')");
        return Ok(());
    }

    // Keychain status
    if crypto::has_passphrase() {
        println!("Encryption key: Stored in Keychain");
    } else {
        println!("Encryption key: Missing (run 'whatsapp-backup init')");
    }

    // GitHub repo
    if let Some(repo) = &config.github_repo {
        println!("GitHub repo: {}", repo);
    }

    // Last backup
    if let Some(last) = config.last_backup {
        println!("Last backup: {}", last.format("%Y-%m-%d %H:%M:%S UTC"));
    } else {
        println!("Last backup: Never");
    }

    // WhatsApp data
    match paths::whatsapp_data_dir() {
        Ok(path) => println!("WhatsApp data: {}", path.display()),
        Err(_) => println!("WhatsApp data: Not found"),
    }

    // Google Drive
    if let Some(drive) = paths::google_drive_dir() {
        println!("Google Drive: {}", drive.display());
    } else {
        println!("Google Drive: Not detected");
    }

    // Schedule status
    let plist_path = paths::launchd_plist_path()?;
    if plist_path.exists() {
        println!("\nSchedule: Installed (every {} hours)", config.backup_interval_hours);

        // Check if running
        let output = Command::new("launchctl")
            .args(["list"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("com.user.whatsapp-backup") {
            println!("Service: Running");
        } else {
            println!("Service: Not running (try 'whatsapp-backup install')");
        }
    } else {
        println!("\nSchedule: Not installed (run 'whatsapp-backup install')");
    }

    // Backup count
    let backups = restore::list_backups()?;
    println!("\nLocal backups: {}", backups.len());

    Ok(())
}
