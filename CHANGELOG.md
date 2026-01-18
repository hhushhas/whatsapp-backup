# Changelog

## [0.2.0] - 2026-01-18

### Added
- Chunked uploads for GitHub: large backups (>90MB) are split into 90MB chunks
- Manifest files with SHA256 checksums for integrity verification
- Automatic reassembly of chunked backups during restore

### Changed
- `list` command now groups chunks as single backup entries
- Old chunks are automatically removed before pushing new backups
- Cleanup logic handles `.enc.001`, `.manifest` files alongside `.enc`
- Chunks pushed incrementally (one commit per file) to avoid GitHub rate limits

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
