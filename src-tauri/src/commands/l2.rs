use chrono::Utc;
use human_money_core::humoco_proto::{
    decode_hex_fixed, types::QuorumCertificate, DEAD_QUORUM_CLAIM_WINDOW_SECS,
};
use human_money_core::models::voucher::Voucher;
use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
use human_money_core::services::{
    crypto::derive_ed25519_keypair,
    dead_quorum_swap::{
        adjudicate_dead_quorum_claims, solve_swap_ticket_pow, DeadQuorumClaimSubmission,
        DeadQuorumSwapOffer,
    },
    l2_rotation,
};
use humoco_wot::{KeyRotationReason, KeySuccessorAttestation};
use uuid::Uuid;

use crate::l2_lifecycle::{
    build_status_dto, discover_anchor_manifest, lifecycle_err, load_key_successor_attestations,
    load_lifecycle_state, run_lifecycle_tick_with_now, save_key_successor_attestations,
    save_lifecycle_state, strip_user_id_prefix, FrontendAnchorDiscoveryDto,
    FrontendApprovedSwapClaimDto, FrontendClaimBundleDto, FrontendDeadQuorumAdjudicationDto,
    FrontendL2LifecycleStatusDto, FrontendRejectedSwapClaimDto, StoredGuarantorRole,
};
use crate::models::{
    AnchorServerInputDto, FrontendError, FrontendL2LockStatusDto, FrontendVoucherL2StatusDto,
    MnemonicLanguage, TransactionRecord,
};
use crate::services::l2_anchor_service;
use crate::settings::{NominatedLockServerMeta, StoredAnchorTemplateMeta};
use crate::state::{AppState, DidWotSigner};
use crate::sync_client::{self, FrontendLockServerProbeResult};

#[tauri::command]
pub fn get_seal_sync_status(state: tauri::State<AppState>) -> Result<String, FrontendError> {
    l2_anchor_service::get_seal_sync_status_impl(&state)
}

#[tauri::command]
pub async fn probe_lock_server_cmd(
    write_url: String,
    expected_admin_pub: Option<String>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendLockServerProbeResult, FrontendError> {
    l2_anchor_service::probe_lock_server_cmd_impl(write_url, expected_admin_pub, password, &state)
        .await
}

#[tauri::command]
pub fn set_own_nominated_lock_server(
    write_url: Option<String>,
    asn: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Option<NominatedLockServerMeta>, FrontendError> {
    l2_anchor_service::set_own_nominated_lock_server_impl(write_url, asn, password, &state)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn assemble_anchor_template(
    label: String,
    retain_until_quarter_str: String,
    servers: Vec<AnchorServerInputDto>,
    guarantor_mig_pubs_hex: Vec<String>,
    threshold: Option<u16>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<StoredAnchorTemplateMeta, FrontendError> {
    l2_anchor_service::assemble_anchor_template_impl(
        label,
        retain_until_quarter_str,
        servers,
        guarantor_mig_pubs_hex,
        threshold,
        password,
        &state,
    )
    .await
}

#[tauri::command]
pub async fn prewarm_l2_checkout(state: tauri::State<'_, AppState>) -> Result<(), FrontendError> {
    l2_anchor_service::prewarm_l2_checkout_impl(&state).await
}

#[tauri::command]
pub fn check_l2_lock_status(
    state: tauri::State<AppState>,
) -> Result<FrontendL2LockStatusDto, FrontendError> {
    l2_anchor_service::check_l2_lock_status_impl(&state)
}

#[tauri::command]
pub async fn check_voucher_l2_status(
    local_instance_id: String,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendVoucherL2StatusDto, FrontendError> {
    l2_anchor_service::check_voucher_l2_status_impl(local_instance_id, password, &state).await
}

#[tauri::command]
pub fn get_l2_lifecycle_status(
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendL2LifecycleStatusDto, FrontendError> {
    let mut service = state
        .service
        .lock()
        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
    let lc = load_lifecycle_state(&mut service, password.as_deref());
    build_status_dto(&mut service, &lc, password.as_deref())
}

#[tauri::command]
pub async fn run_l2_lifecycle_tick(
    force: bool,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendL2LifecycleStatusDto, FrontendError> {
    let client = sync_client::create_sync_probe_client();
    let now_secs = sync_client::current_unix_timestamp();
    run_lifecycle_tick_with_now(state.inner(), &client, now_secs, force, password.as_deref()).await
}

#[tauri::command]
pub async fn discover_anchor_manifest_cmd(
    anchor_id_hex: String,
    creator_did: Option<String>,
    guarantor_dids: Vec<String>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<Option<FrontendAnchorDiscoveryDto>, FrontendError> {
    let anchor_id = decode_hex_fixed::<32>(anchor_id_hex.trim()).map_err(lifecycle_err)?;
    let settings = state.get_cached_settings().unwrap_or_default();
    let client = sync_client::create_sync_probe_client();
    let found = discover_anchor_manifest(
        state.inner(),
        &settings,
        &client,
        &anchor_id,
        creator_did.as_deref(),
        &guarantor_dids,
        password.as_deref(),
    )
    .await?;

    Ok(found.map(|m| FrontendAnchorDiscoveryDto {
        anchor_id: hex::encode(anchor_id),
        revision: m.revision,
        threshold: m.threshold,
        server_count: m.servers.len(),
        reduced_guarantor_protection: m.has_reduced_guarantor_protection().unwrap_or(false),
        manifest_json: serde_json::to_string_pretty(&m).unwrap_or_default(),
    }))
}

#[tauri::command]
pub fn register_guarantor_anchor_role(
    anchor_id_hex: String,
    guarantor_index: u8,
    context_nonce_hex: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    let mut service = state
        .service
        .lock()
        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;

    let trimmed_anchor = anchor_id_hex.trim().to_ascii_lowercase();
    let key = if trimmed_anchor.is_empty() {
        format!("pending:{}", guarantor_index)
    } else {
        decode_hex_fixed::<32>(&trimmed_anchor).map_err(lifecycle_err)?;
        trimmed_anchor
    };

    let nonce_bytes = match context_nonce_hex
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(h) => hex::decode(h).map_err(|e| lifecycle_err(e.to_string()))?,
        None => {
            let mut buf = [0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut buf);
            buf.to_vec()
        }
    };

    let pub_hex = service
        .with_wallet_and_identity(|_, id| {
            let sk = l2_rotation::derive_guarantor_mig_signing_key(id, &nonce_bytes)?;
            Ok::<_, human_money_core::Error>(hex::encode(sk.verifying_key().to_bytes()))
        })
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;

    let mut lc = load_lifecycle_state(&mut service, password.as_deref());
    lc.guarantor_roles.insert(
        key.clone(),
        StoredGuarantorRole {
            anchor_id: key,
            guarantor_index,
            context_nonce_hex: hex::encode(&nonce_bytes),
        },
    );
    save_lifecycle_state(&mut service, &lc, password.as_deref())?;
    Ok(pub_hex)
}

#[tauri::command]
pub fn create_key_successor_attestation_cmd(
    new_mnemonic: String,
    language: Option<MnemonicLanguage>,
    passphrase: Option<String>,
    reason: String,
    new_creator_mig_pub_hex: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    let rot_reason = match reason.trim().to_ascii_lowercase().as_str() {
        "compromise" => KeyRotationReason::Compromise,
        "device_migration" | "devicemigration" => KeyRotationReason::DeviceMigration,
        _ => KeyRotationReason::Routine,
    };
    let core_lang: human_money_core::MnemonicLanguage =
        language.unwrap_or(MnemonicLanguage::English).into();
    let (new_vk, new_sk) = derive_ed25519_keypair(
        new_mnemonic.trim(),
        passphrase.as_deref().filter(|s| !s.is_empty()),
        core_lang,
    )
    .map_err(FrontendError::from)?;
    let new_did = human_money_core::services::crypto::create_user_id(&new_vk, None)
        .map_err(|e| lifecycle_err(e.to_string()))?;
    let new_mig_pub = match new_creator_mig_pub_hex
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(h) => Some(decode_hex_fixed::<32>(h).map_err(lifecycle_err)?),
        None => None,
    };

    let now_secs = sync_client::current_unix_timestamp().max(1);
    let mut service = state
        .service
        .lock()
        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;

    let att = service
        .with_wallet_and_identity(|w, id| {
            let old_signer = DidWotSigner {
                did: strip_user_id_prefix(w.get_user_id()).to_string(),
                signing_key: id.signing_key.clone(),
            };
            let new_signer = DidWotSigner {
                did: strip_user_id_prefix(&new_did).to_string(),
                signing_key: new_sk.clone(),
            };
            KeySuccessorAttestation::sign_dual(
                &old_signer,
                &new_signer,
                rot_reason,
                new_mig_pub,
                now_secs,
            )
        })
        .map_err(FrontendError::from)?
        .map_err(|e| lifecycle_err(e.to_string()))?;

    let mut store = load_key_successor_attestations(&mut service, password.as_deref());
    if !store.contains(&att) {
        store.push(att.clone());
        save_key_successor_attestations(&mut service, &store, password.as_deref())?;
    }

    serde_json::to_string_pretty(&att).map_err(|e| lifecycle_err(e.to_string()))
}

#[tauri::command]
pub fn import_key_successor_attestation_cmd(
    attestation_json: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    let att: KeySuccessorAttestation = serde_json::from_str(attestation_json.trim())
        .map_err(|e| lifecycle_err(format!("Invalid KeySuccessorAttestation JSON: {}", e)))?;
    let now_secs = sync_client::current_unix_timestamp();
    att.verify(now_secs)
        .map_err(|e| lifecycle_err(format!("Attestation verification failed: {}", e)))?;

    let mut service = state
        .service
        .lock()
        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
    let mut store = load_key_successor_attestations(&mut service, password.as_deref());
    if !store.contains(&att) {
        store.push(att.clone());
        save_key_successor_attestations(&mut service, &store, password.as_deref())?;
    }
    Ok(att.new_did)
}

#[tauri::command]
pub fn create_guarantor_claim_bundle(
    local_instance_id: String,
    guarantor_id: String,
    amount: String,
    standard_toml: String,
    notes: Option<String>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendClaimBundleDto, FrontendError> {
    let mut service = state
        .service
        .lock()
        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;

    if let Some(ref pwd) = password {
        let _ = service.unlock_session(pwd, 300);
    }

    let sender_id = service
        .with_wallet(|w| w.get_user_id().to_string())
        .map_err(FrontendError::from)?;

    let res = service
        .create_guarantor_claim_bundle(
            &local_instance_id,
            guarantor_id.trim(),
            amount.trim(),
            &standard_toml,
            None,
            notes.clone(),
            password.as_deref(),
        )
        .map_err(FrontendError::from)?;

    let mut summable_amounts = std::collections::HashMap::new();
    let mut countable_items = std::collections::HashMap::new();
    if let Some(first) = res.involved_sources_details.first() {
        summable_amounts.insert(first.display_currency.clone(), first.amount.clone());
    } else {
        countable_items.insert("Claim".to_string(), 1);
    }

    let record = TransactionRecord {
        id: Uuid::new_v4().to_string(),
        direction: "sent".to_string(),
        recipient_id: guarantor_id.trim().to_string(),
        sender_id,
        timestamp: Utc::now().to_rfc3339(),
        summable_amounts,
        countable_items,
        involved_vouchers: vec![local_instance_id],
        involved_sources_details: Some(
            res.involved_sources_details
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
        ),
        bundle_data: res.bundle_bytes.clone(),
        bundle_id: res.bundle_id.clone(),
        notes,
        sender_profile_name: None,
    };
    let _ = state.append_to_history(&mut service, record, password.as_deref());
    let _ = state.refresh_events_cache(&mut service, password.as_deref());

    Ok(FrontendClaimBundleDto {
        bundle_data: res.bundle_bytes,
        bundle_id: res.bundle_id,
        guarantor_id: guarantor_id.trim().to_string(),
        claimed_amount: amount.trim().to_string(),
    })
}

#[tauri::command]
pub fn create_dead_quorum_swap_offer_cmd(
    dead_anchor_id_hex: String,
    replacement_anchor_id_hex: String,
    claim_window_secs: Option<u64>,
    pow_bits: Option<u32>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    let dead_id = decode_hex_fixed::<32>(dead_anchor_id_hex.trim()).map_err(lifecycle_err)?;
    let repl_id =
        decode_hex_fixed::<32>(replacement_anchor_id_hex.trim()).map_err(lifecycle_err)?;
    let window = claim_window_secs
        .unwrap_or(DEAD_QUORUM_CLAIM_WINDOW_SECS)
        .max(DEAD_QUORUM_CLAIM_WINDOW_SECS);
    let bits = pow_bits.unwrap_or(10);
    let now_secs = sync_client::current_unix_timestamp().max(1);

    let mut service = state
        .service
        .lock()
        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
    if let Some(ref pwd) = password {
        let _ = service.unlock_session(pwd, 300);
    }

    let offer = service
        .with_wallet_and_identity(|_, id| {
            DeadQuorumSwapOffer::create_and_sign(id, dead_id, repl_id, now_secs, window, bits)
        })
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;

    serde_json::to_string_pretty(&offer).map_err(|e| lifecycle_err(e.to_string()))
}

#[tauri::command]
pub fn create_dead_quorum_swap_claim_cmd(
    offer_json: String,
    local_instance_id: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    let offer: DeadQuorumSwapOffer = serde_json::from_str(offer_json.trim())
        .map_err(|e| lifecycle_err(format!("Invalid DeadQuorumSwapOffer JSON: {}", e)))?;
    let offer_digest = offer.verify().map_err(FrontendError::from)?;

    let service = state
        .service
        .lock()
        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
    let _ = password;

    let (voucher, claimant_did, qcs): (Voucher, String, Vec<QuorumCertificate>) = service
        .with_wallet(|w| {
            let inst = w.get_voucher_instance(&local_instance_id).ok_or_else(|| {
                human_money_core::Error::VoucherNotFound(local_instance_id.clone())
            })?;
            let init_t_id = inst
                .voucher
                .transactions
                .first()
                .map(|t| t.t_id.as_str())
                .unwrap_or("");
            let qcs = w
                .profile
                .l2_quorum_certificates
                .get(init_t_id)
                .cloned()
                .unwrap_or_default();
            Ok::<_, human_money_core::Error>((
                inst.voucher.clone(),
                w.get_user_id().to_string(),
                qcs,
            ))
        })
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;

    if voucher.layer2_anchor != Some(offer.dead_anchor_id) {
        return Err(lifecycle_err(
            "Selected voucher does not belong to the dead_anchor_id in this swap offer",
        ));
    }
    let tip_t_id = voucher
        .transactions
        .last()
        .map(|t| t.t_id.clone())
        .unwrap_or_default();

    let pow_nonce = solve_swap_ticket_pow(
        &offer_digest,
        &voucher.voucher_id,
        &tip_t_id,
        &claimant_did,
        offer.swap_ticket_pow_bits,
        0,
        5_000_000,
    )
    .map_err(FrontendError::from)?;

    let submission = DeadQuorumClaimSubmission {
        voucher,
        claimant_did,
        quorum_certificates: qcs,
        swap_pow_nonce: pow_nonce,
    };
    serde_json::to_string_pretty(&submission).map_err(|e| lifecycle_err(e.to_string()))
}

#[tauri::command]
pub fn adjudicate_dead_quorum_swap_cmd(
    offer_json: String,
    submissions_json: Vec<String>,
    standard_toml: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendDeadQuorumAdjudicationDto, FrontendError> {
    let offer: DeadQuorumSwapOffer = serde_json::from_str(offer_json.trim())
        .map_err(|e| lifecycle_err(format!("Invalid DeadQuorumSwapOffer JSON: {}", e)))?;
    let (std_def, _) =
        VoucherStandardDefinition::from_toml(&standard_toml).map_err(FrontendError::from)?;

    let mut submissions = Vec::with_capacity(submissions_json.len());
    for raw in &submissions_json {
        let sub: DeadQuorumClaimSubmission = serde_json::from_str(raw.trim())
            .map_err(|e| lifecycle_err(format!("Invalid DeadQuorumClaimSubmission JSON: {}", e)))?;
        submissions.push(sub);
    }

    let now_secs = sync_client::current_unix_timestamp();
    let dead_hex = hex::encode(offer.dead_anchor_id);

    let mut service = state
        .service
        .lock()
        .map_err(|_| lifecycle_err("Wallet service unavailable"))?;
    if let Some(ref pwd) = password {
        let _ = service.unlock_session(pwd, 300);
    }

    let adj = service
        .with_wallet_and_identity(|w, id| {
            let dead_manifest = w.profile.l2_anchor_manifests.get(&dead_hex);
            adjudicate_dead_quorum_claims(
                &offer,
                dead_manifest,
                &std_def,
                &submissions,
                id,
                now_secs,
            )
        })
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;

    let proofs_count = adj.double_spend_proofs.len();
    for proof in adj.double_spend_proofs {
        let _ = service.import_proof(proof, password.as_deref());
    }

    Ok(FrontendDeadQuorumAdjudicationDto {
        approved: adj
            .approved
            .into_iter()
            .map(|a| FrontendApprovedSwapClaimDto {
                claimant_did: a.claimant_did,
                voucher_id: a.voucher_id,
                tip_t_id: a.tip_t_id,
                approved_amount: a.approved_amount,
            })
            .collect(),
        rejected: adj
            .rejected
            .into_iter()
            .map(|r| FrontendRejectedSwapClaimDto {
                claimant_did: r.claimant_did,
                voucher_id: r.voucher_id,
                tip_t_id: r.tip_t_id,
                reason: r.reason,
            })
            .collect(),
        double_spend_proofs_count: proofs_count,
    })
}
