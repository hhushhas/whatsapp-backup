# Changelog

## [0.1.0] - 2026-01-18

### Added
- Initial release
- AES-256-GCM encryption with Argon2id key derivation
- Passphrase storage in macOS Keychain (via `security` command)
- Automatic backup of WhatsApp Desktop data (~667MB compressed)
- GitHub private repo integration via `gh` CLI (skipped for files >100MB)
- Google Drive sync (optional, auto-detected)
- launchd scheduling (every 6 hours)
- Backup retention (7 days default)
- Restore functionality
- Commands: init, backup, restore, list, install, uninstall, status
