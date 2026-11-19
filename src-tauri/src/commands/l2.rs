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
    let lc = load_lifecycle_state(&mut service, password.as_deref());
    build_status_dto(&mut service, &lc, password.as_deref())
}

#[tauri::command]
pub async fn run_l2_lifecycle_tick(
    force: bool,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendL2LifecycleStatusDto, FrontendError> {
    state.authorize(RequiredAccess::DailyPay, password.as_deref())?;
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
    state.authorize(RequiredAccess::Unlocked, None)?;
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
    let mut service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;

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

    let mut service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;
    let mut store = load_key_successor_attestations(&mut service, password.as_deref());
    if !store.contains(&att) {
        store.push(att.clone());
        save_key_successor_attestations(&mut service, &store, password.as_deref())?;
    }
    Ok(att.new_did)
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
    let normalized_amount = amount.trim().replace(',', ".");
    let (res, l2_requests) = {
        let mut service =
            state.authorize_and_lock(RequiredAccess::DailyPay, password.as_deref())?;

        crate::services::common::ensure_restore_l2_reconciled(
            &state,
            &mut service,
            password.as_deref(),
        )?;

        let (sender_id, source_voucher_id) = service
            .with_wallet(|w| {
                let vid = w
                    .get_voucher_instance(&local_instance_id)
                    .map(|inst| inst.voucher.voucher_id.clone());
                (w.get_user_id().to_string(), vid)
            })
            .map_err(FrontendError::from)?;

        let res = service
            .create_guarantor_claim_bundle(
                &local_instance_id,
                guarantor_id.trim(),
                &normalized_amount,
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
            involved_vouchers: vec![local_instance_id.clone()],
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

        let settings = state.get_cached_settings().unwrap_or_default();
        let l2_requests = source_voucher_id
            .and_then(|vid| {
                service
                    .with_wallet(|w| {
                        let claimed = w
                            .voucher_store
                            .vouchers
                            .values()
                            .filter(|inst| {
                                inst.voucher.voucher_id == vid
                                    && inst
                                        .voucher
                                        .transactions
                                        .last()
                                        .is_some_and(|t| t.t_type == "claim")
                            })
                            .max_by_key(|inst| inst.voucher.transactions.len())
                            .or_else(|| w.get_voucher_instance(&local_instance_id))?;
                        let aid = claimed.voucher.layer2_anchor?;
                        let anchor_hex = hex::encode(aid);
                        let manifest = w.profile.l2_anchor_manifests.get(&anchor_hex)?;
                        let genesis_tickets = w
                            .profile
                            .l2_genesis_tickets
                            .get(&anchor_hex)
                            .map(|v| v.as_slice())
                            .unwrap_or(&[]);
                        let root_tid = claimed
                            .voucher
                            .transactions
                            .first()
                            .map(|t| t.t_id.as_str())
                            .unwrap_or("");
                        let qcs = w
                            .profile
                            .l2_quorum_certificates
                            .get(root_tid)
                            .map(|v| v.as_slice())
                            .unwrap_or(&[]);
                        let root_mig_sig = w
                            .profile
                            .l2_root_mig_sigs
                            .get(root_tid)
                            .and_then(|h| decode_hex_fixed::<64>(h).ok());
                        let first_unconfirmed =
                            human_money_core::services::l2_gateway::first_unconfirmed_hop_index(
                                &claimed.voucher,
                                manifest,
                                qcs,
                            )
                            .ok()?;
                        let pow_bits = crate::services::common::max_pow_base_bits_for_servers(
                            &settings,
                            manifest.servers.iter().map(|s| s.write_url.as_str()),
                        );
                        let entries =
                            human_money_core::services::l2_gateway::build_voucher_lock_batch(
                                &claimed.voucher,
                                first_unconfirmed,
                                &w.profile.l2_causal_outputs,
                                root_mig_sig,
                                pow_bits,
                            )
                            .ok()?;
                        let mut nonce = [0u8; 32];
                        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);
                        if nonce == [0u8; 32] {
                            nonce[0] = 1;
                        }
                        human_money_core::services::l2_gateway::build_committee_batch_requests(
                            &aid,
                            manifest,
                            genesis_tickets,
                            &entries,
                            &nonce,
                            &std::collections::HashMap::new(),
                        )
                        .ok()
                    })
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();

        (res, l2_requests)
    };

    if !l2_requests.is_empty() {
        let client = sync_client::create_sync_http_client();
        let dispatch = crate::gateway_resolver::dispatch_lock_committee_batches(
            &client,
            &l2_requests,
            std::time::Duration::from_secs(4),
        )
        .await;
        if !dispatch.transport_errors.is_empty() || !dispatch.rejected_errors.is_empty() {
            log::warn!(
                "Best-effort guarantor claim L2 lock dispatch incomplete: transport_errors={}, rejected_errors={}",
                dispatch.transport_errors.len(),
                dispatch.rejected_errors.len()
            );
        }
    }

    Ok(FrontendClaimBundleDto {
        bundle_data: res.bundle_bytes,
        bundle_id: res.bundle_id,
        guarantor_id: guarantor_id.trim().to_string(),
        claimed_amount: normalized_amount,
    })
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
        .with_wallet_and_identity(|_, id| {
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
        let service = state.authorize_and_lock(RequiredAccess::DailyPay, password.as_deref())?;
        let _ = password;

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

    let mut service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;

    let sender_id = service
        .with_wallet(|w| w.get_user_id().to_string())
        .map_err(FrontendError::from)?;

    let adj = service
        .execute_dead_quorum_swap(
            &offer,
            &standard_toml,
            &submissions,
            now_secs,
            password.as_deref(),
        )
        .map_err(FrontendError::from)?;

    let proofs_count = adj.double_spend_proofs.len();
    for proof in adj.double_spend_proofs {
        let _ = service.import_proof(proof, password.as_deref());
    }

    for approved in &adj.approved {
        if let (Some(ref bundle_id), Some(ref bundle_bytes)) = (
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
            summable_amounts.insert(display_currency, approved.approved_amount.clone());
            let record = TransactionRecord {
                id: Uuid::new_v4().to_string(),
                direction: "sent".to_string(),
                recipient_id: approved.claimant_did.clone(),
                sender_id: sender_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                summable_amounts,
                countable_items: std::collections::HashMap::new(),
                involved_vouchers: approved.replacement_voucher_id.iter().cloned().collect(),
                involved_sources_details: None,
                bundle_data: bundle_bytes.clone(),
                bundle_id: bundle_id.clone(),
                notes: Some(format!("swap:{}", approved.voucher_id)),
                sender_profile_name: None,
            };
            let _ = state.append_to_history(&mut service, record, password.as_deref());
        }
    }
    let _ = state.refresh_events_cache(&mut service, password.as_deref());

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
                replacement_bundle_id: a.replacement_bundle_id,
                replacement_bundle_bytes: a.replacement_bundle_bytes,
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
