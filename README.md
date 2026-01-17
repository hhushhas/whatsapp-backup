# whatsapp-backup

Encrypted backup of WhatsApp Desktop (macOS) to local storage + Google Drive.

```
WhatsApp Desktop → tar.gz → AES-256-GCM → ~/.whatsapp-backups/
                                       → Google Drive (if installed)
```

## Commands

| Command          | Description                                             |
| ---------------- | ------------------------------------------------------- |
| `init`           | Set passphrase (stored in Keychain), create GitHub repo |
| `backup`         | Archive + encrypt + save                                |
| `restore <file>` | Decrypt + extract to current dir                        |
| `list`           | Show available backups                                  |
| `install`        | Enable 6-hour launchd schedule                          |
| `uninstall`      | Remove schedule                                         |
| `status`         | Show config, last backup, schedule state                |

## Quick Start

```bash
cargo build --release
./target/release/whatsapp-backup init      # Set passphrase, creates GitHub repo
./target/release/whatsapp-backup backup    # First backup
./target/release/whatsapp-backup install   # Schedule every 6 hours
```

## Project Structure

```
src/
├── main.rs      # CLI entry (clap)
├── backup.rs    # Archive → encrypt → save → cleanup
├── restore.rs   # Decrypt → extract
├── crypto.rs    # AES-256-GCM, Argon2id, Keychain (security cmd)
├── config.rs    # JSON config in ~/.config/whatsapp-backup/
├── git.rs       # GitHub repo via gh CLI
└── paths.rs     # WhatsApp/Drive/backup path detection
```

## Paths

| What          | Where                                                            |
| ------------- | ---------------------------------------------------------------- |
| WhatsApp data | `~/Library/Group Containers/group.net.whatsapp.WhatsApp.shared/` |
| Backups       | `~/.whatsapp-backups/*.enc`                                      |
| Config        | `~/.config/whatsapp-backup/config.json`                          |
| Logs          | `~/Library/Logs/whatsapp-backup/`                                |
| GitHub repo   | `~/whatsapp-backup-encrypted/`                                   |
| launchd plist | `~/Library/LaunchAgents/com.user.whatsapp-backup.plist`          |

## Encryption

| Property           | Value                                     |
| ------------------ | ----------------------------------------- |
| Algorithm          | AES-256-GCM (authenticated)               |
| Key derivation     | Argon2id from passphrase                  |
| Passphrase storage | macOS Keychain                            |
| File format        | `[salt:16][nonce:12][ciphertext][tag:16]` |

## Backup Flow

1. Check WhatsApp data exists
2. Create `tar.gz` archive
3. Encrypt with passphrase from Keychain
4. Save to `~/.whatsapp-backups/YYYY-MM-DD_HH-MM-SS.enc`
5. Copy to Google Drive (if detected)
6. Skip GitHub push if >100MB (GitHub limit)
7. Delete backups older than 7 days

## Restore

```bash
whatsapp-backup restore ~/.whatsapp-backups/2026-01-17_19-41-14.enc -o ./restore

# Manual restore to WhatsApp:
# 1. Quit WhatsApp Desktop
# 2. cp -r ./restore/whatsapp-data/* ~/Library/Group\ Containers/group.net.whatsapp.WhatsApp.shared/
# 3. Reopen WhatsApp
```

## Dependencies

| Crate              | Purpose              |
| ------------------ | -------------------- |
| clap               | CLI parsing          |
| aes-gcm            | Encryption           |
| argon2             | Key derivation       |
| tar + flate2       | Archive creation     |
| chrono             | Timestamps           |
| dirs               | Path detection       |
| serde + serde_json | Config serialization |

**External:** `gh` CLI (GitHub repo creation), `git` (push), `security` (Keychain)

## Limitations

- GitHub push skipped for files >100MB (WhatsApp data often 500MB+)
- Google Drive sync requires manual installation of Google Drive for Desktop
- macOS only (uses Keychain, launchd)

## Config (config.json)

```json
{
  "initialized": true,
  "github_repo": "git@github.com:user/whatsapp-backup-encrypted.git",
  "last_backup": "2026-01-17T19:41:37Z",
  "retention_days": 7,
  "backup_interval_hours": 6
}
```

## For AI Agents

**Common tasks:**

| Task                 | Approach                                                                                                         |
| -------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Debug backup failure | Check `status`, verify Keychain entry with `security find-generic-password -s whatsapp-backup -a encryption-key` |
| Change schedule      | Edit plist or modify `backup_interval_hours` in config, re-run `install`                                         |
| Force backup         | Run `whatsapp-backup backup` directly                                                                            |
| Check logs           | `tail ~/Library/Logs/whatsapp-backup/stdout.log`                                                                 |
| Reset encryption     | Delete Keychain entry, re-run `init`                                                                             |

**Key files to read first:** `src/backup.rs` (main logic), `src/paths.rs` (all paths)
