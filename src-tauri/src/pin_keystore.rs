use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use human_money_core::services::crypto;
use human_money_core::SecondaryScope;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const MAX_PIN_ATTEMPTS: u8 = 5;
const PIN_KEYSTORE_FILE: &str = "pin_keystore.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinKeystoreEntry {
    pub encrypted_pepper_b64: String,
    pub failed_attempts: u8,
    pub scope: SecondaryScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PinKeystoreFile {
    entries: HashMap<String, PinKeystoreEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePinStatusDto {
    pub enabled: bool,
    pub scope: Option<String>,
    pub failed_attempts: u8,
    pub max_attempts: u8,
    pub remaining_attempts: u8,
}

pub fn scope_to_str(scope: SecondaryScope) -> &'static str {
    match scope {
        SecondaryScope::DailyPay => "dailyPay",
        SecondaryScope::ReceiveOnly => "receiveOnly",
    }
}

pub fn parse_scope(raw: &str) -> Result<SecondaryScope, String> {
    match raw {
        "dailyPay" => Ok(SecondaryScope::DailyPay),
        other => Err(format!("Unsupported PIN scope: {}", other)),
    }
}

/// Validates that the user-supplied PIN consists strictly of 6 to 8 ASCII digits.
pub fn validate_pin_format(pin: &str) -> Result<(), String> {
    if pin.len() < 6 || pin.len() > 8 || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err("PIN must consist of 6 to 8 digits.".to_string());
    }
    Ok(())
}

/// Derives a device-local high-entropy secondary secret from the 32-byte `device_pepper`,
/// profile `folder_name`, and `pin`.
pub fn derive_pin_secret(pepper: &[u8; 32], folder_name: &str, pin: &str) -> Zeroizing<String> {
    let mut hasher = blake3::Hasher::new_keyed(pepper);
    hasher.update(b"HUMOCO_PIN_SECRET_V1|");
    hasher.update(folder_name.as_bytes());
    hasher.update(b"|");
    hasher.update(pin.as_bytes());
    Zeroizing::new(hasher.finalize().to_hex().to_string())
}

fn derive_keystore_wrap_key(device_binding_id: &str) -> Zeroizing<[u8; 32]> {
    Zeroizing::new(blake3::derive_key(
        "HUMOCO_PIN_KEYSTORE_WRAP_V1",
        device_binding_id.as_bytes(),
    ))
}

fn keystore_path(keystore_dir: &Path) -> PathBuf {
    keystore_dir.join(PIN_KEYSTORE_FILE)
}

fn load_keystore_file(keystore_dir: &Path) -> Result<PinKeystoreFile, String> {
    let path = keystore_path(keystore_dir);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PinKeystoreFile::default())
        }
        Err(e) => return Err(format!("Failed to read PIN keystore: {}", e)),
    };
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid PIN keystore JSON: {}", e))
}

fn save_keystore_file(keystore_dir: &Path, store: &PinKeystoreFile) -> Result<(), String> {
    fs::create_dir_all(keystore_dir)
        .map_err(|e| format!("Failed to create PIN keystore directory: {}", e))?;
    let path = keystore_path(keystore_dir);
    let tmp = keystore_dir.join(format!(
        "{}.tmp.{}",
        PIN_KEYSTORE_FILE,
        uuid::Uuid::new_v4()
    ));
    let bytes = serde_json::to_vec_pretty(store)
        .map_err(|e| format!("Failed to serialize PIN keystore: {}", e))?;
    fs::write(&tmp, &bytes).map_err(|e| format!("Failed to write temp PIN keystore: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, &path).map_err(|e| format!("Failed to commit PIN keystore: {}", e))?;
    Ok(())
}

/// Generates a fresh 32-byte random `device_pepper`, persists it encrypted in `pin_keystore.json`,
/// and returns the derived high-entropy secret for `AppService::enable_secondary_auth`.
pub fn create_and_store_pepper(
    keystore_dir: &Path,
    device_binding_id: &str,
    folder_name: &str,
    pin: &str,
    scope: SecondaryScope,
) -> Result<Zeroizing<String>, String> {
    validate_pin_format(pin)?;
    if device_binding_id.trim().is_empty() {
        return Err("Missing device binding identifier for PIN keystore.".to_string());
    }

    let mut pepper = Zeroizing::new([0u8; 32]);
    OsRng.fill_bytes(&mut *pepper);

    let wrap_key = derive_keystore_wrap_key(device_binding_id);
    let encrypted = crypto::encrypt_data(&wrap_key, pepper.as_ref())
        .map_err(|e| format!("Failed to encrypt device pepper: {}", e))?;

    let mut store = load_keystore_file(keystore_dir)?;
    store.entries.insert(
        folder_name.to_string(),
        PinKeystoreEntry {
            encrypted_pepper_b64: B64.encode(encrypted),
            failed_attempts: 0,
            scope,
        },
    );
    save_keystore_file(keystore_dir, &store)?;

    Ok(derive_pin_secret(&pepper, folder_name, pin))
}

pub fn get_entry(
    keystore_dir: &Path,
    folder_name: &str,
) -> Result<Option<PinKeystoreEntry>, String> {
    let store = load_keystore_file(keystore_dir)?;
    Ok(store.entries.get(folder_name).cloned())
}

pub fn remove_entry(keystore_dir: &Path, folder_name: &str) -> Result<(), String> {
    let mut store = load_keystore_file(keystore_dir)?;
    if store.entries.remove(folder_name).is_some() {
        save_keystore_file(keystore_dir, &store)?;
    }
    Ok(())
}

/// Increments `failed_attempts` BEFORE attempting PIN unlock (crash-safe strike counter) and
/// decrypts the `device_pepper` to derive the candidate secondary secret.
pub fn begin_pin_attempt(
    keystore_dir: &Path,
    device_binding_id: &str,
    folder_name: &str,
    pin: &str,
) -> Result<(Zeroizing<String>, u8), String> {
    validate_pin_format(pin)?;
    let mut store = load_keystore_file(keystore_dir)?;
    let entry = store
        .entries
        .get_mut(folder_name)
        .ok_or_else(|| "PIN is not enabled for this profile on this device.".to_string())?;

    entry.failed_attempts = entry.failed_attempts.saturating_add(1);
    let current_attempt = entry.failed_attempts;
    let enc_b64 = entry.encrypted_pepper_b64.clone();
    save_keystore_file(keystore_dir, &store)?;

    let encrypted = B64
        .decode(&enc_b64)
        .map_err(|e| format!("Corrupt PIN keystore entry: {}", e))?;
    let wrap_key = derive_keystore_wrap_key(device_binding_id);
    let plain =
        Zeroizing::new(crypto::decrypt_data(&wrap_key, &encrypted).map_err(|_| {
            "Failed to unwrap device pepper (device identity changed).".to_string()
        })?);
    let pepper: [u8; 32] = plain
        .as_slice()
        .try_into()
        .map_err(|_| "Invalid unwrapped pepper length.".to_string())?;

    Ok((
        derive_pin_secret(&pepper, folder_name, pin),
        current_attempt,
    ))
}

pub fn reset_failed_attempts(keystore_dir: &Path, folder_name: &str) -> Result<(), String> {
    let mut store = load_keystore_file(keystore_dir)?;
    if let Some(entry) = store.entries.get_mut(folder_name) {
        entry.failed_attempts = 0;
        save_keystore_file(keystore_dir, &store)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_keystore_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "humoco_pin_keystore_test_{}_{}",
            label,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_validate_pin_format_and_scope() {
        assert!(validate_pin_format("12345").is_err());
        assert!(validate_pin_format("123456789").is_err());
        assert!(validate_pin_format("12ab56").is_err());
        assert!(validate_pin_format("123456").is_ok());
        assert!(validate_pin_format("12345678").is_ok());

        assert_eq!(parse_scope("dailyPay").unwrap(), SecondaryScope::DailyPay);
        assert!(parse_scope("receiveOnly").is_err());
        assert!(parse_scope("daily_pay").is_err());
        assert!(parse_scope("receive_only").is_err());
        assert!(parse_scope("full").is_err());
    }

    #[test]
    fn test_pepper_creation_strike_progression_reset_and_remove() {
        let dir = temp_keystore_dir("lifecycle");
        let dev = "device-binding-1";
        let folder = "profile_a";
        let pin = "246810";

        let secret =
            create_and_store_pepper(&dir, dev, folder, pin, SecondaryScope::DailyPay).unwrap();
        assert_eq!(secret.len(), 64);

        let entry = get_entry(&dir, folder).unwrap().expect("entry must exist");
        assert_eq!(entry.failed_attempts, 0);
        assert_eq!(entry.scope, SecondaryScope::DailyPay);

        // Strike progression up to MAX_PIN_ATTEMPTS = 5
        for expected_attempt in 1..=MAX_PIN_ATTEMPTS {
            let (derived, attempt) = begin_pin_attempt(&dir, dev, folder, pin).unwrap();
            assert_eq!(attempt, expected_attempt);
            assert_eq!(derived.as_str(), secret.as_str());
        }

        // Device mismatch fails unwrapping even after recording strike
        let mismatch = begin_pin_attempt(&dir, "other-device", folder, pin);
        assert!(mismatch.is_err());

        // Lockdown / success recovery resets failed_attempts without wiping entry
        reset_failed_attempts(&dir, folder).unwrap();
        let after_reset = get_entry(&dir, folder).unwrap().unwrap();
        assert_eq!(after_reset.failed_attempts, 0);

        // Remove entry cleans keystore slot
        remove_entry(&dir, folder).unwrap();
        assert!(get_entry(&dir, folder).unwrap().is_none());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn test_corrupt_pin_keystore_refuses_overwrite() {
        let dir = temp_keystore_dir("corrupt");
        let path = keystore_path(&dir);
        let corrupt = b"{not-valid-json";
        fs::write(&path, corrupt).unwrap();

        let err = create_and_store_pepper(&dir, "dev-1", "profile_b", "246810", SecondaryScope::DailyPay)
            .unwrap_err();
        assert!(err.contains("Invalid PIN keystore JSON"));
        assert_eq!(fs::read(&path).unwrap(), corrupt);

        let _ = fs::remove_dir_all(dir);
    }
}
