use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use rand::RngCore;
use std::path::Path;
use std::process::Command;

const KEYCHAIN_SERVICE: &str = "whatsapp-backup";
const KEYCHAIN_ACCOUNT: &str = "encryption-key";
const NONCE_SIZE: usize = 12;
const SALT_SIZE: usize = 16;

/// Derives a 256-bit key from passphrase using Argon2id
fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let salt_string = SaltString::encode_b64(salt)
        .map_err(|e| anyhow::anyhow!("Failed to encode salt: {}", e))?;

    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(passphrase.as_bytes(), &salt_string)
        .map_err(|e| anyhow::anyhow!("Failed to derive key: {}", e))?;

    let hash_output = hash.hash.context("No hash output")?;
    let bytes = hash_output.as_bytes();

    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes[..32]);
    Ok(key)
}

/// Encrypts data using AES-256-GCM
/// Format: [salt (16 bytes)][nonce (12 bytes)][ciphertext][tag (16 bytes)]
pub fn encrypt(data: &[u8], passphrase: &str) -> Result<Vec<u8>> {
    let mut salt = [0u8; SALT_SIZE];
    OsRng.fill_bytes(&mut salt);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    let mut result = Vec::with_capacity(SALT_SIZE + NONCE_SIZE + ciphertext.len());
    result.extend_from_slice(&salt);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);

    Ok(result)
}

/// Decrypts data encrypted with AES-256-GCM
pub fn decrypt(encrypted: &[u8], passphrase: &str) -> Result<Vec<u8>> {
    if encrypted.len() < SALT_SIZE + NONCE_SIZE + 16 {
        anyhow::bail!("Encrypted data too short");
    }

    let salt = &encrypted[..SALT_SIZE];
    let nonce_bytes = &encrypted[SALT_SIZE..SALT_SIZE + NONCE_SIZE];
    let ciphertext = &encrypted[SALT_SIZE + NONCE_SIZE..];

    let key = derive_key(passphrase, salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("Decryption failed - wrong passphrase or corrupted data"))
}

/// Encrypts a file and writes to output path
pub fn encrypt_file(input: &Path, output: &Path, passphrase: &str) -> Result<()> {
    let data = std::fs::read(input)
        .with_context(|| format!("Failed to read file: {}", input.display()))?;

    let encrypted = encrypt(&data, passphrase)?;

    std::fs::write(output, &encrypted)
        .with_context(|| format!("Failed to write encrypted file: {}", output.display()))?;

    Ok(())
}

/// Decrypts a file and writes to output path
pub fn decrypt_file(input: &Path, output: &Path, passphrase: &str) -> Result<()> {
    let encrypted = std::fs::read(input)
        .with_context(|| format!("Failed to read encrypted file: {}", input.display()))?;

    let decrypted = decrypt(&encrypted, passphrase)?;

    std::fs::write(output, &decrypted)
        .with_context(|| format!("Failed to write decrypted file: {}", output.display()))?;

    Ok(())
}

/// Stores passphrase in macOS Keychain using security command
pub fn store_passphrase(passphrase: &str) -> Result<()> {
    // First try to delete any existing entry
    let _ = Command::new("security")
        .args([
            "delete-generic-password",
            "-s", KEYCHAIN_SERVICE,
            "-a", KEYCHAIN_ACCOUNT,
        ])
        .output();

    // Add new entry
    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-s", KEYCHAIN_SERVICE,
            "-a", KEYCHAIN_ACCOUNT,
            "-w", passphrase,
            "-U", // Update if exists
        ])
        .output()
        .context("Failed to run security command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to store passphrase in keychain: {}", stderr);
    }

    Ok(())
}

/// Retrieves passphrase from macOS Keychain using security command
pub fn get_passphrase() -> Result<String> {
    let output = Command::new("security")
        .args([
            "find-generic-password",
            "-s", KEYCHAIN_SERVICE,
            "-a", KEYCHAIN_ACCOUNT,
            "-w", // Output password only
        ])
        .output()
        .context("Failed to run security command")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to retrieve passphrase from keychain.\n\
             Run 'whatsapp-backup init' to set up encryption."
        );
    }

    let passphrase = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(passphrase)
}

/// Checks if passphrase exists in keychain
pub fn has_passphrase() -> bool {
    get_passphrase().is_ok()
}

/// Deletes passphrase from keychain
pub fn delete_passphrase() -> Result<()> {
    let output = Command::new("security")
        .args([
            "delete-generic-password",
            "-s", KEYCHAIN_SERVICE,
            "-a", KEYCHAIN_ACCOUNT,
        ])
        .output()
        .context("Failed to run security command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to delete passphrase: {}", stderr);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let data = b"Hello, WhatsApp backup!";
        let passphrase = "test-passphrase-123";

        let encrypted = encrypt(data, passphrase).unwrap();
        let decrypted = decrypt(&encrypted, passphrase).unwrap();

        assert_eq!(data.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_wrong_passphrase_fails() {
        let data = b"Secret data";
        let encrypted = encrypt(data, "correct-password").unwrap();
        let result = decrypt(&encrypted, "wrong-password");

        assert!(result.is_err());
    }
}
