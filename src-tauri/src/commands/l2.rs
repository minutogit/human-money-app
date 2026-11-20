use chrono::Utc;
use human_money_core::humoco_proto::{
    decode_hex_fixed, types::QuorumCertificate, DEAD_QUORUM_CLAIM_WINDOW_SECS,
};
use human_money_core::models::voucher::Voucher;
use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
use human_money_core::services::{
    crypto::derive_ed25519_keypair,
    dead_quorum_swap::{solve_swap_ticket_pow, DeadQuorumClaimSubmission, DeadQuorumSwapOffer},
    l2_rotation,
};
use humoco_wot::{KeyRotationReason, KeySuccessorAttestation};
use uuid::Uuid;

use crate::l2_lifecycle::{
    build_status_dto, discover_anchor_manifest, lifecycle_err, load_key_successor_attestations,
    load_lifecycle_state, run_lifecycle_tick_with_now, save_key_successor_attestations,
    save_lifecycle_state, FrontendAnchorDiscoveryDto, FrontendApprovedSwapClaimDto,
    FrontendClaimBundleDto, FrontendDeadQuorumAdjudicationDto, FrontendL2LifecycleStatusDto,
    FrontendRejectedSwapClaimDto, StoredGuarantorRole,
};
use crate::models::{
    AnchorServerInputDto, FrontendError, FrontendL2LockStatusDto, FrontendVoucherL2StatusDto,
    MnemonicLanguage, TransactionRecord,
};
use crate::services::l2_anchor_service;
use crate::settings::{NominatedLockServerMeta, StoredAnchorTemplateMeta};
use crate::state::{AppState, DidWotSigner, RequiredAccess};
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
    let mut service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
    let lc = load_lifecycle_state(&mut service, password.as_deref())?;
    build_status_dto(&mut service, &lc, password.as_deref())
}

#[tauri::command]
pub async fn run_l2_lifecycle_tick(
    force: bool,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendL2LifecycleStatusDto, FrontendError> {
    let (session, resolved_pw) =
        state.authorize_and_begin_session_secret(RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let client = sync_client::create_sync_probe_client();
    let now_secs = sync_client::current_unix_timestamp();
    run_lifecycle_tick_with_now(
        state.inner(),
        &session,
        &client,
        now_secs,
        force,
        pw,
    )
    .await
}

#[tauri::command]
pub async fn discover_anchor_manifest_cmd(
    anchor_id_hex: String,
    creator_did: Option<String>,
    guarantor_dids: Vec<String>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<Option<FrontendAnchorDiscoveryDto>, FrontendError> {
    let session = state.authorize_and_begin_session(RequiredAccess::Unlocked, None)?;
    let anchor_id = decode_hex_fixed::<32>(anchor_id_hex.trim()).map_err(lifecycle_err)?;
    let client = sync_client::create_sync_probe_client();
    let found = discover_anchor_manifest(
        state.inner(),
        &session,
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
    register_guarantor_anchor_role_impl(
        anchor_id_hex,
        guarantor_index,
        context_nonce_hex,
        password,
        &state,
    )
}

pub(crate) fn register_guarantor_anchor_role_impl(
    anchor_id_hex: String,
    guarantor_index: u8,
    context_nonce_hex: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<String, FrontendError> {
    let mut service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;

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
        .with_authorized_identity(human_money_core::AccessMode::Full, password.as_deref(), |_, id| {
            let sk = l2_rotation::derive_guarantor_mig_signing_key(id, &nonce_bytes)?;
            Ok::<_, human_money_core::Error>(hex::encode(sk.verifying_key().to_bytes()))
        })
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;

    let mut lc = load_lifecycle_state(&mut service, password.as_deref())?;
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
    create_key_successor_attestation_impl(
        new_mnemonic,
        language,
        passphrase,
        reason,
        new_creator_mig_pub_hex,
        password,
        &state,
    )
}

pub(crate) fn create_key_successor_attestation_impl(
    new_mnemonic: String,
    language: Option<MnemonicLanguage>,
    passphrase: Option<String>,
    reason: String,
    new_creator_mig_pub_hex: Option<String>,
    password: Option<String>,
    state: &AppState,
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

    let new_can_did = crate::state::canonicalize_did(&new_did)
        .map_err(|e| lifecycle_err(format!("Invalid new_did: {}", e)))?;

    let now_secs = sync_client::current_unix_timestamp().max(1);
    let mut service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;

    let att = service
        .with_authorized_identity(human_money_core::AccessMode::Full, password.as_deref(), |w, id| {
            let old_can_did = crate::state::canonicalize_did(w.get_user_id())
                .map_err(|e| lifecycle_err(format!("Invalid wallet user_id: {}", e)))?;
            let old_signer = DidWotSigner {
                did: old_can_did,
                signing_key: id.signing_key.clone(),
            };
            let new_signer = DidWotSigner {
                did: new_can_did,
                signing_key: new_sk.clone(),
            };
            KeySuccessorAttestation::sign_dual(
                &old_signer,
                &new_signer,
                rot_reason,
                new_mig_pub,
                now_secs,
            )
            .map_err(|e| lifecycle_err(e.to_string()))
        })
        .map_err(FrontendError::from)??;

    let mut store = load_key_successor_attestations(&mut service, password.as_deref())?;
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

    let mut service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;
    let mut store = load_key_successor_attestations(&mut service, password.as_deref())?;
    if !store.contains(&att) {
        store.push(att.clone());
        save_key_successor_attestations(&mut service, &store, password.as_deref())?;
    }
    Ok(att.new_did)
}

pub(crate) async fn create_guarantor_claim_bundle_impl(
    local_instance_id: String,
    guarantor_id: String,
    amount: String,
    standard_toml: String,
    notes: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendClaimBundleDto, FrontendError> {
    let validated_guarantor = human_money_core::ValidatedUserId::parse(&guarantor_id)
        .map_err(FrontendError::from)?;
    let guarantor_id = validated_guarantor.into_string();
    let normalized_amount = amount.trim().replace(',', ".");
    let (res, intents, pow_bits, session, resolved_pw) = {
        let (mut service, resolved_pw) =
            state.authorize_and_lock_secret(RequiredAccess::DailyPay, password.as_deref())?;
        let pw = resolved_pw.as_deref().map(String::as_str);
        let session = state.session_token_for_service(&service)?;

        let preflight = crate::services::preflight::ValueOutflowPreflight {
            require_restore_reconciled: true,
            check_known_offender_dids: &[guarantor_id.as_str()],
            check_blocking_intent_local_ids: &[local_instance_id.as_str()],
            check_anchor_id_quorum_health: None,
        };
        crate::services::preflight::run_value_outflow_preflight(
            state,
            &mut service,
            pw,
            &preflight,
        )?;

        let lifecycle = state
            .get_or_load_internal_lifecycle(&mut service, pw)
            .map_err(FrontendError::from)?;

        let pow_bits = service
            .with_wallet(|w| {
                w.get_voucher_instance(&local_instance_id)
                    .and_then(|inst| inst.voucher.layer2_anchor)
                    .and_then(|aid| w.profile.l2_anchor_manifests.get(&hex::encode(aid)))
                    .map(|m| {
                        crate::services::common::max_pow_base_bits_for_servers(
                            &lifecycle,
                            m.servers.iter().map(|s| s.write_url.as_str()),
                        )
                    })
                    .unwrap_or(human_money_core::humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS)
            })
            .map_err(FrontendError::from)?;

        let baseline_history = state
            .load_history_from_disk(&mut service, pw)
            .map_err(FrontendError::from)?;
        let baseline_outgoing = crate::state::load_outgoing_bundles(&mut service, pw)?;
        let next_history_cell = std::cell::RefCell::new(None::<Vec<TransactionRecord>>);
        let next_outgoing_cell =
            std::cell::RefCell::new(None::<crate::models::StoredOutgoingBundles>);

        let (res, intent_opt) = service
            .create_guarantor_claim_bundle_with_companions(
                &local_instance_id,
                &guarantor_id,
                &normalized_amount,
                &standard_toml,
                None,
                notes.clone(),
                human_money_core::L2TransferMode::DurableIntent(pow_bits),
                pw,
                |(claim_res, _intent_opt), wallet| {
                    let mut summable_amounts = std::collections::HashMap::new();
                    let mut countable_items = std::collections::HashMap::new();
                    if let Some(first) = claim_res.involved_sources_details.first() {
                        summable_amounts
                            .insert(first.display_currency.clone(), first.amount.clone());
                    } else {
                        countable_items.insert("Claim".to_string(), 1);
                    }

                    let record = TransactionRecord {
                        id: Uuid::new_v4().to_string(),
                        direction: "sent".to_string(),
                        recipient_id: guarantor_id.clone(),
                        sender_id: wallet.get_user_id().to_string(),
                        timestamp: Utc::now().to_rfc3339(),
                        summable_amounts,
                        countable_items,
                        involved_vouchers: vec![local_instance_id.clone()],
                        involved_sources_details: Some(
                            claim_res
                                .involved_sources_details
                                .iter()
                                .cloned()
                                .map(Into::into)
                                .collect(),
                        ),
                        bundle_data: Vec::new(),
                        bundle_id: claim_res.bundle_id.clone(),
                        notes: notes.clone(),
                        sender_profile_name: None,
                    };
                    let batch = crate::state::prepare_outgoing_history_and_bundle_companions(
                        &baseline_history,
                        &baseline_outgoing,
                        vec![(record, claim_res.bundle_bytes.clone())],
                    )
                    .map_err(|e| {
                        human_money_core::Error::Storage(
                            human_money_core::StorageError::InvalidFormat(e.message),
                        )
                    })?;
                    *next_history_cell.borrow_mut() = Some(batch.history);
                    *next_outgoing_cell.borrow_mut() = Some(batch.outgoing);
                    Ok(batch.blobs)
                },
            )
            .map_err(FrontendError::from)?;

        if let Some(next_hist) = next_history_cell.into_inner() {
            state.write_cache_slot_for_service(&service, |c| &mut c.history, Some(next_hist));
        }
        if let Some(next_out) = next_outgoing_cell.into_inner() {
            state.write_cache_slot_for_service(
                &service,
                |c| &mut c.outgoing_bundles,
                Some(next_out),
            );
        }
        if let Err(e) = state.refresh_events_cache(&mut service, pw) {
            log::warn!(
                "Guarantor claim committed; event cache refresh failed: {}",
                e
            );
        }

        let intents = intent_opt.into_iter().collect::<Vec<_>>();
        (res, intents, pow_bits, session, resolved_pw)
    };

    if !intents.is_empty() {
        let _ = crate::services::preflight::dispatch_l2_send_intents(
            state,
            &session,
            &intents,
            pow_bits,
            resolved_pw.as_deref().map(String::as_str),
        )
        .await?;
    }

    Ok(FrontendClaimBundleDto {
        bundle_data: res.bundle_bytes,
        bundle_id: res.bundle_id,
        guarantor_id,
        claimed_amount: normalized_amount,
    })
}

#[tauri::command]
pub async fn create_guarantor_claim_bundle(
    local_instance_id: String,
    guarantor_id: String,
    amount: String,
    standard_toml: String,
    notes: Option<String>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendClaimBundleDto, FrontendError> {
    create_guarantor_claim_bundle_impl(
        local_instance_id,
        guarantor_id,
        amount,
        standard_toml,
        notes,
        password,
        &state,
    )
    .await
}

#[allow(unused_variables)]
#[tauri::command]
pub fn create_dead_quorum_swap_offer_cmd(
    dead_anchor_id_hex: String,
    replacement_anchor_id_hex: String,
    claim_window_secs: Option<u64>,
    pow_bits: Option<u32>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    create_dead_quorum_swap_offer_impl(
        dead_anchor_id_hex,
        replacement_anchor_id_hex,
        claim_window_secs,
        pow_bits,
        password,
        &state,
    )
}

pub(crate) fn create_dead_quorum_swap_offer_impl(
    dead_anchor_id_hex: String,
    replacement_anchor_id_hex: String,
    claim_window_secs: Option<u64>,
    pow_bits: Option<u32>,
    password: Option<String>,
    state: &AppState,
) -> Result<String, FrontendError> {
    let dead_id = decode_hex_fixed::<32>(dead_anchor_id_hex.trim()).map_err(lifecycle_err)?;
    let repl_id =
        decode_hex_fixed::<32>(replacement_anchor_id_hex.trim()).map_err(lifecycle_err)?;
    let window = claim_window_secs
        .unwrap_or(DEAD_QUORUM_CLAIM_WINDOW_SECS)
        .max(DEAD_QUORUM_CLAIM_WINDOW_SECS);
    let bits = pow_bits.unwrap_or(10);
    let now_secs = sync_client::current_unix_timestamp().max(1);

    let service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;

    let offer = service
        .with_authorized_identity(human_money_core::AccessMode::Full, password.as_deref(), |_, id| {
            DeadQuorumSwapOffer::create_and_sign(id, dead_id, repl_id, now_secs, window, bits)
        })
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;

    serde_json::to_string_pretty(&offer).map_err(|e| lifecycle_err(e.to_string()))
}

#[tauri::command]
pub async fn create_dead_quorum_swap_claim_cmd(
    offer_json: String,
    local_instance_id: String,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<String, FrontendError> {
    let offer: DeadQuorumSwapOffer = serde_json::from_str(offer_json.trim())
        .map_err(|e| lifecycle_err(format!("Invalid DeadQuorumSwapOffer JSON: {}", e)))?;
    let offer_digest = offer.verify().map_err(FrontendError::from)?;

    let (voucher, claimant_did, qcs): (Voucher, String, Vec<QuorumCertificate>) = {
        let (service, _resolved_pw) = state
            .authorize_and_lock_secret(RequiredAccess::DailyPay, password.as_deref())?;

        service
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
            .map_err(FrontendError::from)?
    };

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
    let voucher_id_for_pow = voucher.voucher_id.clone();
    let claimant_for_pow = claimant_did.clone();
    let pow_bits = offer.swap_ticket_pow_bits;

    let pow_nonce = tokio::task::spawn_blocking(move || {
        solve_swap_ticket_pow(
            &offer_digest,
            &voucher_id_for_pow,
            &tip_t_id,
            &claimant_for_pow,
            pow_bits,
            0,
            5_000_000,
        )
    })
    .await
    .map_err(|e| lifecycle_err(format!("Swap PoW task failed: {}", e)))?
    .map_err(FrontendError::from)?;

    let submission = DeadQuorumClaimSubmission {
        voucher,
        claimant_did,
        quorum_certificates: qcs,
        swap_pow_nonce: pow_nonce,
    };
    serde_json::to_string_pretty(&submission).map_err(|e| lifecycle_err(e.to_string()))
}

fn swap_status_to_str(status: human_money_core::DeadQuorumSwapStatus) -> String {
    match status {
        human_money_core::DeadQuorumSwapStatus::AwaitingGuarantorSignatures => {
            "awaiting_guarantor_signatures".to_string()
        }
        human_money_core::DeadQuorumSwapStatus::PendingL2Lock => "pending_l2_lock".to_string(),
        human_money_core::DeadQuorumSwapStatus::Completed => "completed".to_string(),
    }
}

pub(crate) async fn adjudicate_dead_quorum_swap_impl(
    offer_json: String,
    submissions_json: Vec<String>,
    standard_toml: String,
    password: Option<String>,
    state: &AppState,
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

    let (mut adj, staged_intents, pow_bits, offer_digest_hex, session) = {
        let mut service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;
        let session = state.session_token_for_service(&service)?;

        let preflight = crate::services::preflight::ValueOutflowPreflight {
            require_restore_reconciled: true,
            check_known_offender_dids: &[],
            check_blocking_intent_local_ids: &[],
            check_anchor_id_quorum_health: Some(offer.replacement_anchor_id),
        };
        crate::services::preflight::run_value_outflow_preflight(
            state,
            &mut service,
            password.as_deref(),
            &preflight,
        )?;

        let lifecycle = state
            .get_or_load_internal_lifecycle(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        let repl_anchor_hex = hex::encode(offer.replacement_anchor_id);
        let pow_bits = service
            .with_wallet(|w| {
                w.profile
                    .l2_anchor_manifests
                    .get(&repl_anchor_hex)
                    .map(|m| {
                        crate::services::common::max_pow_base_bits_for_servers(
                            &lifecycle,
                            m.servers.iter().map(|s| s.write_url.as_str()),
                        )
                    })
                    .unwrap_or(human_money_core::humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS)
            })
            .map_err(FrontendError::from)?;

        let baseline_history = state
            .load_history_from_disk(&mut service, password.as_deref())
            .map_err(FrontendError::from)?;
        let baseline_outgoing =
            crate::state::load_outgoing_bundles(&mut service, password.as_deref())?;
        let next_history_cell = std::cell::RefCell::new(None::<Vec<TransactionRecord>>);
        let next_outgoing_cell =
            std::cell::RefCell::new(None::<crate::models::StoredOutgoingBundles>);

        let (adj, staged_intents) = service
            .execute_dead_quorum_swap_with_companions(
                &offer,
                &standard_toml,
                &submissions,
                now_secs,
                human_money_core::L2TransferMode::DurableIntent(pow_bits),
                password.as_deref(),
                |(adj, _staged), wallet| {
                    let sender_id = wallet.get_user_id().to_string();
                    let mut outgoing_records = Vec::new();
                    for approved in &adj.approved {
                        if let (Some(bundle_id), Some(bundle_bytes)) = (
                            &approved.replacement_bundle_id,
                            &approved.replacement_bundle_bytes,
                        ) {
                            let is_test = submissions
                                .iter()
                                .find(|s| {
                                    s.voucher.voucher_id == approved.voucher_id
                                        && s.claimant_did == approved.claimant_did
                                })
                                .is_some_and(|s| s.voucher.non_redeemable_test_voucher);
                            let raw_unit = &std_def.immutable.blueprint.unit;
                            let display_currency = if is_test && !raw_unit.starts_with("TEST-") {
                                format!("TEST-{}", raw_unit)
                            } else {
                                raw_unit.clone()
                            };
                            let mut summable_amounts = std::collections::HashMap::new();
                            summable_amounts
                                .insert(display_currency, approved.approved_amount.clone());
                            let record = TransactionRecord {
                                id: Uuid::new_v4().to_string(),
                                direction: "sent".to_string(),
                                recipient_id: approved.claimant_did.clone(),
                                sender_id: sender_id.clone(),
                                timestamp: Utc::now().to_rfc3339(),
                                summable_amounts,
                                countable_items: std::collections::HashMap::new(),
                                involved_vouchers: approved
                                    .replacement_voucher_id
                                    .iter()
                                    .cloned()
                                    .collect(),
                                involved_sources_details: None,
                                bundle_data: Vec::new(),
                                bundle_id: bundle_id.clone(),
                                notes: Some(format!("swap:{}", approved.voucher_id)),
                                sender_profile_name: None,
                            };
                            outgoing_records.push((record, bundle_bytes.clone()));
                        }
                    }
                    if outgoing_records.is_empty() {
                        return Ok(Vec::new());
                    }
                    let batch = crate::state::prepare_outgoing_history_and_bundle_companions(
                        &baseline_history,
                        &baseline_outgoing,
                        outgoing_records,
                    )
                    .map_err(|e| {
                        human_money_core::Error::Storage(
                            human_money_core::StorageError::InvalidFormat(e.message),
                        )
                    })?;
                    *next_history_cell.borrow_mut() = Some(batch.history);
                    *next_outgoing_cell.borrow_mut() = Some(batch.outgoing);
                    Ok(batch.blobs)
                },
            )
            .map_err(FrontendError::from)?;

        if let Some(next_hist) = next_history_cell.into_inner() {
            state.write_cache_slot_for_service(&service, |c| &mut c.history, Some(next_hist));
        }
        if let Some(next_out) = next_outgoing_cell.into_inner() {
            state.write_cache_slot_for_service(
                &service,
                |c| &mut c.outgoing_bundles,
                Some(next_out),
            );
        }
        if let Err(e) = state.refresh_events_cache(&mut service, password.as_deref()) {
            log::warn!(
                "Dead-quorum swap committed; event cache refresh failed: {}",
                e
            );
        }

        let offer_digest_hex = hex::encode(offer.digest());
        (adj, staged_intents, pow_bits, offer_digest_hex, session)
    };

    if !staged_intents.is_empty() {
        let _ = crate::services::preflight::dispatch_l2_send_intents(
            state,
            &session,
            &staged_intents,
            pow_bits,
            password.as_deref(),
        )
        .await?;

        state.with_session_required(
            &session,
            "Wallet session changed during dead-quorum swap",
            |service| {
                let _ = service.with_wallet(|w| {
                    for approved in &mut adj.approved {
                        let swap_key = format!(
                            "{offer_digest_hex}:{}:{}:{}",
                            approved.voucher_id, approved.tip_t_id, approved.claimant_did
                        );
                        if let Some(rec) = w.profile.l2_dead_quorum_swaps.get(&swap_key) {
                            approved.status = rec.status;
                        }
                    }
                });
                Ok(())
            },
        )?;
    }

    let proofs_count = adj.double_spend_proofs.len();
    Ok(FrontendDeadQuorumAdjudicationDto {
        approved: adj
            .approved
            .into_iter()
            .map(|a| FrontendApprovedSwapClaimDto {
                claimant_did: a.claimant_did,
                voucher_id: a.voucher_id,
                tip_t_id: a.tip_t_id,
                approved_amount: a.approved_amount,
                replacement_voucher_id: a.replacement_voucher_id,
                replacement_local_instance_id: a.replacement_local_instance_id,
                replacement_bundle_id: a.replacement_bundle_id,
                replacement_bundle_bytes: a.replacement_bundle_bytes,
                replacement_intent_local_id: a.replacement_intent_local_id,
                status: swap_status_to_str(a.status),
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

#[tauri::command]
pub async fn adjudicate_dead_quorum_swap_cmd(
    offer_json: String,
    submissions_json: Vec<String>,
    standard_toml: String,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendDeadQuorumAdjudicationDto, FrontendError> {
    adjudicate_dead_quorum_swap_impl(offer_json, submissions_json, standard_toml, password, &state)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{InternalLifecycleState, SecurityPolicy};
    use crate::test_support::{create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE, BOB_PHRASE};
    use human_money_core::{test_utils, MnemonicLanguage, SecondaryScope};

    #[tokio::test]
    async fn test_restore_reconcile_blocks_claim_and_dead_quorum_swap() {
        let base_dir = temp_test_dir("l2_preflight_blocks_claim_and_swap");
        let pw = "TestPassword123!";
        let svc_alice = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, pw);
        let alice_state = create_test_state(svc_alice);
        let svc_bob = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, pw);
        let bob_state = create_test_state(svc_bob);

        let bob_did = bob_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let ft_toml =
            test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");

        let offer = {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .save_security_policy(
                    &mut s,
                    SecurityPolicy {
                        session_timeout_seconds: 300,
                        ..Default::default()
                    },
                    Some(pw),
                )
                .unwrap();
            alice_state
                .save_internal_lifecycle(
                    &mut s,
                    InternalLifecycleState {
                        pending_restore_l2_reconciliation: true,
                        ..Default::default()
                    },
                    Some(pw),
                )
                .unwrap();
            s.with_wallet_and_identity(|_, id| {
                DeadQuorumSwapOffer::create_and_sign(
                    id,
                    [0x11; 32],
                    [0x22; 32],
                    1_795_000_000,
                    DEAD_QUORUM_CLAIM_WINDOW_SECS,
                    4,
                )
                .unwrap()
            })
            .unwrap()
        };
        let offer_json = serde_json::to_string(&offer).unwrap();

        let claim_err = create_guarantor_claim_bundle_impl(
            "nonexistent-lid".into(),
            bob_did,
            "10.00".into(),
            ft_toml.clone(),
            None,
            Some(pw.into()),
            &alice_state,
        )
        .await
        .unwrap_err();
        assert!(
            claim_err.message.contains("Sicherheitsabgleich"),
            "unexpected claim err: {:?}",
            claim_err
        );

        let swap_err = adjudicate_dead_quorum_swap_impl(
            offer_json.clone(),
            Vec::new(),
            ft_toml.clone(),
            Some(pw.into()),
            &alice_state,
        )
        .await
        .unwrap_err();
        assert!(
            swap_err.message.contains("Sicherheitsabgleich"),
            "unexpected swap err: {:?}",
            swap_err
        );

        // DailyPay secondary session MUST be rejected by adjudicate_dead_quorum_swap_impl (Full required)
        {
            let mut s = bob_state.service.lock().unwrap();
            let folder = s.list_profiles().unwrap()[0].folder_name.clone();
            s.enable_secondary_auth(pw, "123456", SecondaryScope::DailyPay)
                .unwrap();
            s.logout();
            let mode = s
                .login_with_secondary(&folder, "123456", false, "device-drops-test".into())
                .unwrap();
            assert_eq!(mode, human_money_core::AccessMode::DailyPay);
        }
        let daily_swap_err = adjudicate_dead_quorum_swap_impl(
            offer_json,
            Vec::new(),
            ft_toml,
            Some("123456".into()),
            &bob_state,
        )
        .await
        .unwrap_err();
        assert_eq!(daily_swap_err.code, "error.auth.insufficientAccessMode");

        let _ = (MnemonicLanguage::English, std::fs::remove_dir_all(&base_dir));
    }

    #[test]
    fn test_h2_ram_signing_commands_fail_closed_when_session_locked() {
        let base_dir = std::env::temp_dir().join(format!("l2_h2_test_{}", uuid::Uuid::new_v4()));
        let pw = "StrongTestPassword123!";
        let mut bob_service = crate::test_support::setup_test_app(
            &base_dir.join("bob"),
            "Bob",
            crate::test_support::BOB_PHRASE,
            pw,
        );
        bob_service.lock_session();
        let bob_state = crate::test_support::create_test_state(bob_service);

        let anchor_a = "11".repeat(32);
        let anchor_b = "22".repeat(32);

        let reg_err =
            register_guarantor_anchor_role_impl(anchor_a.clone(), 0, None, None, &bob_state)
                .unwrap_err();
        assert_eq!(reg_err.code, "error.auth.sessionNotActive");

        let att_err = create_key_successor_attestation_impl(
            crate::test_support::ALICE_PHRASE.to_string(),
            Some(crate::models::wallet::MnemonicLanguage::English),
            None,
            "routine".to_string(),
            None,
            None,
            &bob_state,
        )
        .unwrap_err();
        assert_eq!(att_err.code, "error.auth.sessionNotActive");

        let offer_err = create_dead_quorum_swap_offer_impl(
            anchor_a.clone(),
            anchor_b.clone(),
            None,
            Some(4),
            None,
            &bob_state,
        )
        .unwrap_err();
        assert_eq!(offer_err.code, "error.auth.sessionNotActive");

        // Explicit valid master password authorizes while session stays locked.
        let offer_ok =
            create_dead_quorum_swap_offer_impl(anchor_a, anchor_b, None, Some(4), Some(pw.into()), &bob_state);
        assert!(offer_ok.is_ok(), "unexpected error: {:?}", offer_ok);

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
