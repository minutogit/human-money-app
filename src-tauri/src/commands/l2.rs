use chrono::Utc;
use human_money_core::humoco_proto::decode_hex_fixed;
use human_money_core::services::crypto::derive_ed25519_keypair;
use humoco_wot::{KeyRotationReason, KeySuccessorAttestation};
use uuid::Uuid;

use crate::l2_lifecycle::{
    discover_anchor_manifest, lifecycle_err, load_key_successor_attestations,
    run_lifecycle_tick_with_now, save_key_successor_attestations, FrontendAnchorDiscoveryDto,
    FrontendClaimBundleDto, FrontendL2LifecycleStatusDto,
};
use crate::models::{
    FrontendError, FrontendLockProtectionOverviewDto, FrontendVoucherL2StatusDto, MnemonicLanguage,
    TransactionRecord,
};
use crate::services::l2_anchor_service;
use crate::state::{AppState, DidWotSigner, RequiredAccess};
use crate::sync_client;

#[tauri::command]
pub fn get_seal_sync_status(state: tauri::State<AppState>) -> Result<String, FrontendError> {
    l2_anchor_service::get_seal_sync_status_impl(&state)
}

#[tauri::command]
pub fn get_lock_protection_overview(
    state: tauri::State<AppState>,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    l2_anchor_service::get_lock_protection_overview_impl(&state)
}

#[tauri::command]
pub fn set_lock_server_trust(
    admin_pub: String,
    trusted: bool,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    l2_anchor_service::set_lock_server_trust_impl(admin_pub, trusted, password, &state)
}

#[tauri::command]
pub async fn refresh_lock_servers(
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    l2_anchor_service::refresh_lock_servers_impl(password, &state).await
}

#[tauri::command]
pub async fn prewarm_l2_checkout(state: tauri::State<'_, AppState>) -> Result<(), FrontendError> {
    l2_anchor_service::prewarm_l2_checkout_impl(&state).await
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
    run_lifecycle_tick_with_now(state.inner(), &session, &client, now_secs, force, pw).await
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
        .with_authorized_identity(
            human_money_core::AccessMode::Full,
            password.as_deref(),
            |w, id| {
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
            },
        )
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

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_guarantor_claim_bundle_impl(
    local_instance_id: String,
    guarantor_id: String,
    amount: String,
    standard_toml: String,
    notes: Option<String>,
    deposit_to_post: Option<bool>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendClaimBundleDto, FrontendError> {
    let validated_guarantor =
        human_money_core::ValidatedUserId::parse(&guarantor_id).map_err(FrontendError::from)?;
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
                            m.servers.iter().map(|s| s.lock_pub),
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

    let (post_deposit, post_deposit_error) = if deposit_to_post.unwrap_or(false) {
        match crate::services::post_service::deposit_transfer_to_post_with_session(
            &session,
            res.bundle_id.clone(),
            resolved_pw.as_deref().map(String::as_str),
            state,
        )
        .await
        {
            Ok(dep) => (Some(dep), None),
            Err(err) => (None, Some(err.message)),
        }
    } else {
        (None, None)
    };

    Ok(FrontendClaimBundleDto {
        bundle_data: res.bundle_bytes,
        bundle_id: res.bundle_id,
        guarantor_id,
        claimed_amount: normalized_amount,
        post_deposit,
        post_deposit_error,
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn create_guarantor_claim_bundle(
    local_instance_id: String,
    guarantor_id: String,
    amount: String,
    standard_toml: String,
    notes: Option<String>,
    deposit_to_post: Option<bool>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FrontendClaimBundleDto, FrontendError> {
    create_guarantor_claim_bundle_impl(
        local_instance_id,
        guarantor_id,
        amount,
        standard_toml,
        notes,
        deposit_to_post,
        password,
        &state,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{InternalLifecycleState, SecurityPolicy};
    use crate::test_support::{
        create_test_state, setup_test_app, temp_test_dir, ALICE_PHRASE, BOB_PHRASE,
    };
    use human_money_core::{test_utils, MnemonicLanguage, SecondaryScope};

    #[tokio::test]
    async fn test_restore_reconcile_blocks_claim_and_lock_trust_gates() {
        let base_dir = temp_test_dir("l2_preflight_blocks_claim_and_trust");
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
        let ft_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );

        {
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
        }

        let claim_err = create_guarantor_claim_bundle_impl(
            "nonexistent-lid".into(),
            bob_did,
            "10.00".into(),
            ft_toml,
            None,
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

        // DailyPay secondary session MUST be rejected by set_lock_server_trust_impl (Full required)
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
        let daily_err = l2_anchor_service::set_lock_server_trust_impl(
            "11".repeat(32),
            false,
            Some("123456".into()),
            &bob_state,
        )
        .unwrap_err();
        assert_eq!(daily_err.code, "error.auth.insufficientAccessMode");

        let _ = (
            MnemonicLanguage::English,
            std::fs::remove_dir_all(&base_dir),
        );
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

        let trust_err =
            l2_anchor_service::set_lock_server_trust_impl("11".repeat(32), false, None, &bob_state)
                .unwrap_err();
        assert_eq!(trust_err.code, "error.auth.sessionNotActive");

        // Explicit valid master password authorizes while session stays locked.
        let trust_ok = l2_anchor_service::set_lock_server_trust_impl(
            "11".repeat(32),
            false,
            Some(pw.into()),
            &bob_state,
        );
        assert!(trust_ok.is_ok(), "unexpected error: {:?}", trust_ok);

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
