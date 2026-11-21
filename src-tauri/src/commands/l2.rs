use chrono::Utc;
use human_money_core::humoco_proto::decode_hex_fixed;
use uuid::Uuid;

use crate::l2_lifecycle::{
    discover_anchor_manifest, lifecycle_err, FrontendAnchorDiscoveryDto, FrontendClaimBundleDto,
};
use crate::models::{
    FrontendError, FrontendLockProtectionOverviewDto, FrontendVoucherL2StatusDto, TransactionRecord,
};
use crate::services::l2_anchor_service;
use crate::state::{AppState, RequiredAccess};
use crate::sync_client;

const MAX_DISCOVERY_GUARANTOR_DIDS: usize = 20;

#[tauri::command]
pub fn get_lock_protection_overview(
    standard_toml: Option<String>,
    validity_duration: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendLockProtectionOverviewDto, FrontendError> {
    l2_anchor_service::get_lock_protection_overview_impl(
        standard_toml.as_deref(),
        validity_duration.as_deref(),
        &state,
    )
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

pub(crate) async fn discover_anchor_manifest_cmd_impl(
    anchor_id_hex: String,
    creator_did: Option<String>,
    guarantor_dids: Vec<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<Option<FrontendAnchorDiscoveryDto>, FrontendError> {
    let (session, resolved_pw) =
        state.authorize_and_begin_session_secret(RequiredAccess::Unlocked, password.as_deref())?;
    if guarantor_dids.len() > MAX_DISCOVERY_GUARANTOR_DIDS {
        return Err(lifecycle_err(format!(
            "Too many guarantor DIDs for discovery (max {})",
            MAX_DISCOVERY_GUARANTOR_DIDS
        )));
    }
    let anchor_id = decode_hex_fixed::<32>(anchor_id_hex.trim()).map_err(lifecycle_err)?;
    let client = sync_client::create_sync_probe_client();
    let found = discover_anchor_manifest(
        state,
        &session,
        &client,
        &anchor_id,
        creator_did.as_deref(),
        &guarantor_dids,
        resolved_pw.as_deref().map(String::as_str),
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
pub async fn discover_anchor_manifest_cmd(
    anchor_id_hex: String,
    creator_did: Option<String>,
    guarantor_dids: Vec<String>,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<Option<FrontendAnchorDiscoveryDto>, FrontendError> {
    discover_anchor_manifest_cmd_impl(
        anchor_id_hex,
        creator_did,
        guarantor_dids,
        password,
        state.inner(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn create_guarantor_claim_bundle_impl(
    app: Option<&tauri::AppHandle>,
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
    let balances_are_summable =
        human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(
            &standard_toml,
        )
        .map(|(def, _)| def.immutable.features.balances_are_summable)
        .unwrap_or(true);
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
            preverified_anchor_manifest: None,
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
                    use crate::services::balance_service::{format_fixed_point, parse_fixed_point};
                    let mut summable_sums: std::collections::HashMap<String, i128> =
                        std::collections::HashMap::new();
                    let mut countable_items = std::collections::HashMap::new();
                    if claim_res.involved_sources_details.is_empty() {
                        countable_items.insert("Claim".to_string(), 1);
                    } else {
                        for detail in &claim_res.involved_sources_details {
                            if balances_are_summable {
                                if let Some(amt) = parse_fixed_point(&detail.amount) {
                                    *summable_sums
                                        .entry(detail.display_currency.clone())
                                        .or_default() += amt;
                                }
                            } else {
                                *countable_items
                                    .entry(detail.display_currency.clone())
                                    .or_default() += 1;
                            }
                        }
                    }
                    let summable_amounts = summable_sums
                        .into_iter()
                        .map(|(k, v)| (k, format_fixed_point(v)))
                        .collect();

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

        if let Some(archive) =
            app.and_then(|a| crate::services::common::open_wallet_archive(a, &service))
        {
            if let Ok((std_def, _)) =
                human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(
                    &standard_toml,
                )
            {
                let _ = service.with_wallet(|w| {
                    let user_id = w.get_user_id().to_string();
                    let claimed_voucher_ids: std::collections::HashSet<&str> = res
                        .involved_sources_details
                        .iter()
                        .map(|d| d.voucher_id.as_str())
                        .collect();
                    for inst in w.voucher_store.vouchers.values() {
                        if claimed_voucher_ids.contains(inst.voucher.voucher_id.as_str())
                            && inst
                                .voucher
                                .transactions
                                .last()
                                .is_some_and(|t| t.t_type == "claim")
                        {
                            if let Err(err) =
                                human_money_core::archive::FileVoucherArchive::archive_voucher(
                                    &archive,
                                    &inst.voucher,
                                    &user_id,
                                    &std_def,
                                )
                            {
                                log::warn!(
                                    "Forensic archiving failed for claimed voucher {} (best effort): {}",
                                    inst.voucher.voucher_id,
                                    err
                                );
                            }
                        }
                    }
                });
            }
        }

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
        if let Err(err) = crate::services::preflight::dispatch_l2_send_intents(
            state,
            &session,
            &intents,
            pow_bits,
            resolved_pw.as_deref().map(String::as_str),
        )
        .await
        {
            log::warn!(
                "Guarantor claim L1 committed ({}); L2 intent dispatch pending: {}",
                res.bundle_id,
                err.message
            );
        }
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
    app: tauri::AppHandle,
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
        Some(&app),
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
            None,
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

    #[tokio::test]
    async fn test_s04_f7_discover_anchor_manifest_rejects_excessive_guarantors() {
        let base_dir = temp_test_dir("s04_f7_discover_limit");
        let pw = "StrongTestPassword123!";
        let svc = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, pw);
        let state = create_test_state(svc);

        let too_many: Vec<String> = (0..21)
            .map(|i| format!("did:key:z6MkgGuarantor{:02}", i))
            .collect();
        let err = discover_anchor_manifest_cmd_impl(
            "aa".repeat(32),
            None,
            too_many,
            Some(pw.to_string()),
            &state,
        )
        .await
        .unwrap_err();
        assert!(
            err.message.contains("Too many guarantor DIDs"),
            "S04-F7: expected guarantor count cap error, got: {:?}",
            err
        );

        let _ = std::fs::remove_dir_all(&base_dir);
    }

    #[tokio::test]
    async fn test_s04_f10_guarantor_claim_non_summable_standard_records_countable_items() {
        use human_money_core::models::profile::PublicProfile;
        use human_money_core::models::voucher::{NewVoucherData, ValueDefinition};
        use human_money_core::models::voucher_standard_definition::VoucherStandardDefinition;
        use human_money_core::wallet::Wallet;

        let base_dir = temp_test_dir("s04_f10_claim_countable");
        let pw = "StrongTestPassword123!";
        let mut svc_alice = setup_test_app(&base_dir.join("alice"), "Alice", ALICE_PHRASE, pw);
        let mut svc_bob = setup_test_app(&base_dir.join("bob"), "Bob", BOB_PHRASE, pw);
        let alice_id = svc_alice
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let bob_id = svc_bob
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let base_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let (base_def, _) = VoucherStandardDefinition::from_toml(&base_toml).unwrap();
        let (non_summable_def, _) = test_utils::create_custom_standard(&base_def, |s| {
            s.immutable.features.balances_are_summable = false;
        });
        let old_sig = &base_def.signature.as_ref().unwrap().signature;
        let new_sig = &non_summable_def.signature.as_ref().unwrap().signature;
        let non_summable_toml = base_toml
            .replace(
                "balances_are_summable = true",
                "balances_are_summable = false",
            )
            .replace(old_sig, new_sig);
        let (parsed_check, _) = VoucherStandardDefinition::from_toml(&non_summable_toml).unwrap();
        assert!(!parsed_check.immutable.features.balances_are_summable);

        let voucher = svc_alice
            .create_new_voucher(
                &non_summable_toml,
                NewVoucherData {
                    creator_profile: PublicProfile {
                        id: Some(alice_id.clone()),
                        ..Default::default()
                    },
                    nominal_value: ValueDefinition {
                        amount: "100".into(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();

        // Bob signs as guarantor and Alice attaches the detached signature
        let bob_sig_bundle = svc_bob
            .create_detached_signature_response_bundle(
                &voucher,
                "guarantor",
                false,
                human_money_core::ContainerConfig::TargetDid(
                    alice_id.clone(),
                    human_money_core::PrivacyMode::TrialDecryption,
                ),
                Some(pw),
            )
            .unwrap();
        let standards = std::collections::HashMap::from([(
            parsed_check.immutable.identity.uuid.clone(),
            non_summable_toml.clone(),
        )]);
        svc_alice
            .process_and_attach_signature(&bob_sig_bundle, &standards, None, Some(pw))
            .unwrap();

        let local_id = Wallet::calculate_local_instance_id(&voucher, &alice_id).unwrap();
        let alice_state = create_test_state(svc_alice);
        let dto = create_guarantor_claim_bundle_impl(
            None,
            local_id,
            bob_id,
            "100".into(),
            non_summable_toml,
            None,
            Some(false),
            Some(pw.into()),
            &alice_state,
        )
        .await
        .unwrap();
        assert!(!dto.bundle_data.is_empty());

        let history = {
            let mut s = alice_state.service.lock().unwrap();
            alice_state
                .load_history_from_disk(&mut s, Some(pw))
                .unwrap()
        };
        let claim_rec = history
            .iter()
            .find(|r| r.bundle_id == dto.bundle_id)
            .expect("claim history record must be persisted");
        assert!(
            claim_rec.summable_amounts.is_empty(),
            "S04-F10: non-summable standard claim must NOT populate summable_amounts: {:?}",
            claim_rec.summable_amounts
        );
        assert!(
            !claim_rec.countable_items.is_empty(),
            "S04-F10: non-summable standard claim MUST populate countable_items: {:?}",
            claim_rec.countable_items
        );

        let _ = std::fs::remove_dir_all(&base_dir);
    }
}
