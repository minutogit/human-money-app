use std::collections::BTreeMap;

use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey};
use hkdf::Hkdf;
use human_money_core::crypto::{
    create_user_id, get_prefix_from_user_id, get_pubkey_from_user_id, verify_ed25519,
};
use human_money_core::services::mnemonic::MnemonicLanguage;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization;
use url::form_urlencoded;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::error::MarketError;
use crate::model::DropDescriptor;

// ============================================================================
// Constants
// ============================================================================

pub const RECOVERY_QUESTION_COUNT: u8 = 12;
pub const RECOVERY_MIN_ANSWER_LEN: usize = 2;
pub const RECOVERY_MIN_TOTAL_LEN: usize = 12;

pub const KDF_M_KIB_PRODUCTION: u32 = 65_536; // 64 MiB
pub const KDF_T_COST_PRODUCTION: u32 = 12;
pub const KDF_P_COST_PRODUCTION: u32 = 1;

pub const KDF_M_KIB_MIN: u32 = 8;
pub const KDF_M_KIB_MAX: u32 = 262_144; // 256 MiB DoS ceiling
pub const KDF_T_COST_MIN: u32 = 1;
pub const KDF_T_COST_MAX: u32 = 32;
pub const KDF_P_COST_MIN: u32 = 1;
pub const KDF_P_COST_MAX: u32 = 4;

pub const PAYLOAD_GUARDIAN_ASSIGN_V1: &str = "guardian_assign_v1";
pub const PAYLOAD_GUARDIAN_ACK_V1: &str = "guardian_ack_v1";
pub const PAYLOAD_GUARDIAN_LOCATOR_V1: &str = "guardian_locator_v1";
pub const PAYLOAD_GUARDIAN_REVOKE_V1: &str = "guardian_revoke_v1";
pub const MAX_GUARDIAN_WARDS: usize = 64;

// ============================================================================
// Normalization & Answer Validation
// ============================================================================

/// Normalizes raw user input deterministically across mobile and desktop platforms.
///
/// Steps:
/// 1. Unicode NFC normalization.
/// 2. `to_lowercase()`.
/// 3. German umlaut / eszett folding (`ä -> ae`, `ö -> oe`, `ü -> ue`, `ß -> ss`).
/// 4. Replace `-` and `_` with `' '`, collapse consecutive whitespace runs to a single `' '`, and trim ends.
pub fn normalize_recovery_input(raw: &str) -> String {
    let nfc: String = raw.nfc().collect();
    let lower = nfc.to_lowercase();
    let mut folded = String::with_capacity(lower.len());
    for ch in lower.chars() {
        match ch {
            'ä' => folded.push_str("ae"),
            'ö' => folded.push_str("oe"),
            'ü' => folded.push_str("ue"),
            'ß' => folded.push_str("ss"),
            '-' | '_' => folded.push(' '),
            other => folded.push(other),
        }
    }
    let words: Vec<&str> = folded.split_whitespace().collect();
    words.join(" ")
}

/// Validates question IDs and user answers, returning the canonical KDF input string in `Zeroizing<String>`.
///
/// Requires:
/// - 3 pairwise distinct question IDs in `1..=RECOVERY_QUESTION_COUNT`.
/// - Each normalized answer `.chars().count() >= RECOVERY_MIN_ANSWER_LEN`.
/// - Sum of all 3 normalized answer lengths `.chars().count() >= RECOVERY_MIN_TOTAL_LEN`.
pub fn validate_and_canonicalize_answers(
    question_ids: &[u8; 3],
    answers: &[&str; 3],
) -> Result<Zeroizing<String>, MarketError> {
    for &qid in question_ids {
        if !(1..=RECOVERY_QUESTION_COUNT).contains(&qid) {
            return Err(MarketError::Validation(format!(
                "Invalid question ID {}: must be between 1 and {}",
                qid, RECOVERY_QUESTION_COUNT
            )));
        }
    }

    if question_ids[0] == question_ids[1]
        || question_ids[0] == question_ids[2]
        || question_ids[1] == question_ids[2]
    {
        return Err(MarketError::Validation(
            "Question IDs must be pairwise distinct".to_string(),
        ));
    }

    let norm_0 = normalize_recovery_input(answers[0]);
    let norm_1 = normalize_recovery_input(answers[1]);
    let norm_2 = normalize_recovery_input(answers[2]);

    let len_0 = norm_0.chars().count();
    let len_1 = norm_1.chars().count();
    let len_2 = norm_2.chars().count();

    if len_0 < RECOVERY_MIN_ANSWER_LEN
        || len_1 < RECOVERY_MIN_ANSWER_LEN
        || len_2 < RECOVERY_MIN_ANSWER_LEN
    {
        return Err(MarketError::Validation(format!(
            "Each answer must be at least {} characters after normalization",
            RECOVERY_MIN_ANSWER_LEN
        )));
    }

    let total_len = len_0 + len_1 + len_2;
    if total_len < RECOVERY_MIN_TOTAL_LEN {
        return Err(MarketError::Validation(format!(
            "Total answer length must be at least {} characters after normalization (got {})",
            RECOVERY_MIN_TOTAL_LEN, total_len
        )));
    }

    let canonical = format!(
        "HUMOCO-REC-KDF-V1|q3|{}:{}|{}:{}|{}:{}",
        question_ids[0], norm_0, question_ids[1], norm_1, question_ids[2], norm_2
    );

    Ok(Zeroizing::new(canonical))
}

// ============================================================================
// High-Cost Argon2id KDF
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryKdfParams {
    pub m_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt: [u8; 16],
}

impl RecoveryKdfParams {
    /// Creates production Argon2id parameters (64 MiB RAM, 12 iterations, 1 lane, fresh 16-byte salt).
    pub fn new_production() -> Self {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        Self {
            m_kib: KDF_M_KIB_PRODUCTION,
            t_cost: KDF_T_COST_PRODUCTION,
            p_cost: KDF_P_COST_PRODUCTION,
            salt,
        }
    }

    /// Creates reduced parameters for fast testing.
    pub fn new_fast_test() -> Self {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        Self {
            m_kib: 64,
            t_cost: 1,
            p_cost: 1,
            salt,
        }
    }
}

/// Enforces hard bounds on Argon2id parameters to prevent OOM and CPU exhaustion DoS attacks.
pub fn validate_kdf_params(params: &RecoveryKdfParams) -> Result<(), MarketError> {
    if params.m_kib < KDF_M_KIB_MIN || params.m_kib > KDF_M_KIB_MAX {
        return Err(MarketError::Validation(format!(
            "m_kib out of bounds: {} (expected {}..={})",
            params.m_kib, KDF_M_KIB_MIN, KDF_M_KIB_MAX
        )));
    }
    if params.t_cost < KDF_T_COST_MIN || params.t_cost > KDF_T_COST_MAX {
        return Err(MarketError::Validation(format!(
            "t_cost out of bounds: {} (expected {}..={})",
            params.t_cost, KDF_T_COST_MIN, KDF_T_COST_MAX
        )));
    }
    if params.p_cost < KDF_P_COST_MIN || params.p_cost > KDF_P_COST_MAX {
        return Err(MarketError::Validation(format!(
            "p_cost out of bounds: {} (expected {}..={})",
            params.p_cost, KDF_P_COST_MIN, KDF_P_COST_MAX
        )));
    }
    Ok(())
}

/// Derives a 32-byte key `P` from 3 question answers and salt using Argon2id.
pub fn derive_recovery_key(
    question_ids: &[u8; 3],
    answers: &[&str; 3],
    params: &RecoveryKdfParams,
) -> Result<Zeroizing<[u8; 32]>, MarketError> {
    validate_kdf_params(params)?;
    let canonical = validate_and_canonicalize_answers(question_ids, answers)?;

    let argon_params = Params::new(params.m_kib, params.t_cost, params.p_cost, Some(32))
        .map_err(|e| MarketError::Crypto(format!("Invalid Argon2 parameters: {}", e)))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(canonical.as_bytes(), &params.salt, &mut key)
        .map_err(|e| MarketError::Crypto(format!("Argon2id derivation failed: {}", e)))?;

    Ok(Zeroizing::new(key))
}

fn derive_single_answer_verifier(
    owner_did: &str,
    rotation_epoch: u32,
    question_id: u8,
    normalized_answer: &str,
    kdf_params: &RecoveryKdfParams,
) -> Result<[u8; 32], MarketError> {
    use sha2::Digest;

    if !(1..=RECOVERY_QUESTION_COUNT).contains(&question_id) {
        return Err(MarketError::Validation(format!(
            "Invalid question ID {}: must be between 1 and {}",
            question_id, RECOVERY_QUESTION_COUNT
        )));
    }
    validate_kdf_params(kdf_params)?;
    let m_kib = (kdf_params.m_kib / 4).max(KDF_M_KIB_MIN);
    let t_cost = (kdf_params.t_cost / 4).max(KDF_T_COST_MIN);
    let p_cost = kdf_params.p_cost;

    let mut salt_hash = Sha256::new();
    salt_hash.update(b"HUMOCO-REC-REHEARSAL-SALT-V1");
    salt_hash.update(owner_did.as_bytes());
    salt_hash.update(rotation_epoch.to_le_bytes());
    salt_hash.update([question_id]);
    let digest = salt_hash.finalize();
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&digest[..16]);

    let input = Zeroizing::new(format!(
        "HUMOCO-REC-REHEARSAL-V1|{}|{}|{}:{}",
        owner_did, rotation_epoch, question_id, normalized_answer
    ));
    let argon_params = Params::new(m_kib, t_cost, p_cost, Some(32))
        .map_err(|e| MarketError::Crypto(format!("Invalid Argon2 parameters: {}", e)))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut tag = [0u8; 32];
    argon2
        .hash_password_into(input.as_bytes(), &salt, &mut tag)
        .map_err(|e| MarketError::Crypto(format!("Argon2id derivation failed: {}", e)))?;
    Ok(tag)
}

/// Computes memory-hard one-way per-answer rehearsal verifiers (no plaintext persisted on disk).
pub fn compute_rehearsal_verifiers(
    owner_did: &str,
    rotation_epoch: u32,
    question_ids: &[u8; 3],
    answers: &[&str; 3],
    kdf_params: &RecoveryKdfParams,
) -> Result<[[u8; 32]; 3], MarketError> {
    let _canonical = validate_and_canonicalize_answers(question_ids, answers)?;
    let mut out = [[0u8; 32]; 3];
    for i in 0..3 {
        let norm = Zeroizing::new(normalize_recovery_input(answers[i]));
        out[i] = derive_single_answer_verifier(
            owner_did,
            rotation_epoch,
            question_ids[i],
            &norm,
            kdf_params,
        )?;
    }
    Ok(out)
}

/// Verifies rehearsal answers individually against stored memory-hard verifiers, returning per-question match flags.
pub fn verify_rehearsal_answers(
    owner_did: &str,
    rotation_epoch: u32,
    question_ids: &[u8; 3],
    answers: &[&str; 3],
    expected_verifiers: &[[u8; 32]; 3],
    kdf_params: &RecoveryKdfParams,
) -> Result<[bool; 3], MarketError> {
    let mut matches = [false; 3];
    for i in 0..3 {
        let norm = Zeroizing::new(normalize_recovery_input(answers[i]));
        if norm.chars().count() < RECOVERY_MIN_ANSWER_LEN {
            matches[i] = false;
            continue;
        }
        let actual = derive_single_answer_verifier(
            owner_did,
            rotation_epoch,
            question_ids[i],
            &norm,
            kdf_params,
        )?;
        let diff = (0..32).fold(0u8, |acc, idx| {
            acc | (actual[idx] ^ expected_verifiers[i][idx])
        });
        matches[i] = diff == 0;
    }
    Ok(matches)
}

// ============================================================================
// Constant-Time / Table-Free GF(256) Shamir Secret Sharing
// ============================================================================

/// Constant-time multiplication in GF(2^8) modulo `0x11b` (irreducible polynomial x^8 + x^4 + x^3 + x + 1).
#[inline]
pub fn gf256_mul(mut a: u8, mut b: u8) -> u8 {
    let mut p: u8 = 0;
    for _ in 0..8 {
        let b_bit = b & 1;
        let mask = 0u8.wrapping_sub(b_bit);
        p ^= a & mask;

        let hi_bit = (a >> 7) & 1;
        let hi_mask = 0u8.wrapping_sub(hi_bit);
        a = (a << 1) ^ (0x1b & hi_mask);

        b >>= 1;
    }
    p
}

/// Multiplicative inverse in GF(2^8) via Fermat's Little Theorem: a^-1 = a^254 mod 0x11b.
pub fn gf256_inv(a: u8) -> Result<u8, MarketError> {
    if a == 0 {
        return Err(MarketError::Crypto(
            "Cannot invert zero in GF(256)".to_string(),
        ));
    }
    // Fixed addition chain for 254 = 11111110_2
    let a2 = gf256_mul(a, a); // a^2
    let a3 = gf256_mul(a2, a); // a^3
    let a6 = gf256_mul(a3, a3); // a^6
    let a7 = gf256_mul(a6, a); // a^7
    let a14 = gf256_mul(a7, a7); // a^14
    let a15 = gf256_mul(a14, a); // a^15
    let a30 = gf256_mul(a15, a15); // a^30
    let a60 = gf256_mul(a30, a30); // a^60
    let a120 = gf256_mul(a60, a60); // a^120
    let a126 = gf256_mul(a120, a6); // a^126
    let a127 = gf256_mul(a126, a); // a^127
    let a254 = gf256_mul(a127, a127); // a^254
    Ok(a254)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShamirShare {
    pub index: u8,
    pub y: [u8; 32],
}

/// Splits a 32-byte secret into `total_n` Shamir shares with threshold `threshold_m` in GF(256).
///
/// Requires `2 <= threshold_m <= total_n <= 16`.
pub fn split_secret_32(
    secret: &[u8; 32],
    threshold_m: u8,
    total_n: u8,
) -> Result<Vec<ShamirShare>, MarketError> {
    if threshold_m < 2 || threshold_m > total_n || total_n > 16 {
        return Err(MarketError::Validation(format!(
            "Invalid Shamir threshold parameters: m={}, n={} (require 2 <= m <= n <= 16)",
            threshold_m, total_n
        )));
    }

    let m = threshold_m as usize;

    let mut shares: Vec<ShamirShare> = (1..=total_n)
        .map(|idx| ShamirShare {
            index: idx,
            y: [0u8; 32],
        })
        .collect();

    // For each byte position in 0..32, construct an independent random polynomial of degree m-1
    for (byte_idx, &sec_byte) in secret.iter().enumerate() {
        let mut coeffs: Zeroizing<Vec<u8>> = Zeroizing::new(vec![0u8; m]);
        coeffs[0] = sec_byte;
        OsRng.fill_bytes(&mut coeffs[1..m]);

        // Evaluate at x in 1..=n using Horner's method
        for (i, share) in shares.iter_mut().enumerate() {
            let x = (i + 1) as u8;
            let mut val = coeffs[m - 1];
            for k in (0..m - 1).rev() {
                val = gf256_mul(val, x) ^ coeffs[k];
            }
            share.y[byte_idx] = val;
        }
    }

    Ok(shares)
}

/// Reconstructs the 32-byte secret from at least `threshold_m` shares via Lagrange interpolation at x = 0.
///
/// Rejects duplicate indices, index == 0, or shares.len() < threshold_m.
pub fn combine_shares_32(
    shares: &[ShamirShare],
    threshold_m: u8,
) -> Result<Zeroizing<[u8; 32]>, MarketError> {
    if threshold_m < 2 {
        return Err(MarketError::Validation(
            "Threshold m must be at least 2".to_string(),
        ));
    }
    if shares.len() < threshold_m as usize {
        return Err(MarketError::Validation(format!(
            "Insufficient shares: got {}, required {}",
            shares.len(),
            threshold_m
        )));
    }

    // Check for index == 0 and reject ANY duplicate share indices across the entire slice
    let mut seen = [false; 256];
    for share in shares {
        if share.index == 0 {
            return Err(MarketError::Validation(
                "Shamir share index 0 is invalid".to_string(),
            ));
        }
        if seen[share.index as usize] {
            return Err(MarketError::Validation(format!(
                "Duplicate Shamir share index: {}",
                share.index
            )));
        }
        seen[share.index as usize] = true;
    }

    let m = threshold_m as usize;
    let selected = &shares[..m];

    // Compute Lagrange basis coefficients lambda_j(0) = \prod_{k != j} (x_k * (x_j ^ x_k)^-1)
    let mut lambdas = Vec::with_capacity(m);
    for (j, share_j) in selected.iter().enumerate() {
        let xj = share_j.index;
        let mut lambda = 1u8;
        for (k, share_k) in selected.iter().enumerate() {
            if j == k {
                continue;
            }
            let xk = share_k.index;
            let diff = xj ^ xk;
            let inv_diff = gf256_inv(diff)?;
            let factor = gf256_mul(xk, inv_diff);
            lambda = gf256_mul(lambda, factor);
        }
        lambdas.push(lambda);
    }

    let mut secret = [0u8; 32];
    for (byte_idx, sec_byte) in secret.iter_mut().enumerate() {
        let mut acc = 0u8;
        for (j, share) in selected.iter().enumerate() {
            let term = gf256_mul(share.y[byte_idx], lambdas[j]);
            acc ^= term;
        }
        *sec_byte = acc;
    }

    Ok(Zeroizing::new(secret))
}

// ============================================================================
// Mnemonic Language Mapping
// ============================================================================

pub fn mnemonic_language_to_code(lang: MnemonicLanguage) -> u8 {
    match lang {
        MnemonicLanguage::English => 1,
        MnemonicLanguage::German => 2,
        MnemonicLanguage::French => 3,
        MnemonicLanguage::Spanish => 4,
        MnemonicLanguage::Italian => 5,
        MnemonicLanguage::Japanese => 6,
        MnemonicLanguage::Korean => 7,
        MnemonicLanguage::ChineseSimplified => 8,
        MnemonicLanguage::ChineseTraditional => 9,
        MnemonicLanguage::Portuguese => 10,
        MnemonicLanguage::Czech => 11,
    }
}

pub fn code_to_mnemonic_language(code: u8) -> Result<MnemonicLanguage, MarketError> {
    match code {
        1 => Ok(MnemonicLanguage::English),
        2 => Ok(MnemonicLanguage::German),
        3 => Ok(MnemonicLanguage::French),
        4 => Ok(MnemonicLanguage::Spanish),
        5 => Ok(MnemonicLanguage::Italian),
        6 => Ok(MnemonicLanguage::Japanese),
        7 => Ok(MnemonicLanguage::Korean),
        8 => Ok(MnemonicLanguage::ChineseSimplified),
        9 => Ok(MnemonicLanguage::ChineseTraditional),
        10 => Ok(MnemonicLanguage::Portuguese),
        11 => Ok(MnemonicLanguage::Czech),
        _ => Err(MarketError::Validation(format!(
            "Unknown mnemonic language code: {}",
            code
        ))),
    }
}

// ============================================================================
// Master Payload & Sealing
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct RecoveryMasterPayload {
    #[zeroize(skip)]
    pub version: u8,
    #[zeroize(skip)]
    pub target_did: String,
    pub mnemonic_entropy: Vec<u8>,
    #[zeroize(skip)]
    pub language_code: u8,
    #[zeroize(skip)]
    pub user_prefix: Option<String>,
    #[zeroize(skip)]
    pub has_bip39_passphrase: bool,
}

impl RecoveryMasterPayload {
    pub fn new(
        target_did: impl Into<String>,
        mnemonic_entropy: Vec<u8>,
        language: MnemonicLanguage,
        user_prefix: Option<String>,
        has_bip39_passphrase: bool,
    ) -> Self {
        Self {
            version: 1,
            target_did: target_did.into(),
            mnemonic_entropy,
            language_code: mnemonic_language_to_code(language),
            user_prefix,
            has_bip39_passphrase,
        }
    }

    pub fn language(&self) -> Result<MnemonicLanguage, MarketError> {
        code_to_mnemonic_language(self.language_code)
    }
}

/// Encrypts a `RecoveryMasterPayload` with key `S` via `XChaCha20Poly1305` (AAD `b"HUMOCO-REC-MASTER-V1"`).
///
/// Output format: `nonce (24 bytes) || ciphertext (includes 16-byte tag)`.
pub fn seal_recovery_master_payload(
    payload: &RecoveryMasterPayload,
    s: &[u8; 32],
) -> Result<Vec<u8>, MarketError> {
    let cipher =
        XChaCha20Poly1305::new_from_slice(s).map_err(|e| MarketError::Crypto(e.to_string()))?;

    let mut nonce_bytes = [0u8; 24];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    let plaintext = Zeroizing::new(
        bincode::serialize(payload).map_err(|e| MarketError::Serialization(e.to_string()))?,
    );

    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: &plaintext,
                aad: b"HUMOCO-REC-MASTER-V1",
            },
        )
        .map_err(|e| MarketError::Crypto(format!("AEAD master sealing failed: {}", e)))?;

    let mut r_enc = Vec::with_capacity(24 + ciphertext.len());
    r_enc.extend_from_slice(&nonce_bytes);
    r_enc.extend_from_slice(&ciphertext);
    Ok(r_enc)
}

/// Decrypts `r_enc` with key `S` via `XChaCha20Poly1305` (AAD `b"HUMOCO-REC-MASTER-V1"`).
pub fn unseal_recovery_master_payload(
    r_enc: &[u8],
    s: &[u8; 32],
) -> Result<RecoveryMasterPayload, MarketError> {
    if r_enc.len() < 24 + 16 {
        return Err(MarketError::Crypto("r_enc payload too short".to_string()));
    }

    let nonce = XNonce::from_slice(&r_enc[0..24]);
    let ciphertext = &r_enc[24..];

    let cipher =
        XChaCha20Poly1305::new_from_slice(s).map_err(|e| MarketError::Crypto(e.to_string()))?;

    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad: b"HUMOCO-REC-MASTER-V1",
                },
            )
            .map_err(|e| MarketError::Crypto(format!("AEAD master unsealing failed: {}", e)))?,
    );

    let payload: RecoveryMasterPayload = bincode::deserialize(&plaintext)
        .map_err(|e| MarketError::Serialization(format!("AEAD master unsealing failed: {}", e)))?;
    Ok(payload)
}

// ============================================================================
// Share Commitments & Header Signatures
// ============================================================================

/// Computes the 16-byte share commitment:
/// `BLAKE3("HUMOCO-SHARE-COMMIT-V1" || len(owner_did) || owner_did || epoch || index || y)[0..16]`
pub fn compute_share_commitment(
    owner_did: &str,
    rotation_epoch: u32,
    share: &ShamirShare,
) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"HUMOCO-SHARE-COMMIT-V1");
    hasher.update(&(owner_did.len() as u32).to_be_bytes());
    hasher.update(owner_did.as_bytes());
    hasher.update(&rotation_epoch.to_be_bytes());
    hasher.update(&[share.index]);
    hasher.update(&share.y);
    let hash = hasher.finalize();
    let mut commit = [0u8; 16];
    commit.copy_from_slice(&hash.as_bytes()[0..16]);
    commit
}

/// Canonical header signing bytes covering:
/// `b"HUMOCO-REC-ENTRY-SIG-V1" || len(did) || did || epoch || m || n || kdf_params || q_ids || blake3(r_enc) || commitments`
#[allow(clippy::too_many_arguments)]
pub fn guardian_entry_signing_bytes(
    owner_did: &str,
    rotation_epoch: u32,
    threshold_m: u8,
    total_n: u8,
    kdf_params: &RecoveryKdfParams,
    question_ids: &[u8; 3],
    r_enc: &[u8],
    share_commitments: &[[u8; 16]],
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"HUMOCO-REC-ENTRY-SIG-V1");
    buf.extend_from_slice(&(owner_did.len() as u32).to_be_bytes());
    buf.extend_from_slice(owner_did.as_bytes());
    buf.extend_from_slice(&rotation_epoch.to_be_bytes());
    buf.push(threshold_m);
    buf.push(total_n);
    buf.extend_from_slice(&kdf_params.m_kib.to_be_bytes());
    buf.extend_from_slice(&kdf_params.t_cost.to_be_bytes());
    buf.extend_from_slice(&kdf_params.p_cost.to_be_bytes());
    buf.extend_from_slice(&kdf_params.salt);
    buf.extend_from_slice(question_ids);
    let r_enc_hash = blake3::hash(r_enc);
    buf.extend_from_slice(r_enc_hash.as_bytes());
    buf.extend_from_slice(&(share_commitments.len() as u32).to_be_bytes());
    for commit in share_commitments {
        buf.extend_from_slice(commit);
    }
    buf
}

pub mod serde_sig64 {
    use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(sig: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            hex::encode(sig).serialize(serializer)
        } else {
            sig.as_slice().serialize(serializer)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let bytes = hex::decode(s.trim()).map_err(D::Error::custom)?;
            bytes
                .try_into()
                .map_err(|_| D::Error::custom("owner_sig must be exactly 64 bytes"))
        } else {
            let bytes = Vec::<u8>::deserialize(deserializer)?;
            bytes
                .try_into()
                .map_err(|_| D::Error::custom("owner_sig must be exactly 64 bytes"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianVaultEntry {
    pub owner_did: String,
    pub owner_display_name: String,
    pub rotation_epoch: u32,
    pub threshold_m: u8,
    pub total_n: u8,
    pub share: ShamirShare,
    pub share_commitments: Vec<[u8; 16]>,
    pub r_enc: Vec<u8>,
    pub kdf_params: RecoveryKdfParams,
    pub question_ids: [u8; 3],
    #[serde(with = "serde_sig64")]
    pub owner_sig: [u8; 64],
    pub backup_sync_descriptors: Vec<DropDescriptor>,
    pub updated_at: u64,
}

/// Validates and canonicalizes an account owner ID while preserving an optional subaccount prefix.
pub fn canonicalize_owner_account_id(owner_did: &str) -> Result<String, MarketError> {
    let trimmed = owner_did.trim();
    let pk = get_pubkey_from_user_id(trimmed)
        .map_err(|e| MarketError::Identity(format!("Invalid owner DID: {}", e)))?;
    let prefix = get_prefix_from_user_id(trimmed);
    let canonical = create_user_id(&pk, prefix)
        .map_err(|e| MarketError::Identity(format!("Failed to canonicalize owner DID: {}", e)))?;
    if trimmed != canonical {
        return Err(MarketError::Identity(format!(
            "owner_did must be canonical (expected {}, got {})",
            canonical, trimmed
        )));
    }
    Ok(canonical)
}

/// Verifies all cryptographic and structural invariants of a `GuardianVaultEntry`:
/// 1. Verifying key extraction & canonical account ID verification (root or prefixed subaccount).
/// 2. Threshold bounds (`2 <= threshold_m <= total_n <= 16`), distinct `question_ids` in `1..=12`, `share.index` in `1..=total_n`.
/// 3. `kdf_params` bounds check against DoS/OOM bombs.
/// 4. Ed25519 `owner_sig` verification.
/// 5. Share commitment check against `entry.share_commitments[share.index - 1]`.
pub fn verify_guardian_vault_entry(entry: &GuardianVaultEntry) -> Result<(), MarketError> {
    let canonical_owner = canonicalize_owner_account_id(&entry.owner_did)?;
    let pk = get_pubkey_from_user_id(&canonical_owner)
        .map_err(|e| MarketError::Identity(format!("Invalid owner DID: {}", e)))?;

    if entry.threshold_m < 2 || entry.threshold_m > entry.total_n || entry.total_n > 16 {
        return Err(MarketError::Validation(format!(
            "Invalid threshold bounds: m={}, n={}",
            entry.threshold_m, entry.total_n
        )));
    }

    for &qid in &entry.question_ids {
        if !(1..=RECOVERY_QUESTION_COUNT).contains(&qid) {
            return Err(MarketError::Validation(format!(
                "Invalid question ID {} in entry",
                qid
            )));
        }
    }
    if entry.question_ids[0] == entry.question_ids[1]
        || entry.question_ids[0] == entry.question_ids[2]
        || entry.question_ids[1] == entry.question_ids[2]
    {
        return Err(MarketError::Validation(
            "question_ids must be pairwise distinct".to_string(),
        ));
    }

    if entry.share.index < 1 || entry.share.index > entry.total_n {
        return Err(MarketError::Validation(format!(
            "Share index {} out of range 1..={}",
            entry.share.index, entry.total_n
        )));
    }

    if entry.share_commitments.len() != entry.total_n as usize {
        return Err(MarketError::Validation(format!(
            "share_commitments count {} != total_n {}",
            entry.share_commitments.len(),
            entry.total_n
        )));
    }

    validate_kdf_params(&entry.kdf_params)?;

    let signing_bytes = guardian_entry_signing_bytes(
        &entry.owner_did,
        entry.rotation_epoch,
        entry.threshold_m,
        entry.total_n,
        &entry.kdf_params,
        &entry.question_ids,
        &entry.r_enc,
        &entry.share_commitments,
    );

    let signature = Signature::from_bytes(&entry.owner_sig);
    if !verify_ed25519(&pk, &signing_bytes, &signature) {
        return Err(MarketError::Crypto(
            "Invalid owner signature on guardian vault entry".to_string(),
        ));
    }

    let expected_commit =
        compute_share_commitment(&entry.owner_did, entry.rotation_epoch, &entry.share);
    let recorded_commit = entry.share_commitments[(entry.share.index - 1) as usize];
    if expected_commit != recorded_commit {
        return Err(MarketError::Validation(format!(
            "Share commitment mismatch for index {}: expected {:?}, got {:?}",
            entry.share.index, expected_commit, recorded_commit
        )));
    }

    Ok(())
}

// ============================================================================
// Post Wire Payloads & GuardianVaultStore
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianVaultAck {
    pub owner_did: String,
    pub rotation_epoch: u32,
    pub acked_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianLocatorUpdate {
    pub owner_did: String,
    pub rotation_epoch: u32,
    pub backup_sync_descriptors: Vec<DropDescriptor>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianRevoke {
    pub owner_did: String,
    pub rotation_epoch: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianVaultStore {
    pub wards: BTreeMap<String, GuardianVaultEntry>,
    /// Monotonic high-water mark of seen `rotation_epoch` per canonical owner DID.
    pub highest_epoch_by_owner: BTreeMap<String, u32>,
    /// Highest revoked `rotation_epoch` per canonical owner DID to block assign replays after revoke
    /// or out-of-order assigns arriving after an earlier revoke.
    pub revoked_epoch_by_owner: BTreeMap<String, u32>,
}

impl GuardianVaultStore {
    pub fn new() -> Self {
        Self {
            wards: BTreeMap::new(),
            highest_epoch_by_owner: BTreeMap::new(),
            revoked_epoch_by_owner: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.wards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.wards.is_empty()
    }

    fn canonical_owner_key(owner_did: &str) -> String {
        let trimmed = owner_did.trim();
        canonicalize_owner_account_id(trimmed).unwrap_or_else(|_| trimmed.to_string())
    }

    pub fn get_ward(&self, owner_did: &str) -> Option<&GuardianVaultEntry> {
        let key = Self::canonical_owner_key(owner_did);
        self.wards.get(&key).or_else(|| self.wards.get(owner_did))
    }

    /// Stores or updates a ward's recovery entry.
    ///
    /// Invariants:
    /// - `verify_guardian_vault_entry(&entry)` must pass.
    /// - Monotonic epoch watermarks: rejects any epoch `<= revoked_epoch_by_owner` or `< highest_epoch_by_owner`.
    /// - Max `MAX_GUARDIAN_WARDS` (64) entries total.
    /// - 1 entry per `owner_did`.
    /// - If existing entry exists, `entry.rotation_epoch >= old.rotation_epoch` (preserving idempotent active re-upsert).
    pub fn upsert_ward(&mut self, entry: GuardianVaultEntry) -> Result<(), MarketError> {
        verify_guardian_vault_entry(&entry)?;
        let key = Self::canonical_owner_key(&entry.owner_did);

        if let Some(&rev_epoch) = self.revoked_epoch_by_owner.get(&key) {
            if entry.rotation_epoch <= rev_epoch {
                return Err(MarketError::Validation(format!(
                    "Rejected revoked or stale rotation epoch {} <= revoked {}",
                    entry.rotation_epoch, rev_epoch
                )));
            }
        }
        if let Some(&hi_epoch) = self.highest_epoch_by_owner.get(&key) {
            if entry.rotation_epoch < hi_epoch {
                return Err(MarketError::Validation(format!(
                    "Stale rotation epoch {} < highest seen {}",
                    entry.rotation_epoch, hi_epoch
                )));
            }
        }

        if let Some(existing) = self.wards.get(&key) {
            if entry.rotation_epoch < existing.rotation_epoch {
                return Err(MarketError::Validation(format!(
                    "Stale rotation epoch {} < existing {}",
                    entry.rotation_epoch, existing.rotation_epoch
                )));
            }
        } else if self.wards.len() >= MAX_GUARDIAN_WARDS {
            return Err(MarketError::Validation(format!(
                "Guardian vault capacity reached (maximum {} wards)",
                MAX_GUARDIAN_WARDS
            )));
        }

        let epoch = entry.rotation_epoch;
        self.wards.insert(key.clone(), entry);
        let hi = self.highest_epoch_by_owner.entry(key).or_insert(0);
        *hi = (*hi).max(epoch);
        Ok(())
    }

    /// Updates the backup sync descriptors for an existing ward if `update.rotation_epoch` is monotonic
    /// and not revoked.
    pub fn update_locator(&mut self, update: &GuardianLocatorUpdate) -> Result<bool, MarketError> {
        let key = Self::canonical_owner_key(&update.owner_did);
        if let Some(&rev_epoch) = self.revoked_epoch_by_owner.get(&key) {
            if update.rotation_epoch <= rev_epoch {
                return Err(MarketError::Validation(format!(
                    "Rejected locator update for revoked epoch {} <= revoked {}",
                    update.rotation_epoch, rev_epoch
                )));
            }
        }
        if let Some(&hi_epoch) = self.highest_epoch_by_owner.get(&key) {
            if update.rotation_epoch < hi_epoch {
                return Err(MarketError::Validation(format!(
                    "Stale locator update epoch {} < highest seen {}",
                    update.rotation_epoch, hi_epoch
                )));
            }
        }

        if let Some(entry) = self.wards.get_mut(&key) {
            if update.rotation_epoch < entry.rotation_epoch {
                return Err(MarketError::Validation(format!(
                    "Stale locator update epoch {} < existing {}",
                    update.rotation_epoch, entry.rotation_epoch
                )));
            }
            entry.backup_sync_descriptors = update.backup_sync_descriptors.clone();
            entry.updated_at = update.updated_at;
            let hi = self.highest_epoch_by_owner.entry(key).or_insert(0);
            *hi = (*hi).max(update.rotation_epoch);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Removes a ward from the guardian vault and records monotonic high-water & revoke marks so
    /// replayed or out-of-order assigns at `<= revoke.rotation_epoch` are permanently rejected.
    pub fn revoke_ward(&mut self, revoke: &GuardianRevoke) -> Result<bool, MarketError> {
        let key = Self::canonical_owner_key(&revoke.owner_did);
        if let Some(&hi_epoch) = self.highest_epoch_by_owner.get(&key) {
            if revoke.rotation_epoch < hi_epoch {
                return Err(MarketError::Validation(format!(
                    "Stale revoke epoch {} < highest seen {}",
                    revoke.rotation_epoch, hi_epoch
                )));
            }
        }
        if let Some(entry) = self.wards.get(&key) {
            if revoke.rotation_epoch < entry.rotation_epoch {
                return Err(MarketError::Validation(format!(
                    "Stale revoke epoch {} < existing {}",
                    revoke.rotation_epoch, entry.rotation_epoch
                )));
            }
        }

        let removed = self.wards.remove(&key).is_some();
        let prev_hi = self.highest_epoch_by_owner.get(&key).copied();
        let next_hi = prev_hi.map_or(revoke.rotation_epoch, |v| v.max(revoke.rotation_epoch));
        let hi_changed = prev_hi != Some(next_hi);
        self.highest_epoch_by_owner.insert(key.clone(), next_hi);

        let prev_rev = self.revoked_epoch_by_owner.get(&key).copied();
        let next_rev = prev_rev.map_or(revoke.rotation_epoch, |v| v.max(revoke.rotation_epoch));
        let rev_changed = prev_rev != Some(next_rev);
        self.revoked_epoch_by_owner.insert(key, next_rev);

        Ok(removed || hi_changed || rev_changed)
    }
}

// ============================================================================
// Ephemeral 1-Tap URI Handshake
// ============================================================================

/// Computes a 6-digit SAS verification code:
/// `sas_num = u32::from_be_bytes(blake3(b"HUMOCO-SAS-V1" || pk_temp)[0..4]) % 1_000_000`
/// Formatted as `XXX-XXX` (e.g. `482-910`).
pub fn compute_recovery_sas(pk_temp: &[u8; 32]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"HUMOCO-SAS-V1");
    hasher.update(pk_temp);
    let hash = hasher.finalize();
    let num = u32::from_be_bytes([
        hash.as_bytes()[0],
        hash.as_bytes()[1],
        hash.as_bytes()[2],
        hash.as_bytes()[3],
    ]) % 1_000_000;
    format!("{:03}-{:03}", num / 1000, num % 1000)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryRequestLink {
    pub version: u8,
    pub pk_temp: [u8; 32],
    pub ts: u64,
    pub name_hint: Option<String>,
}

impl RecoveryRequestLink {
    pub fn is_stale_48h(&self, now_unix: u64) -> bool {
        now_unix.saturating_sub(self.ts) > 48 * 3600
    }
}

/// Builds `humoco://recovery-req?v=1&pk=<base64url>&ts=<unix>&name=<url_encoded>`
pub fn build_recovery_request_uri(pk_temp: &[u8; 32], ts: u64, name_hint: Option<&str>) -> String {
    let pk_b64 = URL_SAFE_NO_PAD.encode(pk_temp);
    let mut query = format!("v=1&pk={}&ts={}", pk_b64, ts);
    if let Some(name) = name_hint {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            let encoded: String = form_urlencoded::byte_serialize(trimmed.as_bytes()).collect();
            query.push_str(&format!("&name={}", encoded));
        }
    }
    format!("humoco://recovery-req?{}", query)
}

fn extract_query_from_uri_text<'a>(uri: &'a str, action: &str) -> Option<&'a str> {
    let trimmed = uri.trim();
    let lower = trimmed.to_ascii_lowercase();
    let full_prefix = format!("humoco://{}?", action);
    let idx = lower.find(&full_prefix)?;
    let raw_query = &trimmed[idx + full_prefix.len()..];
    Some(
        raw_query
            .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')'))
            .next()
            .unwrap_or("")
            .trim_end_matches(&['.', ',', ';', ':', '!', '?', ')', ']'][..]),
    )
}

/// Parses a `humoco://recovery-req` URI.
pub fn parse_recovery_request_uri(uri: &str) -> Result<RecoveryRequestLink, MarketError> {
    let query_str = extract_query_from_uri_text(uri, "recovery-req").ok_or_else(|| {
        MarketError::Validation("Invalid recovery request URI: missing 'recovery-req?'".to_string())
    })?;

    let mut v: Option<u8> = None;
    let mut pk_opt: Option<[u8; 32]> = None;
    let mut ts_opt: Option<u64> = None;
    let mut name_opt: Option<String> = None;

    for (key, val) in form_urlencoded::parse(query_str.as_bytes()) {
        match key.as_ref() {
            "v" => {
                let ver: u8 = val.parse().map_err(|_| {
                    MarketError::Validation("Invalid version in recovery request".to_string())
                })?;
                v = Some(ver);
            }
            "pk" => {
                let pk_bytes = URL_SAFE_NO_PAD
                    .decode(val.as_bytes())
                    .map_err(|e| MarketError::Validation(format!("Invalid pk base64url: {}", e)))?;
                let arr: [u8; 32] = pk_bytes.try_into().map_err(|_| {
                    MarketError::Validation("pk must be exactly 32 bytes".to_string())
                })?;
                pk_opt = Some(arr);
            }
            "ts" => {
                let ts: u64 = val.parse().map_err(|_| {
                    MarketError::Validation("Invalid ts in recovery request".to_string())
                })?;
                ts_opt = Some(ts);
            }
            "name" => {
                let n = val.trim().to_string();
                if !n.is_empty() {
                    name_opt = Some(n);
                }
            }
            _ => {}
        }
    }

    let version = v.ok_or_else(|| {
        MarketError::Validation("Missing 'v' in recovery request URI".to_string())
    })?;
    if version != 1 {
        return Err(MarketError::Validation(format!(
            "Unsupported recovery request version: {}",
            version
        )));
    }
    let pk_temp = pk_opt.ok_or_else(|| {
        MarketError::Validation("Missing 'pk' in recovery request URI".to_string())
    })?;
    let ts = ts_opt.ok_or_else(|| {
        MarketError::Validation("Missing 'ts' in recovery request URI".to_string())
    })?;

    Ok(RecoveryRequestLink {
        version,
        pk_temp,
        ts,
        name_hint: name_opt,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianReleasePayload {
    pub entry: GuardianVaultEntry,
    pub extra_sync_descriptors: Vec<DropDescriptor>,
    pub released_at: u64,
}

/// Seals a `GuardianReleasePayload` to ephemeral `pk_temp` via ephemeral X25519 + HKDF-SHA256 + `XChaCha20Poly1305`.
///
/// Output: `humoco://recovery-reply?v=1&data=<base64url>`
pub fn seal_guardian_release_reply(
    pk_temp: &[u8; 32],
    payload: &GuardianReleasePayload,
) -> Result<String, MarketError> {
    let eph_sk = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let eph_pk = x25519_dalek::PublicKey::from(&eph_sk);

    let peer_pk = x25519_dalek::PublicKey::from(*pk_temp);
    let shared = eph_sk.diffie_hellman(&peer_pk);
    if !shared.was_contributory() {
        return Err(MarketError::Crypto(
            "Non-contributory Diffie-Hellman in recovery reply".to_string(),
        ));
    }
    if shared.as_bytes() == &[0u8; 32] {
        return Err(MarketError::Crypto(
            "All-zero DH shared secret in recovery reply".to_string(),
        ));
    }

    let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut k_reply = [0u8; 32];
    hkdf.expand(b"HUMOCO-REC-REPLY-V1", &mut k_reply)
        .map_err(|e| {
            MarketError::Crypto(format!("HKDF expand failed for recovery reply: {}", e))
        })?;

    let cipher = XChaCha20Poly1305::new_from_slice(&k_reply)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;

    let mut msg_nonce = [0u8; 24];
    OsRng.fill_bytes(&mut msg_nonce);
    let nonce = XNonce::from_slice(&msg_nonce);

    let plaintext =
        bincode::serialize(payload).map_err(|e| MarketError::Serialization(e.to_string()))?;

    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: &plaintext,
                aad: b"HUMOCO-REC-REPLY-V1",
            },
        )
        .map_err(|e| MarketError::Crypto(format!("Encryption failed for recovery reply: {}", e)))?;

    let mut package = Vec::with_capacity(32 + 24 + ciphertext.len());
    package.extend_from_slice(eph_pk.as_bytes());
    package.extend_from_slice(&msg_nonce);
    package.extend_from_slice(&ciphertext);

    let data_b64 = URL_SAFE_NO_PAD.encode(&package);
    Ok(format!("humoco://recovery-reply?v=1&data={}", data_b64))
}

/// Opens and decrypts a `humoco://recovery-reply` URI with `sk_temp`, and **immediately verifies** the contained `GuardianVaultEntry`.
pub fn open_and_verify_guardian_release_reply(
    sk_temp: &[u8; 32],
    uri: &str,
) -> Result<GuardianReleasePayload, MarketError> {
    // Rejoin email hard-wrapped base64url continuation lines when the URI contains humoco://recovery-reply?
    let unwrapped;
    let candidate = if uri
        .trim()
        .to_ascii_lowercase()
        .contains("humoco://recovery-reply?")
    {
        let trimmed = uri.trim();
        let lower = trimmed.to_ascii_lowercase();
        let idx = lower.find("humoco://recovery-reply?").unwrap_or(0);
        let slice = &trimmed[idx..];
        let mut lines = slice.lines();
        let mut joined = lines.next().map(str::trim).unwrap_or("").to_string();
        for line in lines {
            let t = line.trim();
            if t.is_empty() {
                break;
            }
            let stripped = t.trim_end_matches(&['.', ',', ';', ':', '!', '?', ')', ']'][..]);
            let is_short_prose_word = t != stripped
                && stripped.len() < 24
                && stripped.chars().all(|c| c.is_ascii_alphabetic());
            if !stripped.is_empty()
                && !is_short_prose_word
                && stripped
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '='))
            {
                joined.push_str(t);
            } else {
                break;
            }
        }
        unwrapped = joined;
        unwrapped.as_str()
    } else {
        uri
    };
    let query_str = extract_query_from_uri_text(candidate, "recovery-reply").ok_or_else(|| {
        MarketError::Validation("Invalid recovery reply URI: missing 'recovery-reply?'".to_string())
    })?;

    let mut v: Option<u8> = None;
    let mut data_opt: Option<String> = None;

    for (key, val) in form_urlencoded::parse(query_str.as_bytes()) {
        match key.as_ref() {
            "v" => {
                let ver: u8 = val.parse().map_err(|_| {
                    MarketError::Validation("Invalid version in recovery reply".to_string())
                })?;
                v = Some(ver);
            }
            "data" => {
                data_opt = Some(val.into_owned());
            }
            _ => {}
        }
    }

    let version =
        v.ok_or_else(|| MarketError::Validation("Missing 'v' in recovery reply URI".to_string()))?;
    if version != 1 {
        return Err(MarketError::Validation(format!(
            "Unsupported recovery reply version: {}",
            version
        )));
    }
    let data_b64 = data_opt.ok_or_else(|| {
        MarketError::Validation("Missing 'data' in recovery reply URI".to_string())
    })?;

    let package = URL_SAFE_NO_PAD
        .decode(data_b64.as_bytes())
        .map_err(|e| MarketError::Validation(format!("Invalid base64url data in reply: {}", e)))?;

    if package.len() < 32 + 24 + 16 {
        return Err(MarketError::Validation(
            "Recovery reply package too short".to_string(),
        ));
    }

    let eph_pk_bytes: [u8; 32] = package[0..32].try_into().unwrap();
    let msg_nonce: [u8; 24] = package[32..56].try_into().unwrap();
    let ciphertext = &package[56..];

    let my_sk = x25519_dalek::StaticSecret::from(*sk_temp);
    let peer_pk = x25519_dalek::PublicKey::from(eph_pk_bytes);
    let shared = my_sk.diffie_hellman(&peer_pk);
    if !shared.was_contributory() {
        return Err(MarketError::Crypto(
            "Non-contributory Diffie-Hellman in recovery reply".to_string(),
        ));
    }
    if shared.as_bytes() == &[0u8; 32] {
        return Err(MarketError::Crypto(
            "All-zero DH shared secret in recovery reply".to_string(),
        ));
    }

    let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut k_reply = [0u8; 32];
    hkdf.expand(b"HUMOCO-REC-REPLY-V1", &mut k_reply)
        .map_err(|e| {
            MarketError::Crypto(format!("HKDF expand failed for recovery reply: {}", e))
        })?;

    let cipher = XChaCha20Poly1305::new_from_slice(&k_reply)
        .map_err(|e| MarketError::Crypto(e.to_string()))?;
    let nonce = XNonce::from_slice(&msg_nonce);

    let plaintext = cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad: b"HUMOCO-REC-REPLY-V1",
            },
        )
        .map_err(|e| MarketError::Crypto(format!("Failed to decrypt recovery reply: {}", e)))?;

    let payload: GuardianReleasePayload = bincode::deserialize(&plaintext).map_err(|e| {
        MarketError::Serialization(format!("Failed to parse GuardianReleasePayload: {}", e))
    })?;

    verify_guardian_vault_entry(&payload.entry)?;

    Ok(payload)
}

// ============================================================================
// High-Level End-to-End Bundle & Recovery Helpers
// ============================================================================

/// High-level bundle generation: derives `P`, generates random `S`, seals `master_payload` into `r_enc`,
/// splits `C = S XOR P` into `total_n` Shamir shares, signs the header with `owner_signing_key`,
/// and returns `total_n` sealed `GuardianVaultEntry` objects ready to send to guardians.
#[allow(clippy::too_many_arguments)]
pub fn create_social_recovery_vault_bundle(
    owner_signing_key: &SigningKey,
    owner_did: &str,
    owner_display_name: &str,
    rotation_epoch: u32,
    threshold_m: u8,
    total_n: u8,
    question_ids: &[u8; 3],
    answers: &[&str; 3],
    kdf_params: &RecoveryKdfParams,
    master_payload: &RecoveryMasterPayload,
    backup_sync_descriptors: &[DropDescriptor],
    now_unix: u64,
) -> Result<Vec<GuardianVaultEntry>, MarketError> {
    let canonical_owner = canonicalize_owner_account_id(owner_did)?;
    let owner_pk = get_pubkey_from_user_id(&canonical_owner)
        .map_err(|e| MarketError::Identity(format!("Invalid owner DID: {}", e)))?;
    if owner_pk != owner_signing_key.verifying_key() {
        return Err(MarketError::Identity(format!(
            "owner_did {} does not match signing key public key",
            canonical_owner
        )));
    }
    if master_payload.target_did != canonical_owner {
        return Err(MarketError::Validation(format!(
            "master_payload target_did {} does not match owner_did {}",
            master_payload.target_did, canonical_owner
        )));
    }

    validate_kdf_params(kdf_params)?;

    // Derive P from answers
    let p = derive_recovery_key(question_ids, answers, kdf_params)?;

    // Generate random secret S (32 bytes)
    let mut s_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut s_bytes);
    let s = Zeroizing::new(s_bytes);

    // Seal master payload with S
    let r_enc = seal_recovery_master_payload(master_payload, &s)?;

    // Mask C = S XOR P
    let mut c_bytes = [0u8; 32];
    for i in 0..32 {
        c_bytes[i] = s[i] ^ p[i];
    }
    let c = Zeroizing::new(c_bytes);

    // Split C into N Shamir shares
    let shares = split_secret_32(&c, threshold_m, total_n)?;

    // Compute share commitments
    let mut commitments = Vec::with_capacity(total_n as usize);
    for share in &shares {
        commitments.push(compute_share_commitment(
            &canonical_owner,
            rotation_epoch,
            share,
        ));
    }

    // Sign header
    let signing_bytes = guardian_entry_signing_bytes(
        &canonical_owner,
        rotation_epoch,
        threshold_m,
        total_n,
        kdf_params,
        question_ids,
        &r_enc,
        &commitments,
    );
    let sig = owner_signing_key.sign(&signing_bytes);
    let owner_sig: [u8; 64] = sig.to_bytes();

    let mut entries = Vec::with_capacity(total_n as usize);
    for share in shares {
        entries.push(GuardianVaultEntry {
            owner_did: canonical_owner.clone(),
            owner_display_name: owner_display_name.to_string(),
            rotation_epoch,
            threshold_m,
            total_n,
            share,
            share_commitments: commitments.clone(),
            r_enc: r_enc.clone(),
            kdf_params: kdf_params.clone(),
            question_ids: *question_ids,
            owner_sig,
            backup_sync_descriptors: backup_sync_descriptors.to_vec(),
            updated_at: now_unix,
        });
    }

    Ok(entries)
}

/// Reconstructs the `RecoveryMasterPayload` from at least `threshold_m` verified guardian vault entries
/// and user-supplied answers to the 3 recovery questions.
pub fn recover_master_payload_from_shares(
    entries: &[GuardianVaultEntry],
    answers: &[&str; 3],
) -> Result<RecoveryMasterPayload, MarketError> {
    if entries.is_empty() {
        return Err(MarketError::Validation(
            "No guardian vault entries provided".to_string(),
        ));
    }

    let first = &entries[0];
    let threshold_m = first.threshold_m;
    if entries.len() < threshold_m as usize {
        return Err(MarketError::Validation(format!(
            "Insufficient guardian entries: got {}, required {}",
            entries.len(),
            threshold_m
        )));
    }

    for (i, entry) in entries.iter().enumerate() {
        verify_guardian_vault_entry(entry)?;
        if entry.owner_did != first.owner_did {
            return Err(MarketError::Validation(format!(
                "Mismatched owner_did in entry {}: {} != {}",
                i, entry.owner_did, first.owner_did
            )));
        }
        if entry.rotation_epoch != first.rotation_epoch {
            return Err(MarketError::Validation(format!(
                "Mismatched rotation_epoch in entry {}: {} != {}",
                i, entry.rotation_epoch, first.rotation_epoch
            )));
        }
    }

    // Combine shares into C
    let shares: Vec<ShamirShare> = entries.iter().map(|e| e.share.clone()).collect();
    let c = combine_shares_32(&shares, threshold_m)?;

    // Derive P from user answers
    let p = derive_recovery_key(&first.question_ids, answers, &first.kdf_params)?;

    // Reconstruct S = C XOR P
    let mut s_bytes = [0u8; 32];
    for i in 0..32 {
        s_bytes[i] = c[i] ^ p[i];
    }
    let s = Zeroizing::new(s_bytes);

    // Unseal master payload
    let payload = unseal_recovery_master_payload(&first.r_enc, &s)?;

    if payload.target_did != first.owner_did {
        return Err(MarketError::Validation(format!(
            "Recovered target_did {} does not match entry owner_did {}",
            payload.target_did, first.owner_did
        )));
    }

    Ok(payload)
}

// ============================================================================
// Emergency Kit URI
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyKitData {
    pub version: u8,
    pub entropy_hex: String,
    pub language_code: u8,
    pub user_prefix: Option<String>,
    pub has_bip39_passphrase: bool,
    pub sync_descriptors: Vec<DropDescriptor>,
}

/// Builds `humoco://kit?v=1&ent=<hex>&lang=<code>&prefix=...&pass=<0|1>&sync=<base64url_json>`
pub fn build_emergency_kit_uri(kit: &EmergencyKitData) -> Result<String, MarketError> {
    if kit.version != 1 {
        return Err(MarketError::Validation(format!(
            "Unsupported emergency kit version: {}",
            kit.version
        )));
    }

    let entropy_bytes = hex::decode(&kit.entropy_hex)
        .map_err(|e| MarketError::Validation(format!("Invalid entropy hex: {}", e)))?;
    if entropy_bytes.len() != 16 && entropy_bytes.len() != 32 {
        return Err(MarketError::Validation(format!(
            "Emergency kit entropy must be 16 or 32 bytes (got {})",
            entropy_bytes.len()
        )));
    }

    if !(1..=11).contains(&kit.language_code) {
        return Err(MarketError::Validation(format!(
            "Invalid language code: {}",
            kit.language_code
        )));
    }

    let sync_json = serde_json::to_vec(&kit.sync_descriptors)
        .map_err(|e| MarketError::Serialization(e.to_string()))?;
    let sync_b64 = URL_SAFE_NO_PAD.encode(&sync_json);

    let pass_str = if kit.has_bip39_passphrase { "1" } else { "0" };

    let mut query = format!(
        "v=1&ent={}&lang={}&pass={}&sync={}",
        kit.entropy_hex, kit.language_code, pass_str, sync_b64
    );
    if let Some(ref prefix) = kit.user_prefix {
        let trimmed = prefix.trim();
        if !trimmed.is_empty() {
            let encoded: String = form_urlencoded::byte_serialize(trimmed.as_bytes()).collect();
            query.push_str(&format!("&prefix={}", encoded));
        }
    }

    Ok(format!("humoco://kit?{}", query))
}

/// Parses a `humoco://kit` URI.
pub fn parse_emergency_kit_uri(uri: &str) -> Result<EmergencyKitData, MarketError> {
    let query_str = extract_query_from_uri_text(uri, "kit").ok_or_else(|| {
        MarketError::Validation("Invalid emergency kit URI: missing 'kit?'".to_string())
    })?;

    let mut v: Option<u8> = None;
    let mut ent: Option<String> = None;
    let mut lang: Option<u8> = None;
    let mut pass: Option<bool> = None;
    let mut sync: Option<Vec<DropDescriptor>> = None;
    let mut prefix: Option<String> = None;

    for (key, val) in form_urlencoded::parse(query_str.as_bytes()) {
        match key.as_ref() {
            "v" => {
                let ver: u8 = val.parse().map_err(|_| {
                    MarketError::Validation("Invalid version number in kit".to_string())
                })?;
                v = Some(ver);
            }
            "ent" => {
                let ent_str = val.into_owned();
                let bytes = hex::decode(&ent_str)
                    .map_err(|e| MarketError::Validation(format!("Invalid entropy hex: {}", e)))?;
                if bytes.len() != 16 && bytes.len() != 32 {
                    return Err(MarketError::Validation(format!(
                        "Entropy must be 16 or 32 bytes, got {}",
                        bytes.len()
                    )));
                }
                ent = Some(ent_str);
            }
            "lang" => {
                let code: u8 = val.parse().map_err(|_| {
                    MarketError::Validation("Invalid language code in kit".to_string())
                })?;
                if !(1..=11).contains(&code) {
                    return Err(MarketError::Validation(format!(
                        "Language code out of range: {}",
                        code
                    )));
                }
                lang = Some(code);
            }
            "pass" => match val.as_ref() {
                "1" => pass = Some(true),
                "0" => pass = Some(false),
                other => {
                    return Err(MarketError::Validation(format!(
                        "Invalid pass flag in kit: {}",
                        other
                    )))
                }
            },
            "sync" => {
                let sync_bytes = URL_SAFE_NO_PAD.decode(val.as_bytes()).map_err(|e| {
                    MarketError::Validation(format!("Invalid sync base64url: {}", e))
                })?;
                let descriptors: Vec<DropDescriptor> = serde_json::from_slice(&sync_bytes)
                    .map_err(|e| {
                        MarketError::Serialization(format!("Invalid sync descriptors JSON: {}", e))
                    })?;
                sync = Some(descriptors);
            }
            "prefix" => {
                let p = val.trim().to_string();
                if !p.is_empty() {
                    prefix = Some(p);
                }
            }
            _ => {}
        }
    }

    let version =
        v.ok_or_else(|| MarketError::Validation("Missing 'v' in emergency kit URI".to_string()))?;
    if version != 1 {
        return Err(MarketError::Validation(format!(
            "Unsupported emergency kit version: {}",
            version
        )));
    }
    let entropy_hex = ent
        .ok_or_else(|| MarketError::Validation("Missing 'ent' in emergency kit URI".to_string()))?;
    let language_code = lang.ok_or_else(|| {
        MarketError::Validation("Missing 'lang' in emergency kit URI".to_string())
    })?;
    let has_bip39_passphrase = pass.ok_or_else(|| {
        MarketError::Validation("Missing 'pass' in emergency kit URI".to_string())
    })?;
    let sync_descriptors = sync.unwrap_or_default();

    Ok(EmergencyKitData {
        version,
        entropy_hex,
        language_code,
        user_prefix: prefix,
        has_bip39_passphrase,
        sync_descriptors,
    })
}
