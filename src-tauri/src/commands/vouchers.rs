// src-tauri/src/commands/vouchers.rs
use crate::{
    models::{FrontendError, FrontendNewVoucherData},
    AppState,
};
use human_money_core::{models::secure_container::ContainerConfig, NewVoucherData};
use log::{error, info};

pub(crate) async fn create_new_voucher_impl(
    standard_toml_content: String,
    data: FrontendNewVoucherData,
    password: Option<String>,
    state: &AppState,
) -> Result<crate::models::FrontendVoucherDetails, FrontendError> {
    info!("Attempting to create a new voucher...");
    let explicit_anchor_bytes = match data
        .layer2_anchor
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(hex_str) => Some(
            human_money_core::humoco_proto::decode_hex_fixed::<32>(hex_str)
                .map_err(|e| FrontendError::from(format!("Invalid layer2_anchor hex: {}", e)))?,
        ),
        None => None,
    };

    let (mut voucher_data, session, resolved_pw, auto_anchor_deletable_ms) = {
        let (mut service, resolved_pw) = state
            .authorize_and_lock_secret(crate::state::RequiredAccess::Full, password.as_deref())?;
        let pw = resolved_pw.as_deref().map(String::as_str);
        let session = state.session_token_for_service(&service)?;
        let user_id = service
            .with_wallet(|w| w.get_user_id().to_string())
            .map_err(FrontendError::from)?;

        let preflight = crate::services::preflight::ValueOutflowPreflight {
            require_restore_reconciled: true,
            check_known_offender_dids: &[],
            check_blocking_intent_local_ids: &[],
            check_anchor_id_quorum_health: explicit_anchor_bytes,
            preverified_anchor_manifest: None,
        };
        crate::services::preflight::run_value_outflow_preflight(
            state,
            &mut service,
            pw,
            &preflight,
        )?;

        let v_data = NewVoucherData {
            nominal_value: human_money_core::models::voucher::ValueDefinition {
                amount: data.nominal_value.amount.trim().replace(',', "."),
                unit: data.nominal_value.unit,
                ..Default::default()
            },
            collateral: Some(human_money_core::models::voucher::Collateral {
                value: human_money_core::models::voucher::ValueDefinition {
                    amount: data.collateral.amount.trim().replace(',', "."),
                    unit: data.collateral.unit,
                    abbreviation: {
                        let abbr = data.collateral.abbreviation.trim();
                        (!abbr.is_empty()).then(|| abbr.to_string())
                    },
                    ..Default::default()
                },
                ..Default::default()
            }),
            creator_profile: human_money_core::models::profile::PublicProfile {
                id: Some(user_id),
                ..data.creator.into()
            },
            validity_duration: data.validity_duration,
            non_redeemable_test_voucher: data.non_redeemable_test_voucher,
            layer2_anchor: explicit_anchor_bytes,
        };

        let deletable_ms_opt = if explicit_anchor_bytes.is_none()
            && !v_data.non_redeemable_test_voucher
            && !data.explicit_offline_only
        {
            let (verified_standard, standard_hash) =
                human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(
                    &standard_toml_content,
                )
                .map_err(FrontendError::from)?;
            let preview_data = v_data.clone();
            let deletable_ms = service
                .with_wallet_and_identity(|_, identity| {
                    let dry_run = human_money_core::models::voucher::Voucher::create(
                        identity,
                        &verified_standard,
                        &standard_hash,
                        preview_data,
                    )?;
                    let deletable_str = dry_run
                        .transactions
                        .first()
                        .and_then(|t| t.deletable_at.as_deref())
                        .ok_or_else(|| {
                            human_money_core::Error::Validation(
                                human_money_core::error::ValidationError::InvalidTransaction(
                                    "Init transaction missing deletable_at".to_string(),
                                ),
                            )
                        })?;
                    human_money_core::services::utils::parse_timestamp_ms(deletable_str)
                })
                .map_err(FrontendError::from)?
                .map_err(FrontendError::from)?;
            Some(deletable_ms)
        } else {
            None
        };

        (v_data, session, resolved_pw, deletable_ms_opt)
    };

    let mut anchor_bytes = explicit_anchor_bytes;
    let mut assembled_lock_pubs: Vec<String> = Vec::new();
    let mut assembled_threshold: u8 = 0;
    let mut assembled_source: &'static str = "none";

    if let Some(deletable_ms) = auto_anchor_deletable_ms {
        if let Some(assembled) =
            crate::services::l2_anchor_service::assemble_auto_committee_for_voucher(
                state,
                &session,
                deletable_ms,
                data.force_reprobe,
                resolved_pw.as_deref().map(String::as_str),
            )
            .await?
        {
            anchor_bytes = Some(assembled.anchor_id);
            voucher_data.layer2_anchor = Some(assembled.anchor_id);
            assembled_lock_pubs = assembled.lock_pubs;
            assembled_threshold = assembled.threshold;
            assembled_source = assembled.source;
        }
    } else if data.explicit_offline_only {
        anchor_bytes = None;
        voucher_data.layer2_anchor = None;
    }

    if let Some(ref exp) = data.expected_committee {
        if explicit_anchor_bytes.is_none()
            && !data.explicit_offline_only
            && !voucher_data.non_redeemable_test_voucher
        {
            let mut exp_pks: Vec<String> = exp
                .lock_pubs
                .iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            exp_pks.sort();
            if assembled_lock_pubs != exp_pks
                || assembled_threshold != exp.threshold
                || assembled_source != exp.source
            {
                let mut err = FrontendError::with_code(
                    "error.voucher.l2CommitteeChanged",
                    format!(
                        "L2 lock committee changed before issuance (expected {} {}/{}, got {} {}/{})",
                        exp.source,
                        exp.threshold,
                        exp_pks.len(),
                        assembled_source,
                        assembled_threshold,
                        assembled_lock_pubs.len()
                    ),
                );
                err.details
                    .insert("expectedSource".into(), exp.source.clone());
                err.details
                    .insert("expectedThreshold".into(), exp.threshold.to_string());
                err.details
                    .insert("expectedCount".into(), exp_pks.len().to_string());
                err.details
                    .insert("actualSource".into(), assembled_source.to_string());
                err.details
                    .insert("actualThreshold".into(), assembled_threshold.to_string());
                err.details
                    .insert("actualCount".into(), assembled_lock_pubs.len().to_string());
                err.details
                    .insert("expectedLockPubs".into(), exp_pks.join(","));
                err.details
                    .insert("actualLockPubs".into(), assembled_lock_pubs.join(","));
                return Err(err);
            }
        }
    }

    if !voucher_data.non_redeemable_test_voucher
        && anchor_bytes.is_none()
        && !data.allow_offline_only
        && !data.explicit_offline_only
    {
        return Err(FrontendError::with_code(
            "error.voucher.l2AnchorUnavailable",
            "Online double-spend protection could not be established for this voucher.",
        ));
    }

    let pw = resolved_pw.as_deref().map(String::as_str);
    let details_res = state.with_session_required(
        &session,
        "Active wallet session changed before voucher could be created",
        |service| {
            if anchor_bytes.is_some() && explicit_anchor_bytes != anchor_bytes {
                let post_preflight = crate::services::preflight::ValueOutflowPreflight {
                    require_restore_reconciled: true,
                    check_known_offender_dids: &[],
                    check_blocking_intent_local_ids: &[],
                    check_anchor_id_quorum_health: anchor_bytes,
                    preverified_anchor_manifest: None,
                };
                crate::services::preflight::run_value_outflow_preflight(
                    state,
                    service,
                    pw,
                    &post_preflight,
                )?;
            }
            if let Some(anchor_id) = anchor_bytes {
                let anchor_hex = hex::encode(anchor_id);
                let (verified_standard, standard_hash) =
                    human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(
                        &standard_toml_content,
                    )
                    .map_err(FrontendError::from)?;
                let voucher_data_preview = voucher_data.clone();
                service
                    .with_wallet_and_identity(|wallet, identity| {
                        let manifest = wallet
                            .profile
                            .l2_anchor_manifests
                            .get(&anchor_hex)
                            .ok_or_else(|| {
                                FrontendError::from("anchor_template_not_found".to_string())
                            })?;
                        if !wallet.profile.l2_anchor_nonces.contains_key(&anchor_hex) {
                            return Err(FrontendError::from(
                                "anchor_template_missing_creator_nonce".to_string(),
                            ));
                        }
                        let dry_run = human_money_core::models::voucher::Voucher::create(
                            identity,
                            &verified_standard,
                            &standard_hash,
                            voucher_data_preview,
                        )
                        .map_err(FrontendError::from)?;
                        let deletable_str = dry_run
                            .transactions
                            .first()
                            .and_then(|t| t.deletable_at.as_deref())
                            .ok_or_else(|| {
                                FrontendError::from("Init transaction missing deletable_at".to_string())
                            })?;
                        let deletable_ms =
                            human_money_core::services::utils::parse_timestamp_ms(deletable_str)
                                .map_err(FrontendError::from)?;
                        if !human_money_core::humoco_proto::retention_covers(
                            deletable_ms,
                            manifest.retain_until_quarter,
                        ) {
                            return Err(FrontendError::from(
                                "anchor_template_retention_too_short".to_string(),
                            ));
                        }
                        Ok(())
                    })
                    .map_err(FrontendError::from)??;
            }

            let voucher = service
                .create_new_voucher(&standard_toml_content, voucher_data, pw)
                .map_err(FrontendError::from)?;

            if let Err(e) = state.refresh_events_cache(service, pw) {
                error!(
                    "Failed to refresh events cache after create_new_voucher: {}",
                    e
                );
            }

            let created_voucher_id = voucher.voucher_id.clone();
            service
                .with_wallet_and_identity(
                    |w, id| -> Result<crate::models::FrontendVoucherDetails, human_money_core::Error> {
                        let local_id = w
                            .voucher_store
                            .vouchers
                            .iter()
                            .find(|(_, inst)| inst.voucher.voucher_id == created_voucher_id)
                            .map(|(lid, _)| lid.clone())
                            .ok_or_else(|| {
                                human_money_core::Error::VoucherNotFound(created_voucher_id.clone())
                            })?;
                        crate::services::balance_service::build_enriched_voucher_details(
                            state, w, id, &local_id,
                        )
                    },
                )
                .map_err(FrontendError::from)?
                .map_err(FrontendError::from)
        },
    )?;

    Ok(details_res)
}

#[tauri::command]
pub async fn create_new_voucher(
    standard_toml_content: String,
    data: FrontendNewVoucherData,
    password: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<crate::models::FrontendVoucherDetails, FrontendError> {
    create_new_voucher_impl(standard_toml_content, data, password, &state).await
}

pub(crate) fn create_signing_request_bundle_impl(
    local_instance_id: &str,
    config: ContainerConfig,
    probe_cache_opt: Option<
        &std::collections::BTreeMap<String, crate::sync_client::FrontendLockServerProbeResult>,
    >,
    password: Option<&str>,
    service: &human_money_core::app_service::AppService,
) -> Result<Vec<u8>, FrontendError> {
    let delegations_json = probe_cache_opt.and_then(|cache| {
        service
            .with_wallet(|w| {
                w.get_voucher_details(local_instance_id)
                    .ok()
                    .and_then(|inst| inst.voucher.layer2_anchor)
                    .and_then(|aid| {
                        w.profile
                            .l2_anchor_manifests
                            .get(&hex::encode(aid))
                            .and_then(|manifest| {
                                crate::services::common::collect_verified_delegations_for_manifest(
                                    manifest, cache,
                                )
                            })
                    })
            })
            .ok()
            .flatten()
    });

    service
        .create_signing_request_bundle(
            local_instance_id,
            config,
            delegations_json.as_deref(),
            password,
        )
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn create_signing_request_bundle(
    local_instance_id: String,
    config: ContainerConfig,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    info!(
        "Creating signing request bundle for voucher {} with config {:?}",
        local_instance_id, config
    );
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let probe_cache = state
        .get_or_load_internal_lifecycle(&mut service, pw)
        .ok()
        .map(|lc| lc.l2_probe_cache);
    create_signing_request_bundle_impl(
        &local_instance_id,
        config,
        probe_cache.as_ref(),
        pw,
        &service,
    )
}

pub(crate) fn open_voucher_signing_request_impl(
    container_bytes: &[u8],
    password: Option<&str>,
    service: &human_money_core::app_service::AppService,
) -> Result<crate::models::FrontendVoucherDetails, FrontendError> {
    match service.open_voucher_signing_request(container_bytes, password) {
        Ok(bundle) => {
            let human_money_core::SigningRequestBundle {
                voucher: v,
                anchor_manifest: embedded_manifest,
                anchor_delegations_json,
            } = bundle;

            info!(
                "Signature request opened successfully. Voucher ID: {}, Creator: {}",
                v.voucher_id,
                v.creator_profile.id.as_deref().unwrap_or("N/A")
            );

            let is_test_voucher = v.non_redeemable_test_voucher;
            let raw_currency = v
                .nominal_value
                .abbreviation
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| v.nominal_value.unit.clone());
            let display_currency = if is_test_voucher && !raw_currency.starts_with("TEST-") {
                format!("TEST-{}", raw_currency)
            } else {
                raw_currency
            };
            let display_standard_name =
                if is_test_voucher && !v.voucher_standard.name.starts_with("TEST-") {
                    format!("TEST-{}", v.voucher_standard.name)
                } else {
                    v.voucher_standard.name.clone()
                };
            let current_amount = v.nominal_value.amount.clone();

            let local_manifest = v.layer2_anchor.and_then(|aid| {
                service
                    .with_wallet(|w| {
                        w.profile
                            .l2_anchor_manifests
                            .get(&hex::encode(aid))
                            .cloned()
                    })
                    .ok()
                    .flatten()
                    .filter(|m| m.verify_anchor_id(&aid).is_ok())
            });

            let resolved_manifest = match (embedded_manifest, local_manifest) {
                (Some(emb), Some(local)) => {
                    if local.revision >= emb.revision {
                        Some(local)
                    } else {
                        Some(emb)
                    }
                }
                (Some(emb), None) => Some(emb),
                (None, Some(local)) => Some(local),
                (None, None) => None,
            };
            let anchor_manifest_json = resolved_manifest
                .as_ref()
                .and_then(|m| serde_json::to_string(m).ok());

            let reduced_guarantor_protection = resolved_manifest
                .as_ref()
                .map(|m| m.has_reduced_guarantor_protection().unwrap_or(false))
                .unwrap_or(false);
            let manifest_revision = resolved_manifest.as_ref().map(|m| m.revision);

            Ok(crate::models::FrontendVoucherDetails {
                local_instance_id: format!(
                    "external-{}",
                    v.voucher_id.chars().take(8).collect::<String>()
                ),
                status: "incomplete".to_string(),
                current_amount,
                voucher: v.into(),
                display_currency,
                display_standard_name,
                is_test_voucher,
                trust: Default::default(),
                reduced_guarantor_protection,
                manifest_revision,
                anchor_manifest_json,
                anchor_delegations_json,
            })
        }
        Err(e) => Err(FrontendError::from(e)),
    }
}

#[tauri::command]
pub fn open_voucher_signing_request(
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendVoucherDetails, FrontendError> {
    info!("Opening voucher signing request bundle...");
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    open_voucher_signing_request_impl(&container_bytes, password.as_deref(), &service)
}

/// Resolves the highest verified `AnchorManifest` for `anchor_id` between an optional
/// caller-supplied JSON string and the local wallet cache. Invalid JSON fails explicitly,
/// mismatched supplied manifests are surfaced without masking, and older supplied revisions
/// never downgrade a higher verified local revision.
fn resolve_manifest_candidate(
    wallet: &human_money_core::wallet::Wallet,
    anchor_id: &[u8; 32],
    supplied_manifest_json: Option<&str>,
) -> Result<Option<human_money_core::humoco_proto::AnchorManifest>, FrontendError> {
    let anchor_hex = hex::encode(anchor_id);
    let supplied_opt = match supplied_manifest_json
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => Some(
            serde_json::from_str::<human_money_core::humoco_proto::AnchorManifest>(raw)
                .map_err(|e| FrontendError::from(format!("Invalid manifest JSON: {}", e)))?,
        ),
        None => None,
    };
    let local_opt = wallet.profile.l2_anchor_manifests.get(&anchor_hex).cloned();
    match (supplied_opt, local_opt) {
        (Some(supplied), Some(local)) => {
            if supplied.verify_anchor_id(anchor_id).is_err() {
                Ok(Some(supplied))
            } else if local.verify_anchor_id(anchor_id).is_ok()
                && local.revision >= supplied.revision
            {
                Ok(Some(local))
            } else {
                Ok(Some(supplied))
            }
        }
        (Some(supplied), None) => Ok(Some(supplied)),
        (None, Some(local)) => Ok(Some(local)),
        (None, None) => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn create_detached_signature_response_bundle_impl(
    voucher: crate::models::FrontendVoucher,
    role: String,
    include_details: bool,
    config: ContainerConfig,
    password: Option<String>,
    anchor_manifest_json: Option<String>,
    anchor_delegations_json: Option<String>,
    app_opt: Option<&tauri::AppHandle>,
    state: &AppState,
) -> Result<Vec<u8>, FrontendError> {
    info!(
        "Creating detached signature response for role {} with config {:?}",
        role, config
    );
    let (mut service, resolved_pw) =
        state.authorize_and_lock_secret(crate::state::RequiredAccess::Full, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);

    let core_voucher = human_money_core::models::voucher::Voucher::try_from(voucher)?;
    let resolved_manifest = if let Some(aid) = core_voucher.layer2_anchor {
        service
            .with_wallet(|w| resolve_manifest_candidate(w, &aid, anchor_manifest_json.as_deref()))
            .map_err(FrontendError::from)??
    } else {
        None
    };

    if role.trim().eq_ignore_ascii_case("guarantor") && core_voucher.layer2_anchor.is_some() {
        let all_standards = crate::services::common::load_all_standard_tomls(app_opt, None);
        let std_toml = all_standards
            .get(&core_voucher.voucher_standard.uuid)
            .ok_or_else(|| {
                FrontendError::with_code(
                    "error.voucher.guarantorAnchorCheckFailed",
                    "Guarantor anchor verification failed (missing standard definition for anchored voucher)",
                )
            })?;
        let check = evaluate_guarantor_anchor_check_with_service(
            state,
            &mut service,
            &core_voucher,
            std_toml,
            anchor_manifest_json.as_deref(),
            anchor_delegations_json.as_deref(),
            pw,
        )?;
        if check.required && !check.passed {
            return Err(FrontendError::with_code(
                "error.voucher.guarantorAnchorCheckFailed",
                format!(
                    "Guarantor anchor verification failed ({})",
                    check.issues.join(", ")
                ),
            ));
        }
    }

    let own_did = service
        .with_wallet(|w| w.get_user_id().to_string())
        .map_err(FrontendError::from)?;
    let mut offender_dids = vec![own_did.as_str()];
    if let Some(creator_id) = core_voucher.creator_profile.id.as_deref() {
        offender_dids.push(creator_id);
    }
    let preflight = crate::services::preflight::ValueOutflowPreflight {
        require_restore_reconciled: true,
        check_known_offender_dids: &offender_dids,
        check_blocking_intent_local_ids: &[],
        check_anchor_id_quorum_health: core_voucher.layer2_anchor,
        preverified_anchor_manifest: resolved_manifest.as_ref(),
    };
    crate::services::preflight::run_value_outflow_preflight(state, &mut service, pw, &preflight)?;

    if let (Some(aid), Some(manifest)) = (core_voucher.layer2_anchor, resolved_manifest) {
        let anchor_hex = hex::encode(aid);
        let should_cache = service
            .with_wallet(|w| {
                w.profile
                    .l2_anchor_manifests
                    .get(&anchor_hex)
                    .is_none_or(|existing| manifest.revision > existing.revision)
            })
            .map_err(FrontendError::from)?;
        if should_cache {
            service
                .update_l2_anchor_manifest(manifest, pw)
                .map_err(FrontendError::from)?;
        }
    }

    service
        .create_detached_signature_response_bundle(
            &core_voucher,
            &role,
            include_details,
            config,
            pw,
        )
        .map_err(FrontendError::from)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn create_detached_signature_response_bundle(
    app: tauri::AppHandle,
    voucher: crate::models::FrontendVoucher,
    role: String,
    include_details: bool,
    config: ContainerConfig,
    password: Option<String>,
    anchor_manifest_json: Option<String>,
    anchor_delegations_json: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    create_detached_signature_response_bundle_impl(
        voucher,
        role,
        include_details,
        config,
        password,
        anchor_manifest_json,
        anchor_delegations_json,
        Some(&app),
        &state,
    )
}

#[tauri::command]
pub fn process_and_attach_signature(
    container_bytes: Vec<u8>,
    container_password: Option<String>,
    wallet_password: Option<String>,
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    info!("Processing and attaching signature from bundle...");
    let all_standards = crate::services::common::load_all_standard_tomls(Some(&app), None);
    let (mut service, resolved_pw) = state.authorize_and_lock_secret(
        crate::state::RequiredAccess::DailyPay,
        wallet_password.as_deref(),
    )?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let voucher_id = service
        .process_and_attach_signature(
            &container_bytes,
            &all_standards,
            container_password.as_deref(),
            pw,
        )
        .map_err(FrontendError::from)?;
    if let Err(e) = state.refresh_events_cache(&mut service, pw) {
        log::warn!(
            "Failed to refresh events cache after process_and_attach_signature: {}",
            e
        );
    }
    Ok(voucher_id)
}

#[tauri::command]
pub fn remove_voucher_signature(
    local_instance_id: String,
    signature_id: String,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<(), FrontendError> {
    info!(
        "Removing signature {} from voucher {}...",
        signature_id, local_instance_id
    );
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    service
        .remove_voucher_signature(&local_instance_id, &signature_id, pw)
        .map_err(FrontendError::from)?;
    if let Err(e) = state.refresh_events_cache(&mut service, pw) {
        log::warn!(
            "Failed to refresh events cache after remove_voucher_signature: {}",
            e
        );
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendGuarantorServerTrustDto {
    pub write_url: String,
    pub admin_pub: String,
    pub lock_pub: String,
    pub trusted: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendGuarantorAnchorCheckDto {
    pub required: bool,
    pub passed: bool,
    pub has_layer2_anchor: bool,
    pub retention_ok: Option<bool>,
    pub sybil_independent_count: usize,
    pub threshold: u8,
    pub server_count: usize,
    pub untrusted_servers_count: usize,
    pub unverified_delegations_count: usize,
    pub offender_check_unverified: bool,
    pub servers: Vec<FrontendGuarantorServerTrustDto>,
    pub issues: Vec<String>,
}

pub const GUARANTOR_ISSUE_MISSING_ANCHOR_MANIFEST: &str = "missing_anchor_manifest";
pub const GUARANTOR_ISSUE_MANIFEST_ANCHOR_MISMATCH: &str = "manifest_anchor_mismatch";
pub const GUARANTOR_ISSUE_RETENTION_MISMATCH: &str = "retention_does_not_cover_deletable_at";
pub const GUARANTOR_ISSUE_SYBIL_BELOW_THRESHOLD: &str = "sybil_independent_servers_below_threshold";
pub const GUARANTOR_ISSUE_BANNED_SERVER: &str = "banned_lock_server_in_manifest";

pub const GUARANTOR_ANCHOR_ISSUE_CODES: &[&str] = &[
    GUARANTOR_ISSUE_BANNED_SERVER,
    GUARANTOR_ISSUE_MANIFEST_ANCHOR_MISMATCH,
    GUARANTOR_ISSUE_MISSING_ANCHOR_MANIFEST,
    GUARANTOR_ISSUE_RETENTION_MISMATCH,
    GUARANTOR_ISSUE_SYBIL_BELOW_THRESHOLD,
];

/// Evaluates guarantor pre-signing L2 security requirements (Concept §5.3) under an
/// already-locked `AppService` to prevent redundant lock cycles and TOCTOU gaps.
pub(crate) fn evaluate_guarantor_anchor_check_with_service(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    core_voucher: &human_money_core::models::voucher::Voucher,
    standard_toml_content: &str,
    manifest_json: Option<&str>,
    anchor_delegations_json: Option<&str>,
    password: Option<&str>,
) -> Result<FrontendGuarantorAnchorCheckDto, FrontendError> {
    let (std_def, _) =
        human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(
            standard_toml_content,
        )
        .map_err(FrontendError::from)?;

    let min_sigs = std_def
        .immutable
        .issuance
        .additional_signatures_range
        .first()
        .copied()
        .unwrap_or(0);
    let allows_guarantor = std_def
        .immutable
        .issuance
        .allowed_signature_roles
        .iter()
        .any(|r| r.eq_ignore_ascii_case("guarantor"));
    let requires_check = min_sigs > 0 && allows_guarantor;

    if !requires_check {
        return Ok(FrontendGuarantorAnchorCheckDto {
            required: false,
            passed: true,
            has_layer2_anchor: core_voucher.layer2_anchor.is_some(),
            retention_ok: None,
            sybil_independent_count: 0,
            threshold: 0,
            server_count: 0,
            untrusted_servers_count: 0,
            unverified_delegations_count: 0,
            offender_check_unverified: false,
            servers: Vec::new(),
            issues: Vec::new(),
        });
    }

    let mut issues = Vec::new();
    let Some(anchor_id) = core_voucher.layer2_anchor else {
        return Ok(FrontendGuarantorAnchorCheckDto {
            required: false,
            passed: true,
            has_layer2_anchor: false,
            retention_ok: None,
            sybil_independent_count: 0,
            threshold: 0,
            server_count: 0,
            untrusted_servers_count: 0,
            unverified_delegations_count: 0,
            offender_check_unverified: false,
            servers: Vec::new(),
            issues,
        });
    };

    let manifest_opt = service
        .with_wallet(|w| resolve_manifest_candidate(w, &anchor_id, manifest_json))
        .map_err(FrontendError::from)??;

    let Some(manifest) = manifest_opt else {
        issues.push(GUARANTOR_ISSUE_MISSING_ANCHOR_MANIFEST.to_string());
        return Ok(FrontendGuarantorAnchorCheckDto {
            required: true,
            passed: false,
            has_layer2_anchor: true,
            retention_ok: None,
            sybil_independent_count: 0,
            threshold: 0,
            server_count: 0,
            untrusted_servers_count: 0,
            unverified_delegations_count: 0,
            offender_check_unverified: false,
            servers: Vec::new(),
            issues,
        });
    };

    if manifest.verify_anchor_id(&anchor_id).is_err() {
        issues.push(GUARANTOR_ISSUE_MANIFEST_ANCHOR_MISMATCH.to_string());
    }

    let retention_ok = core_voucher
        .transactions
        .first()
        .and_then(|t| t.deletable_at.as_deref())
        .and_then(|s| human_money_core::services::utils::parse_timestamp_ms(s).ok())
        .map(|del_ms| {
            human_money_core::humoco_proto::retention_covers(del_ms, manifest.retain_until_quarter)
        })
        .unwrap_or(false);
    if !retention_ok {
        issues.push(GUARANTOR_ISSUE_RETENTION_MISMATCH.to_string());
    }

    let security_policy = state
        .get_or_load_security_policy(service, password)
        .map_err(FrontendError::from)?;
    let probe_cache = state
        .get_or_load_internal_lifecycle(service, password)
        .map_err(FrontendError::from)?
        .l2_probe_cache;
    let embedded_delegations = crate::services::common::parse_and_verify_embedded_delegations(
        anchor_delegations_json,
        &manifest,
    );

    let mut unverified_delegations_count = 0usize;
    let mut servers: Vec<FrontendGuarantorServerTrustDto> =
        Vec::with_capacity(manifest.servers.len());
    let mut candidates: Vec<crate::gateway_resolver::SybilServerCandidate> =
        Vec::with_capacity(manifest.servers.len());

    for s in &manifest.servers {
        let admin_pub = hex::encode(s.admin_pub);
        let lock_pub = hex::encode(s.lock_pub);
        let verified_w = crate::services::common::resolve_locator_with_delegations(
            s,
            &probe_cache,
            &embedded_delegations,
        )
        .map(|(w, _)| w);
        let verified_domain_url = verified_w.unwrap_or_else(|| {
            unverified_delegations_count += 1;
            String::new()
        });
        let trusted = crate::settings::is_lock_admin_trusted(&security_policy, &admin_pub);
        servers.push(FrontendGuarantorServerTrustDto {
            write_url: if verified_domain_url.is_empty() {
                s.write_url.clone()
            } else {
                verified_domain_url.clone()
            },
            admin_pub,
            lock_pub,
            trusted,
        });
        candidates.push(crate::gateway_resolver::SybilServerCandidate {
            write_url: verified_domain_url,
            lock_pub: s.lock_pub,
            admin_pub: Some(s.admin_pub),
            asn: None,
        });
    }
    let untrusted_servers_count = servers.iter().filter(|s| !s.trusted).count();

    let folded = crate::gateway_resolver::sybil_fold_committee(&candidates);
    if manifest.servers.len() < 2 || folded.len() < manifest.threshold as usize {
        issues.push(GUARANTOR_ISSUE_SYBIL_BELOW_THRESHOLD.to_string());
    }

    let mut offender_check_unverified = false;
    let offenders = match state.get_or_load_server_offender_list(service, password) {
        Ok(list) => list,
        Err(err)
            if password.is_none()
                && (err.code == "error.auth.sessionNotActive"
                    || err.code == "error.auth.sessionTimeout") =>
        {
            offender_check_unverified = true;
            humoco_wot::ServerOffenderList::new()
        }
        Err(err) => return Err(err),
    };
    if manifest.servers.iter().any(|s| {
        crate::l2_lifecycle::is_server_key_revoked(&offenders, &s.lock_pub, Some(&s.admin_pub))
    }) {
        issues.push(GUARANTOR_ISSUE_BANNED_SERVER.to_string());
    }

    let passed = issues.is_empty();
    Ok(FrontendGuarantorAnchorCheckDto {
        required: true,
        passed,
        has_layer2_anchor: true,
        retention_ok: Some(retention_ok),
        sybil_independent_count: folded.len(),
        threshold: manifest.threshold,
        server_count: manifest.servers.len(),
        untrusted_servers_count,
        unverified_delegations_count,
        offender_check_unverified,
        servers,
        issues,
    })
}

/// Evaluates guarantor pre-signing L2 security requirements (Concept §5.3).
/// Active **only** for vouched voucher standards (`additional_signatures_range[0] > 0`
/// and `"guarantor"` allowed). Non-guarantor / `N=0` voucher flows stay unaffected.
pub(crate) fn evaluate_guarantor_anchor_check_impl(
    voucher: crate::models::FrontendVoucher,
    standard_toml_content: String,
    manifest_json: Option<String>,
    anchor_delegations_json: Option<String>,
    password: Option<&str>,
    state: &AppState,
) -> Result<FrontendGuarantorAnchorCheckDto, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let core_voucher = human_money_core::models::voucher::Voucher::try_from(voucher)?;
    evaluate_guarantor_anchor_check_with_service(
        state,
        &mut service,
        &core_voucher,
        &standard_toml_content,
        manifest_json.as_deref(),
        anchor_delegations_json.as_deref(),
        password,
    )
}

/// Evaluates guarantor pre-signing L2 security requirements (Concept §5.3).
/// Active **only** for vouched voucher standards (`additional_signatures_range[0] > 0`
/// and `"guarantor"` allowed). Non-guarantor / `N=0` voucher flows stay unaffected.
#[tauri::command]
pub fn evaluate_guarantor_anchor_check(
    voucher: crate::models::FrontendVoucher,
    standard_toml_content: String,
    manifest_json: Option<String>,
    anchor_delegations_json: Option<String>,
    state: tauri::State<AppState>,
) -> Result<FrontendGuarantorAnchorCheckDto, FrontendError> {
    evaluate_guarantor_anchor_check_impl(
        voucher,
        standard_toml_content,
        manifest_json,
        anchor_delegations_json,
        None,
        &state,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::fs_helpers::TempDirGuard;
    use crate::settings::{InternalLifecycleState, SecurityPolicy};
    use ed25519_dalek::SigningKey;
    use human_money_core::{
        humoco_proto::{encode_quarter, AnchorAuthority, AnchorManifest, NominatedBy, ServerEntry},
        models::secure_container::{ContainerConfig, PrivacyMode},
        test_utils::{self, ACTORS},
        wallet::Wallet,
    };

    fn load_minuto_standard_toml() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../voucher_standards/minuto_v1/standard.toml"
        ))
        .unwrap()
    }

    fn make_server_entry(seed: u8, url: &str, nominated_by: NominatedBy) -> (ServerEntry, SigningKey) {
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let pub_bytes = sk.verifying_key().to_bytes();
        let entry = ServerEntry {
            write_url: format!("{}/sync.php", url),
            read_url: format!("{}/d", url),
            admin_pub: pub_bytes,
            lock_pub: pub_bytes,
            nominated_by,
        };
        (entry, sk)
    }

    fn build_two_server_manifest(
        seeds: (u8, u8, u8, u8),
        urls: (&str, &str),
        retain_until_quarter: u16,
    ) -> (AnchorManifest, [u8; 32], SigningKey, SigningKey, SigningKey) {
        let (s1, s1_sk) = make_server_entry(seeds.0, urls.0, NominatedBy::Creator);
        let (s2, s2_sk) = make_server_entry(seeds.1, urls.1, NominatedBy::Guarantor(0));
        let creator_mig_sk = SigningKey::from_bytes(&[seeds.2; 32]);
        let guar_mig_sk = SigningKey::from_bytes(&[seeds.3; 32]);
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![s1, s2],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
                guarantor_mig_pubs: vec![guar_mig_sk.verifying_key().to_bytes()],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter,
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        (manifest, anchor_id, s1_sk, s2_sk, creator_mig_sk)
    }

    fn sign_delegation_dto(
        srv: &ServerEntry,
        sk: &SigningKey,
        issued_at: u64,
    ) -> crate::sync_client::SyncLockDelegationDto {
        let admin_hex = hex::encode(srv.admin_pub);
        let lock_hex = hex::encode(srv.lock_pub);
        let msg = humoco_market::format_lock_delegate_signature_message(
            &admin_hex,
            &lock_hex,
            &srv.write_url,
            &srv.read_url,
            issued_at,
        );
        let sig = hex::encode(ed25519_dalek::Signer::sign(sk, msg.as_bytes()).to_bytes());
        crate::sync_client::SyncLockDelegationDto {
            admin_pub: admin_hex,
            lock_pub: lock_hex,
            write_url: srv.write_url.clone(),
            read_url: srv.read_url.clone(),
            issued_at,
            sig,
        }
    }

    fn build_signed_probe_cache(
        manifest: &AnchorManifest,
        keys: &[&SigningKey],
        issued_at: u64,
    ) -> std::collections::BTreeMap<String, crate::sync_client::FrontendLockServerProbeResult> {
        let mut cache = std::collections::BTreeMap::new();
        for (srv, sk) in manifest.servers.iter().zip(keys.iter()) {
            let del = sign_delegation_dto(srv, sk, issued_at);
            cache.insert(
                del.lock_pub.clone(),
                crate::sync_client::FrontendLockServerProbeResult {
                    write_url: srv.write_url.clone(),
                    read_url: srv.read_url.clone(),
                    drop_pub: del.admin_pub.clone(),
                    admin_pub: del.admin_pub.clone(),
                    lock_pub: del.lock_pub.clone(),
                    delegation_valid: true,
                    waf_probe_passed: true,
                    selftest_passed: true,
                    key_leak_safe: true,
                    passed: true,
                    latency_ms: 5,
                    max_retention_quarter: manifest.retain_until_quarter,
                    pow_base_bits:
                        human_money_core::humoco_proto::crypto::DEFAULT_LOCK_POW_BASE_BITS,
                    allow_pow_tickets: true,
                    ticket_pow_bits: 16,
                    inode_warning: false,
                    used_inodes: 0,
                    max_inodes: 50_000,
                    probed_at_secs: issued_at,
                    delegation: Some(del),
                    error: None,
                },
            );
        }
        cache
    }

    fn build_signed_delegations_json(
        manifest: &AnchorManifest,
        keys: &[&SigningKey],
        issued_at: u64,
    ) -> String {
        let list: Vec<crate::sync_client::SyncLockDelegationDto> = manifest
            .servers
            .iter()
            .zip(keys.iter())
            .map(|(srv, sk)| sign_delegation_dto(srv, sk, issued_at))
            .collect();
        serde_json::to_string(&list).unwrap()
    }

    fn build_migrated_rev2_manifest(
        prev_manifest: &AnchorManifest,
        anchor_id: [u8; 32],
        new_servers: Vec<ServerEntry>,
        ack_keys: &[&SigningKey],
        commit_keys: &[&SigningKey],
        creator_mig_sk: &SigningKey,
        ts: u64,
    ) -> AnchorManifest {
        let prop = human_money_core::services::l2_rotation::build_creator_rotation_proposal(
            anchor_id,
            prev_manifest,
            2,
            new_servers,
            Vec::new(),
            ts,
            creator_mig_sk,
        )
        .unwrap();
        let imported_state_hash = human_money_core::humoco_proto::compute_import_union_state_hash(
            &anchor_id,
            prop.to_rev,
            &std::collections::BTreeMap::new(),
        )
        .unwrap();
        let prop_hash = prop.core_digest().unwrap();
        let import_acks: Vec<human_money_core::humoco_proto::ImportAck> = ack_keys
            .iter()
            .map(|sk| {
                let mut ack = human_money_core::humoco_proto::ImportAck {
                    anchor_id,
                    proposal_hash: prop_hash,
                    to_rev: 2,
                    imported_state_hash,
                    retain_until_quarter: prev_manifest.retain_until_quarter,
                    lock_pub: sk.verifying_key().to_bytes(),
                    acked_at: ts,
                    sig: [0u8; 64],
                };
                ack.sig = ed25519_dalek::Signer::sign(*sk, &ack.digest()).to_bytes();
                ack
            })
            .collect();
        let commit_d = human_money_core::humoco_proto::MigrationCert::commit_digest(
            &anchor_id,
            &prop_hash,
            2,
            &imported_state_hash,
        );
        let commit_sigs: Vec<_> = commit_keys
            .iter()
            .map(|sk| human_money_core::humoco_proto::CommitteeSignature {
                lock_pub: sk.verifying_key().to_bytes(),
                committed_at: ts,
                sig: ed25519_dalek::Signer::sign(*sk, &commit_d).to_bytes(),
            })
            .collect();
        human_money_core::services::l2_rotation::assemble_migrated_manifest(
            prev_manifest,
            &anchor_id,
            prop,
            imported_state_hash,
            import_acks,
            commit_sigs,
        )
        .unwrap()
        .0
    }

    fn make_freetaler_voucher_data(
        profile: &human_money_core::models::profile::PublicProfile,
        amount: &str,
        allow_offline_only: bool,
        explicit_offline_only: bool,
    ) -> FrontendNewVoucherData {
        FrontendNewVoucherData {
            nominal_value: crate::models::NominalValueData {
                amount: amount.into(),
                unit: "FT".into(),
            },
            creator: crate::models::FrontendCreatorData::from(profile.clone()),
            validity_duration: Some("P1Y".into()),
            non_redeemable_test_voucher: false,
            collateral: crate::models::FrontendCollateralData {
                amount: amount.into(),
                unit: "FT".into(),
                abbreviation: "FT".into(),
            },
            layer2_anchor: None,
            allow_offline_only,
            expected_committee: None,
            force_reprobe: false,
            explicit_offline_only,
        }
    }

    fn setup_creator_and_guarantor(
        tag: &str,
        pw: &str,
    ) -> (
        TempDirGuard,
        human_money_core::app_service::AppService,
        human_money_core::app_service::AppService,
        String,
        String,
    ) {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir(tag));
        let (mut svc_creator, _) = test_utils::setup_service_with_profile(
            &dir_guard.0.join("c"),
            &ACTORS.alice,
            "Alice",
            pw,
        );
        svc_creator.unlock_session(pw, 600).unwrap();
        let (mut svc_guarantor, _) = test_utils::setup_service_with_profile(
            &dir_guard.0.join("g"),
            &ACTORS.bob,
            "Bob",
            pw,
        );
        svc_guarantor.unlock_session(pw, 600).unwrap();
        let creator_id = svc_creator
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let guar_id = svc_guarantor
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        (dir_guard, svc_creator, svc_guarantor, creator_id, guar_id)
    }

    #[test]
    fn test_ask_signing_request_embeds_and_validates_anchor_manifest_for_guarantor() {
        let pw = "pw-ask-test";
        let (dir_guard, mut svc_creator, svc_guarantor, creator_id, guar_id) =
            setup_creator_and_guarantor("vouchers_ask", pw);
        assert!(dir_guard.0.exists());

        let retain_q = encode_quarter(2035, 4).unwrap();
        let (manifest, anchor_id, s1_sk, s2_sk, _) = build_two_server_manifest(
            (11, 12, 13, 14),
            ("https://sync1.example", "https://sync2.example"),
            retain_q,
        );
        let creator_probe_cache =
            build_signed_probe_cache(&manifest, &[&s1_sk, &s2_sk], 1_760_000_000);
        svc_creator
            .update_l2_anchor_manifest(manifest, Some(pw))
            .unwrap();

        let minuto_toml = load_minuto_standard_toml();
        let mut v_data = test_utils::create_minuto_voucher_data(
            human_money_core::models::profile::PublicProfile {
                id: Some(creator_id.clone()),
                ..Default::default()
            },
        );
        v_data.layer2_anchor = Some(anchor_id);
        let voucher = svc_creator
            .create_new_voucher(&minuto_toml, v_data, Some(pw))
            .unwrap();
        let local_id = Wallet::calculate_local_instance_id(&voucher, &creator_id).unwrap();

        let ask_bytes = create_signing_request_bundle_impl(
            &local_id,
            ContainerConfig::TargetDid(guar_id, PrivacyMode::TrialDecryption),
            Some(&creator_probe_cache),
            Some(pw),
            &svc_creator,
        )
        .unwrap();

        let details =
            open_voucher_signing_request_impl(&ask_bytes, Some(pw), &svc_guarantor).unwrap();
        assert!(details.is_test_voucher);
        assert_eq!(details.display_currency, "TEST-Minuto");
        assert_eq!(details.display_standard_name, "TEST-Minuto");
        let embedded_manifest: AnchorManifest = serde_json::from_str(
            details
                .anchor_manifest_json
                .as_deref()
                .expect("ask bundle must embed AnchorManifest JSON"),
        )
        .unwrap();
        assert_eq!(embedded_manifest.rev1_anchor_id().unwrap(), anchor_id);
        assert_eq!(embedded_manifest.servers.len(), 2);
        assert_eq!(details.manifest_revision, Some(1));

        // Non-empty abbreviation distinct from unit must take precedence in display_currency
        let abbr_toml =
            test_utils::generate_signed_standard_toml("voucher_standards/standard_template.toml");
        let abbr_v_data = human_money_core::NewVoucherData {
            nominal_value: human_money_core::models::voucher::ValueDefinition {
                amount: "50.00".into(),
                unit: "Regional-Taler".into(),
                abbreviation: Some("RT".into()),
                ..Default::default()
            },
            collateral: Some(human_money_core::models::voucher::Collateral {
                value: human_money_core::models::voucher::ValueDefinition {
                    amount: "50.00".into(),
                    unit: "Regional-Taler".into(),
                    abbreviation: Some("RT".into()),
                    ..Default::default()
                },
                ..Default::default()
            }),
            creator_profile: human_money_core::models::profile::PublicProfile {
                id: Some(creator_id.clone()),
                ..Default::default()
            },
            validity_duration: Some("P2Y".into()),
            ..Default::default()
        };
        let abbr_v = svc_creator
            .create_new_voucher(&abbr_toml, abbr_v_data, Some(pw))
            .unwrap();
        let abbr_local_id = Wallet::calculate_local_instance_id(&abbr_v, &creator_id).unwrap();
        let abbr_ask = create_signing_request_bundle_impl(
            &abbr_local_id,
            ContainerConfig::Password("abbr-pw".into()),
            None,
            Some(pw),
            &svc_creator,
        )
        .unwrap();
        let abbr_opened =
            open_voucher_signing_request_impl(&abbr_ask, Some("abbr-pw"), &svc_guarantor).unwrap();
        assert!(!abbr_opened.is_test_voucher);
        assert_eq!(abbr_opened.display_currency, "RT");
        assert!(!abbr_opened.display_currency.starts_with("TEST-"));
        assert!(!abbr_opened.display_standard_name.starts_with("TEST-"));

        let guar_state = AppState::new(svc_guarantor);
        guar_state.set_cached_security_policy(Some(SecurityPolicy::default()));
        guar_state.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));

        let reject_err = create_detached_signature_response_bundle_impl(
            details.voucher.clone(),
            "guarantor".to_string(),
            false,
            ContainerConfig::TargetDid(creator_id, PrivacyMode::TrialDecryption),
            Some(pw.to_string()),
            None,
            None,
            None,
            &guar_state,
        )
        .unwrap_err();
        assert_eq!(reject_err.code, "error.voucher.guarantorAnchorCheckFailed");
        assert!(reject_err.message.contains(GUARANTOR_ISSUE_MISSING_ANCHOR_MANIFEST));

        let check = evaluate_guarantor_anchor_check_impl(
            details.voucher,
            minuto_toml,
            details.anchor_manifest_json,
            details.anchor_delegations_json,
            None,
            &guar_state,
        )
        .unwrap();
        assert!(
            check.passed,
            "guarantor check must pass with embedded .ask manifest, got {:?}",
            check.issues
        );
        assert_eq!(check.untrusted_servers_count, 0);
        assert_eq!(check.servers.len(), 2);
        assert_eq!(check.sybil_independent_count, 2);
    }

    #[tokio::test]
    async fn test_restore_reconciliation_blocks_voucher_issuance_and_detached_signing() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("vouchers_restore_gate"));
        let dir = &dir_guard.0;
        let pw = "TestPassword123!";
        let svc_creator = crate::test_support::setup_test_app(
            &dir.join("creator"),
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let creator_state = AppState::new(svc_creator);
        let svc_signer = crate::test_support::setup_test_app(
            &dir.join("signer"),
            "Bob",
            crate::test_support::BOB_PHRASE,
            pw,
        );
        let signer_state = AppState::new(svc_signer);

        let (creator_id, creator_profile) = {
            let mut s = creator_state.service.lock().unwrap();
            let id = s.with_wallet(|w| w.get_user_id().to_string()).unwrap();
            let p = s.with_wallet(|w| w.profile.to_public_profile()).unwrap();
            creator_state
                .save_internal_lifecycle(
                    &mut s,
                    InternalLifecycleState {
                        pending_restore_l2_reconciliation: true,
                        ..Default::default()
                    },
                    Some(pw),
                )
                .unwrap();
            (id, p)
        };

        let ft_toml =
            test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
        let minuto_toml =
            test_utils::generate_signed_standard_toml("voucher_standards/minuto_v1/standard.toml");

        let issue_err = create_new_voucher_impl(
            ft_toml,
            make_freetaler_voucher_data(&creator_profile, "50.00", false, false),
            Some(pw.into()),
            &creator_state,
        )
        .await
        .unwrap_err();
        assert!(
            issue_err.message.contains("Sicherheitsabgleich"),
            "unexpected issuance err: {:?}",
            issue_err
        );

        let minuto_voucher: crate::models::FrontendVoucher = {
            let mut s = creator_state.service.lock().unwrap();
            creator_state
                .save_internal_lifecycle(&mut s, InternalLifecycleState::default(), Some(pw))
                .unwrap();
            let v = s
                .create_new_voucher(
                    &minuto_toml,
                    human_money_core::NewVoucherData {
                        creator_profile: human_money_core::models::profile::PublicProfile {
                            id: Some(creator_id.clone()),
                            first_name: Some("Alice".into()),
                            last_name: Some("Tester".into()),
                            ..Default::default()
                        },
                        nominal_value: human_money_core::models::voucher::ValueDefinition {
                            amount: "60".into(),
                            ..Default::default()
                        },
                        validity_duration: Some("P3Y".into()),
                        ..Default::default()
                    },
                    Some(pw),
                )
                .unwrap();
            v.into()
        };

        {
            let mut s = signer_state.service.lock().unwrap();
            signer_state
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
        let sign_restore_err = create_detached_signature_response_bundle_impl(
            minuto_voucher,
            "guarantor".into(),
            false,
            ContainerConfig::TargetDid(creator_id, PrivacyMode::TrialDecryption),
            Some(pw.into()),
            None,
            None,
            None,
            &signer_state,
        )
        .unwrap_err();
        assert!(
            sign_restore_err.message.contains("Sicherheitsabgleich"),
            "unexpected sign restore err: {:?}",
            sign_restore_err
        );
    }

    #[test]
    fn test_detached_signing_requires_full_access_rejecting_dailypay_pin() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("vouchers_dailypay_gate"));
        let dir = &dir_guard.0;
        let pw = "TestPassword123!";
        let svc_creator = crate::test_support::setup_test_app(
            &dir.join("creator"),
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let creator_id = svc_creator
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let minuto_toml =
            test_utils::generate_signed_standard_toml("voucher_standards/minuto_v1/standard.toml");
        let creator_state = AppState::new(svc_creator);
        let minuto_voucher: crate::models::FrontendVoucher = {
            let mut s = creator_state.service.lock().unwrap();
            s.create_new_voucher(
                &minuto_toml,
                human_money_core::NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(creator_id.clone()),
                        first_name: Some("Alice".into()),
                        last_name: Some("Tester".into()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "60".into(),
                        ..Default::default()
                    },
                    validity_duration: Some("P3Y".into()),
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap()
            .into()
        };

        let mut svc_signer = crate::test_support::setup_test_app(
            &dir.join("signer"),
            "Bob",
            crate::test_support::BOB_PHRASE,
            pw,
        );
        let folder = svc_signer.list_profiles().unwrap()[0].folder_name.clone();
        svc_signer
            .enable_secondary_auth(pw, "123456", human_money_core::SecondaryScope::DailyPay)
            .unwrap();
        svc_signer.logout();
        let mode = svc_signer
            .login_with_secondary(&folder, "123456", false, "device-drops-test".into())
            .unwrap();
        assert_eq!(mode, human_money_core::AccessMode::DailyPay);
        let signer_state = AppState::new(svc_signer);

        let daily_sign_err = create_detached_signature_response_bundle_impl(
            minuto_voucher.clone(),
            "guarantor".into(),
            false,
            ContainerConfig::TargetDid(creator_id.clone(), PrivacyMode::TrialDecryption),
            Some("123456".into()),
            None,
            None,
            None,
            &signer_state,
        )
        .unwrap_err();
        assert_eq!(daily_sign_err.code, "error.auth.insufficientAccessMode");

        let ok_sig = create_detached_signature_response_bundle_impl(
            minuto_voucher,
            "guarantor".into(),
            false,
            ContainerConfig::TargetDid(creator_id, PrivacyMode::TrialDecryption),
            Some(pw.into()),
            None,
            None,
            None,
            &signer_state,
        )
        .expect("Full access detached sign should succeed");
        assert!(!ok_sig.is_empty());
    }

    #[tokio::test]
    async fn test_h10_create_voucher_rejects_silent_offline_fallback_unless_confirmed() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("h10_offline_guard"));
        let pw = "StrongPassword123!";
        let svc = crate::test_support::setup_test_app(
            &dir_guard.0,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let profile = svc.with_wallet(|w| w.profile.to_public_profile()).unwrap();
        let state = crate::test_support::create_test_state(svc);
        let ft_toml = human_money_core::test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );

        let err = create_new_voucher_impl(
            ft_toml.clone(),
            make_freetaler_voucher_data(&profile, "25.00", false, false),
            Some(pw.into()),
            &state,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "error.voucher.l2AnchorUnavailable");

        let mut bad_hex = make_freetaler_voucher_data(&profile, "25.00", true, false);
        bad_hex.layer2_anchor = Some("zz-not-hex".to_string());
        let hex_err = create_new_voucher_impl(ft_toml.clone(), bad_hex, Some(pw.into()), &state)
            .await
            .unwrap_err();
        assert!(hex_err.message.starts_with("Invalid layer2_anchor hex:"));

        let created = create_new_voucher_impl(
            ft_toml.clone(),
            make_freetaler_voucher_data(&profile, "25.00", true, false),
            Some(pw.into()),
            &state,
        )
        .await
        .expect("allow_offline_only confirmation must succeed");
        assert_eq!(created.voucher.layer2_anchor, None);

        let mut explicit_data = make_freetaler_voucher_data(&profile, "10.00", false, true);
        explicit_data.creator.id = None;
        let created_explicit =
            create_new_voucher_impl(ft_toml, explicit_data, Some(pw.into()), &state)
                .await
                .expect("explicit_offline_only must succeed without L2 servers");
        assert_eq!(created_explicit.voucher.layer2_anchor, None);
        assert_eq!(created_explicit.voucher.creator.id, profile.id);
    }

    #[test]
    fn test_k4_guarantor_check_rejects_n1_and_accepts_migrated_rev2_with_delegations() {
        let pw = "pw-k4-test";
        let (dir_guard, mut svc_creator, svc_guar, creator_id, guar_id) =
            setup_creator_and_guarantor("vouchers_k4_rev2", pw);
        assert!(dir_guard.0.exists());
        let guar_state = AppState::new(svc_guar);

        let (s1, s1_sk) = make_server_entry(21, "https://sync1.example", NominatedBy::Creator);
        let (s2, s2_sk) = make_server_entry(22, "https://sync2.example", NominatedBy::Creator);
        let creator_mig_sk = SigningKey::from_bytes(&[23u8; 32]);
        let retain_q = encode_quarter(2035, 4).unwrap();

        let manifest_n1 = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![s1.clone()],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: retain_q,
        };
        let anchor_id = manifest_n1.rev1_anchor_id().unwrap();
        svc_creator
            .update_l2_anchor_manifest(manifest_n1.clone(), Some(pw))
            .unwrap();

        let minuto_toml = load_minuto_standard_toml();
        let mut v_data = test_utils::create_minuto_voucher_data(
            human_money_core::models::profile::PublicProfile {
                id: Some(creator_id.clone()),
                ..Default::default()
            },
        );
        v_data.layer2_anchor = Some(anchor_id);
        let voucher = svc_creator
            .create_new_voucher(&minuto_toml, v_data, Some(pw))
            .unwrap();
        let local_id = Wallet::calculate_local_instance_id(&voucher, &creator_id).unwrap();

        let ask_n1 = create_signing_request_bundle_impl(
            &local_id,
            ContainerConfig::TargetDid(guar_id, PrivacyMode::TrialDecryption),
            None,
            Some(pw),
            &svc_creator,
        )
        .unwrap();
        let details_n1 = open_voucher_signing_request_impl(
            &ask_n1,
            Some(pw),
            &guar_state.lock_service().unwrap(),
        )
        .unwrap();
        let res_n1 = evaluate_guarantor_anchor_check_impl(
            details_n1.voucher.clone(),
            minuto_toml.clone(),
            details_n1.anchor_manifest_json,
            None,
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert!(!res_n1.passed);
        assert!(res_n1
            .issues
            .contains(&GUARANTOR_ISSUE_SYBIL_BELOW_THRESHOLD.to_string()));

        let rev2_manifest = build_migrated_rev2_manifest(
            &manifest_n1,
            anchor_id,
            vec![s1, s2],
            &[&s1_sk, &s2_sk],
            &[&s1_sk],
            &creator_mig_sk,
            1_760_000_010,
        );
        let rev2_json = serde_json::to_string(&rev2_manifest).unwrap();
        let rev2_del_json =
            build_signed_delegations_json(&rev2_manifest, &[&s1_sk, &s2_sk], 1_760_000_010);

        let res_rev2 = evaluate_guarantor_anchor_check_impl(
            details_n1.voucher.clone(),
            minuto_toml.clone(),
            Some(rev2_json.clone()),
            Some(rev2_del_json),
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert!(
            res_rev2.passed,
            "rev=2 manifest must verify against anchor_id: {:?}",
            res_rev2.issues
        );
        assert_eq!(res_rev2.unverified_delegations_count, 0);

        let res_no_del = evaluate_guarantor_anchor_check_impl(
            details_n1.voucher,
            minuto_toml,
            Some(rev2_json),
            None,
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert!(!res_no_del.passed);
        assert_eq!(res_no_del.unverified_delegations_count, 2);
        assert!(res_no_del
            .issues
            .contains(&GUARANTOR_ISSUE_SYBIL_BELOW_THRESHOLD.to_string()));
    }

    #[test]
    fn test_s06_f1_first_contact_guarantor_signing_preflight_guard_and_manifest_cache() {
        let pw = "pw-s06-f1";
        let (dir_guard, mut svc_creator, svc_guar, creator_id, guar_id) =
            setup_creator_and_guarantor("s06_f1_guarantor", pw);
        assert!(dir_guard.0.exists());
        assert!(!guar_id.is_empty());
        let guar_state = AppState::new(svc_guar);

        let (manifest_rev1, anchor_id, s1_sk, s2_sk, _) = build_two_server_manifest(
            (41, 42, 44, 45),
            ("https://sync1.example", "https://sync2.example"),
            encode_quarter(2035, 4).unwrap(),
        );
        let anchor_hex = hex::encode(anchor_id);
        svc_creator
            .update_l2_anchor_manifest(manifest_rev1.clone(), Some(pw))
            .unwrap();

        let minuto_toml = load_minuto_standard_toml();
        let mut v_data = test_utils::create_minuto_voucher_data(
            human_money_core::models::profile::PublicProfile {
                id: Some(creator_id.clone()),
                ..Default::default()
            },
        );
        v_data.layer2_anchor = Some(anchor_id);
        let voucher = svc_creator
            .create_new_voucher(&minuto_toml, v_data, Some(pw))
            .unwrap();
        let fe_voucher = crate::models::FrontendVoucher::from(voucher);
        let rev1_json = serde_json::to_string(&manifest_rev1).unwrap();
        let rev1_del_json =
            build_signed_delegations_json(&manifest_rev1, &[&s1_sk, &s2_sk], 1_760_000_010);

        // 1. Failed preflight (pending restore) MUST NOT persist supplied manifest
        {
            let mut s = guar_state.service.lock().unwrap();
            guar_state
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
        let blocked_err = create_detached_signature_response_bundle_impl(
            fe_voucher.clone(),
            "guarantor".into(),
            false,
            ContainerConfig::TargetDid(creator_id.clone(), PrivacyMode::TrialDecryption),
            Some(pw.into()),
            Some(rev1_json.clone()),
            Some(rev1_del_json.clone()),
            None,
            &guar_state,
        )
        .unwrap_err();
        assert!(
            blocked_err.message.contains("Sicherheitsabgleich"),
            "expected restore block error, got {:?}",
            blocked_err
        );
        {
            let s = guar_state.service.lock().unwrap();
            let stored = s
                .with_wallet(|w| w.profile.l2_anchor_manifests.get(&anchor_hex).cloned())
                .unwrap();
            assert_eq!(stored, None);
        }

        // 2. Invalid supplied manifest JSON MUST return explicit parse error
        {
            let mut s = guar_state.service.lock().unwrap();
            guar_state
                .save_internal_lifecycle(&mut s, InternalLifecycleState::default(), Some(pw))
                .unwrap();
        }
        let invalid_json_err = create_detached_signature_response_bundle_impl(
            fe_voucher.clone(),
            "guarantor".into(),
            false,
            ContainerConfig::TargetDid(creator_id.clone(), PrivacyMode::TrialDecryption),
            Some(pw.into()),
            Some("{not-valid-manifest-json".into()),
            Some(rev1_del_json.clone()),
            None,
            &guar_state,
        )
        .unwrap_err();
        assert!(
            invalid_json_err
                .message
                .starts_with("Invalid manifest JSON:"),
            "unexpected invalid JSON err: {:?}",
            invalid_json_err
        );

        // 3. First-contact guarantor signing with valid supplied manifest + delegations succeeds and caches rev=1
        let sig_bytes = create_detached_signature_response_bundle_impl(
            fe_voucher,
            "guarantor".into(),
            false,
            ContainerConfig::TargetDid(creator_id, PrivacyMode::TrialDecryption),
            Some(pw.into()),
            Some(rev1_json),
            Some(rev1_del_json),
            None,
            &guar_state,
        )
        .expect("S06-F1: first-contact guarantor signing must succeed");
        assert!(!sig_bytes.is_empty());
        let stored_rev = guar_state
            .service
            .lock()
            .unwrap()
            .with_wallet(|w| w.profile.l2_anchor_manifests.get(&anchor_hex).map(|m| m.revision))
            .unwrap();
        assert_eq!(stored_rev, Some(1));
    }

    #[test]
    fn test_s06_f8_higher_verified_local_manifest_prevents_downgrade_on_open_and_check() {
        let pw = "pw-s06-f8";
        let (dir_guard, mut svc_creator, svc_guar, creator_id, guar_id) =
            setup_creator_and_guarantor("s06_f8_no_downgrade", pw);
        assert!(dir_guard.0.exists());
        let guar_state = AppState::new(svc_guar);

        let (s1, s1_sk) = make_server_entry(41, "https://sync1.example", NominatedBy::Creator);
        let (s2, s2_sk) = make_server_entry(42, "https://sync2.example", NominatedBy::Creator);
        let (s3, s3_sk) = make_server_entry(43, "https://sync3.example", NominatedBy::Creator);
        let creator_mig_sk = SigningKey::from_bytes(&[44u8; 32]);
        let manifest_rev1 = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![s1, s2],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
                guarantor_mig_pubs: vec![],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: encode_quarter(2035, 4).unwrap(),
        };
        let anchor_id = manifest_rev1.rev1_anchor_id().unwrap();
        svc_creator
            .update_l2_anchor_manifest(manifest_rev1.clone(), Some(pw))
            .unwrap();

        let minuto_toml = load_minuto_standard_toml();
        let mut v_data = test_utils::create_minuto_voucher_data(
            human_money_core::models::profile::PublicProfile {
                id: Some(creator_id.clone()),
                ..Default::default()
            },
        );
        v_data.layer2_anchor = Some(anchor_id);
        let voucher = svc_creator
            .create_new_voucher(&minuto_toml, v_data, Some(pw))
            .unwrap();
        let local_id = Wallet::calculate_local_instance_id(&voucher, &creator_id).unwrap();

        let mut new_servers = manifest_rev1.servers.clone();
        new_servers.push(s3);
        let rev2_manifest = build_migrated_rev2_manifest(
            &manifest_rev1,
            anchor_id,
            new_servers,
            &[&s1_sk, &s2_sk, &s3_sk],
            &[&s1_sk, &s2_sk],
            &creator_mig_sk,
            1_760_000_020,
        );
        let rev2_json = serde_json::to_string(&rev2_manifest).unwrap();
        let rev2_del_json =
            build_signed_delegations_json(&rev2_manifest, &[&s1_sk, &s2_sk, &s3_sk], 1_760_000_010);

        // With valid lower rev=1 cached locally, supplying higher valid rev=2 must upgrade (3 servers)
        {
            let mut s = guar_state.service.lock().unwrap();
            s.update_l2_anchor_manifest(manifest_rev1.clone(), Some(pw))
                .unwrap();
        }
        let check_upgrade = evaluate_guarantor_anchor_check_impl(
            crate::models::FrontendVoucher::from(voucher.clone()),
            minuto_toml.clone(),
            Some(rev2_json),
            Some(rev2_del_json.clone()),
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert_eq!(check_upgrade.server_count, 3);
        assert!(check_upgrade.passed);

        {
            let mut s = guar_state.service.lock().unwrap();
            s.update_l2_anchor_manifest(rev2_manifest, Some(pw))
                .unwrap();
        }

        let ask_rev1 = create_signing_request_bundle_impl(
            &local_id,
            ContainerConfig::TargetDid(guar_id, PrivacyMode::TrialDecryption),
            None,
            Some(pw),
            &svc_creator,
        )
        .unwrap();
        let opened = open_voucher_signing_request_impl(
            &ask_rev1,
            Some(pw),
            &guar_state.lock_service().unwrap(),
        )
        .unwrap();
        assert_eq!(opened.manifest_revision, Some(2));
        let opened_manifest: AnchorManifest =
            serde_json::from_str(opened.anchor_manifest_json.as_deref().unwrap()).unwrap();
        assert_eq!(opened_manifest.servers.len(), 3);

        let rev1_json = serde_json::to_string(&manifest_rev1).unwrap();
        let check_no_downgrade = evaluate_guarantor_anchor_check_impl(
            crate::models::FrontendVoucher::from(voucher),
            minuto_toml,
            Some(rev1_json),
            Some(rev2_del_json),
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert_eq!(check_no_downgrade.server_count, 3);
        assert_eq!(check_no_downgrade.sybil_independent_count, 3);
        assert!(check_no_downgrade.passed);
    }

    #[test]
    fn test_s06_f10_ask_bundle_encrypts_anchor_metadata_and_ignores_outer_json() {
        let pw = "pw-s06-f10";
        let (dir_guard, mut svc_creator, svc_guarantor, creator_id, guar_id) =
            setup_creator_and_guarantor("vouchers_s06_f10", pw);
        assert!(dir_guard.0.exists());

        let (manifest, anchor_id, s1_sk, s2_sk, creator_mig_sk) = build_two_server_manifest(
            (41, 42, 43, 44),
            ("https://private-lock-a.example", "https://private-lock-b.example"),
            encode_quarter(2035, 4).unwrap(),
        );
        let creator_mig_pub_hex = hex::encode(creator_mig_sk.verifying_key().to_bytes());
        let creator_probe_cache =
            build_signed_probe_cache(&manifest, &[&s1_sk, &s2_sk], 1_760_000_000);
        svc_creator
            .update_l2_anchor_manifest(manifest, Some(pw))
            .unwrap();

        let minuto_toml = load_minuto_standard_toml();
        let mut v_data = test_utils::create_minuto_voucher_data(
            human_money_core::models::profile::PublicProfile {
                id: Some(creator_id.clone()),
                ..Default::default()
            },
        );
        v_data.layer2_anchor = Some(anchor_id);
        let voucher = svc_creator
            .create_new_voucher(&minuto_toml, v_data, Some(pw))
            .unwrap();
        let local_id = Wallet::calculate_local_instance_id(&voucher, &creator_id).unwrap();

        let forbidden_leaks = [
            "private-lock-a.example",
            "private-lock-b.example",
            "anchor_manifest_json",
            "anchor_delegations_json",
            creator_mig_pub_hex.as_str(),
        ];

        let ask_target = create_signing_request_bundle_impl(
            &local_id,
            ContainerConfig::TargetDid(guar_id, PrivacyMode::TrialDecryption),
            Some(&creator_probe_cache),
            Some(pw),
            &svc_creator,
        )
        .unwrap();
        let ask_target_text = String::from_utf8_lossy(&ask_target);
        for needle in forbidden_leaks {
            assert!(!ask_target_text.contains(needle));
        }
        let opened_target =
            open_voucher_signing_request_impl(&ask_target, None, &svc_guarantor).unwrap();
        let parsed_manifest: AnchorManifest =
            serde_json::from_str(opened_target.anchor_manifest_json.as_deref().unwrap()).unwrap();
        assert_eq!(parsed_manifest.rev1_anchor_id().unwrap(), anchor_id);
        let parsed_dels: Vec<crate::sync_client::SyncLockDelegationDto> =
            serde_json::from_str(opened_target.anchor_delegations_json.as_deref().unwrap())
                .unwrap();
        assert_eq!(parsed_dels.len(), 2);

        let ask_pw = "super-secret-ask-password";
        let ask_sym = create_signing_request_bundle_impl(
            &local_id,
            ContainerConfig::Password(ask_pw.to_string()),
            Some(&creator_probe_cache),
            Some(pw),
            &svc_creator,
        )
        .unwrap();
        let ask_sym_text = String::from_utf8_lossy(&ask_sym);
        for needle in forbidden_leaks {
            assert!(!ask_sym_text.contains(needle));
        }
        let opened_sym =
            open_voucher_signing_request_impl(&ask_sym, Some(ask_pw), &svc_guarantor).unwrap();
        assert_eq!(
            opened_sym.anchor_manifest_json,
            opened_target.anchor_manifest_json
        );
        assert_eq!(
            opened_sym.anchor_delegations_json,
            opened_target.anchor_delegations_json
        );

        let mut outer_tampered: serde_json::Value = serde_json::from_slice(&ask_target).unwrap();
        outer_tampered.as_object_mut().unwrap().insert(
            "anchor_manifest_json".to_string(),
            serde_json::Value::String("{\"bogus\":true}".to_string()),
        );
        outer_tampered.as_object_mut().unwrap().insert(
            "anchor_delegations_json".to_string(),
            serde_json::Value::String("[{\"bogus\":true}]".to_string()),
        );
        let tampered_bytes = serde_json::to_vec(&outer_tampered).unwrap();
        let opened_tampered =
            open_voucher_signing_request_impl(&tampered_bytes, None, &svc_guarantor)
                .expect("extra outer JSON keys must be ignored without breaking container decryption");
        assert_eq!(
            opened_tampered.anchor_manifest_json,
            opened_target.anchor_manifest_json
        );
        assert_eq!(
            opened_tampered.anchor_delegations_json,
            opened_target.anchor_delegations_json
        );

        let guar_state = AppState::new(svc_guarantor);
        let sig_bytes = create_detached_signature_response_bundle_impl(
            opened_target.voucher,
            "guarantor".to_string(),
            false,
            ContainerConfig::TargetDid(creator_id, PrivacyMode::TrialDecryption),
            Some(pw.to_string()),
            opened_target.anchor_manifest_json,
            opened_target.anchor_delegations_json,
            None,
            &guar_state,
        )
        .unwrap();
        assert!(!sig_bytes.is_empty());
    }

    #[test]
    fn test_resolve_manifest_candidate_and_guarantor_check_edge_invariants() {
        let pw = "pw-guarantor-edges";
        let (dir_guard, mut svc_creator, svc_guar, creator_id, guar_id) =
            setup_creator_and_guarantor("vouchers_guarantor_edges", pw);
        assert!(dir_guard.0.exists());
        let guar_state = AppState::new(svc_guar);

        let minuto_toml = load_minuto_standard_toml();
        let ft_toml =
            test_utils::generate_signed_standard_toml("voucher_standards/freetaler_v1/standard.toml");
        let (manifest, anchor_id, s1_sk, s2_sk, _) = build_two_server_manifest(
            (61, 62, 63, 64),
            ("https://edge1.example", "https://edge2.example"),
            encode_quarter(2035, 4).unwrap(),
        );
        let dels_json = build_signed_delegations_json(&manifest, &[&s1_sk, &s2_sk], 1_760_000_000);

        let mut test_v_data = test_utils::create_minuto_voucher_data(
            human_money_core::models::profile::PublicProfile {
                id: Some(creator_id.clone()),
                ..Default::default()
            },
        );
        test_v_data.layer2_anchor = Some(anchor_id);
        test_v_data.non_redeemable_test_voucher = true;
        let test_voucher = svc_creator
            .create_new_voucher(&minuto_toml, test_v_data, Some(pw))
            .unwrap();
        let local_id = Wallet::calculate_local_instance_id(&test_voucher, &creator_id).unwrap();
        let ask_test = create_signing_request_bundle_impl(
            &local_id,
            ContainerConfig::TargetDid(guar_id, PrivacyMode::TrialDecryption),
            None,
            Some(pw),
            &svc_creator,
        )
        .unwrap();
        let opened_test = open_voucher_signing_request_impl(
            &ask_test,
            None,
            &guar_state.lock_service().unwrap(),
        )
        .unwrap();
        assert!(opened_test.is_test_voucher);
        assert!(opened_test.display_currency.starts_with("TEST-"));
        assert!(!opened_test.display_currency.starts_with("TEST-TEST-"));
        assert!(opened_test.display_standard_name.starts_with("TEST-"));
        assert!(!opened_test.display_standard_name.starts_with("TEST-TEST-"));

        // Non-guarantor standard -> check not required
        let non_guar = evaluate_guarantor_anchor_check_impl(
            opened_test.voucher.clone(),
            ft_toml,
            None,
            None,
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert!(!non_guar.required);
        assert!(non_guar.passed);

        // Unanchored Minuto -> check not required
        let mut unanchored = opened_test.voucher.clone();
        unanchored.layer2_anchor = None;
        let unanch_res = evaluate_guarantor_anchor_check_impl(
            unanchored,
            minuto_toml.clone(),
            None,
            None,
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert!(!unanch_res.required);
        assert!(!unanch_res.has_layer2_anchor);
        assert!(unanch_res.passed);

        // Missing manifest -> explicit MISSING issue
        let missing_res = evaluate_guarantor_anchor_check_impl(
            opened_test.voucher.clone(),
            minuto_toml.clone(),
            None,
            None,
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert!(missing_res.required);
        assert!(!missing_res.passed);
        assert_eq!(
            missing_res.issues,
            vec![GUARANTOR_ISSUE_MISSING_ANCHOR_MANIFEST]
        );

        // Cache valid manifest locally, then supply a mismatched manifest + ban s1_sk + short retention
        {
            let mut s = guar_state.service.lock().unwrap();
            s.update_l2_anchor_manifest(manifest, Some(pw)).unwrap();
            let mut offenders = humoco_wot::ServerOffenderList::new();
            offenders
                .revoked_lock_pubs
                .insert(s1_sk.verifying_key().to_bytes());
            crate::l2_lifecycle::save_server_offender_list(&mut s, &offenders, Some(pw)).unwrap();
        }
        let (other_manifest, _, _, _, _) = build_two_server_manifest(
            (71, 72, 73, 74),
            ("https://other1.example", "https://other2.example"),
            encode_quarter(2020, 1).unwrap(),
        );
        let mismatch_res = evaluate_guarantor_anchor_check_impl(
            opened_test.voucher.clone(),
            minuto_toml.clone(),
            Some(serde_json::to_string(&other_manifest).unwrap()),
            Some(dels_json.clone()),
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert!(!mismatch_res.passed);
        assert_eq!(mismatch_res.retention_ok, Some(false));
        assert!(mismatch_res
            .issues
            .contains(&GUARANTOR_ISSUE_MANIFEST_ANCHOR_MISMATCH.to_string()));
        assert!(mismatch_res
            .issues
            .contains(&GUARANTOR_ISSUE_RETENTION_MISMATCH.to_string()));

        // Without supplied manifest override, local manifest triggers banned-server rejection
        let banned_res = evaluate_guarantor_anchor_check_impl(
            opened_test.voucher,
            minuto_toml,
            None,
            Some(dels_json),
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert!(!banned_res.passed);
        assert_eq!(banned_res.retention_ok, Some(true));
        assert_eq!(banned_res.issues, vec![GUARANTOR_ISSUE_BANNED_SERVER]);
    }

    #[tokio::test]
    async fn test_create_new_voucher_explicit_anchor_template_guards_and_committee_mismatch() {
        let dir_guard =
            TempDirGuard(crate::test_support::temp_test_dir("vouchers_explicit_anchor"));
        let pw = "StrongPassword123!";
        let svc = crate::test_support::setup_test_app(
            &dir_guard.0,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let profile = svc.with_wallet(|w| w.profile.to_public_profile()).unwrap();
        let state = crate::test_support::create_test_state(svc);
        let ft_toml = human_money_core::test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );

        let (manifest, anchor_id, s1_sk, _, _) = build_two_server_manifest(
            (81, 82, 83, 84),
            ("https://exp1.example", "https://exp2.example"),
            encode_quarter(2035, 4).unwrap(),
        );
        let anchor_hex = hex::encode(anchor_id);

        // 1. Explicit anchor without cached manifest fails in preflight
        let mut d1 = make_freetaler_voucher_data(&profile, "25.00", false, false);
        d1.layer2_anchor = Some(anchor_hex.clone());
        let missing_manifest_err =
            create_new_voucher_impl(ft_toml.clone(), d1, Some(pw.into()), &state)
                .await
                .unwrap_err();
        assert_eq!(missing_manifest_err.code, "error.l2.anchorManifestMissing");

        // 2. Cached manifest without creator nonce fails template check
        {
            let mut s = state.service.lock().unwrap();
            s.update_l2_anchor_manifest(manifest, Some(pw)).unwrap();
        }
        let mut d2 = make_freetaler_voucher_data(&profile, "25.00", false, false);
        d2.layer2_anchor = Some(anchor_hex);
        let missing_nonce_err =
            create_new_voucher_impl(ft_toml.clone(), d2, Some(pw.into()), &state)
                .await
                .unwrap_err();
        assert_eq!(
            missing_nonce_err.message,
            "anchor_template_missing_creator_nonce"
        );

        // 3. Expected committee mismatch fails before issuance
        let mut d3 = make_freetaler_voucher_data(&profile, "25.00", false, false);
        d3.expected_committee = Some(crate::models::FrontendExpectedL2Committee {
            lock_pubs: vec![hex::encode(s1_sk.verifying_key().to_bytes())],
            threshold: 1,
            source: "live".to_string(),
        });
        let committee_err = create_new_voucher_impl(ft_toml, d3, Some(pw.into()), &state)
            .await
            .unwrap_err();
        assert_eq!(committee_err.code, "error.voucher.l2CommitteeChanged");
        assert_eq!(
            committee_err.details.get("expectedSource").map(String::as_str),
            Some("live")
        );
        assert_eq!(
            committee_err.details.get("actualSource").map(String::as_str),
            Some("none")
        );
    }

    #[test]
    fn test_guarantor_anchor_check_locked_session_fallback_and_wrong_password() {
        let dir_guard = TempDirGuard(crate::test_support::temp_test_dir("vouchers_locked_check"));
        let dir = &dir_guard.0;
        let pw = "pw-locked-check";
        let (mut svc_creator, _) =
            test_utils::setup_service_with_profile(&dir.join("c"), &ACTORS.alice, "Alice", pw);
        svc_creator.unlock_session(pw, 600).unwrap();
        let creator_id = svc_creator
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let (mut svc_guar, _) =
            test_utils::setup_service_with_profile(&dir.join("g"), &ACTORS.bob, "Bob", pw);
        let guar_folder = svc_guar.list_profiles().unwrap()[0].folder_name.clone();
        svc_guar
            .enable_secondary_auth(pw, "654321", human_money_core::SecondaryScope::DailyPay)
            .unwrap();
        svc_guar.logout();
        svc_guar
            .login_with_secondary(&guar_folder, "654321", false, "test-id".into())
            .unwrap();
        let guar_state = AppState::new(svc_guar);
        guar_state.set_cached_security_policy(Some(SecurityPolicy::default()));
        guar_state.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));

        let (manifest, anchor_id, s1_sk, s2_sk, _) = build_two_server_manifest(
            (91, 92, 93, 94),
            ("https://lock1.example", "https://lock2.example"),
            encode_quarter(2035, 4).unwrap(),
        );
        let manifest_json = serde_json::to_string(&manifest).unwrap();
        let dels_json = build_signed_delegations_json(&manifest, &[&s1_sk, &s2_sk], 1_760_000_000);
        let minuto_toml = load_minuto_standard_toml();
        let mut v_data = test_utils::create_minuto_voucher_data(
            human_money_core::models::profile::PublicProfile {
                id: Some(creator_id),
                ..Default::default()
            },
        );
        v_data.layer2_anchor = Some(anchor_id);
        let voucher = svc_creator
            .create_new_voucher(&minuto_toml, v_data, Some(pw))
            .unwrap();
        let fe_voucher = crate::models::FrontendVoucher::from(voucher);

        // Case A: DailyPay PIN without cached session -> sessionNotActive -> soft fallback
        {
            let mut s = guar_state.service.lock().unwrap();
            let err_not_active = guar_state
                .get_or_load_server_offender_list(&mut s, None)
                .unwrap_err();
            assert_eq!(err_not_active.code, "error.auth.sessionNotActive");
        }
        let check_daily = evaluate_guarantor_anchor_check_impl(
            fe_voucher.clone(),
            minuto_toml.clone(),
            Some(manifest_json.clone()),
            Some(dels_json.clone()),
            None,
            &guar_state,
        )
        .unwrap();
        assert!(check_daily.passed);
        assert!(check_daily.offender_check_unverified);

        // Case B: Wrong password must NOT trigger soft fallback
        let wrong_pw_err = evaluate_guarantor_anchor_check_impl(
            fe_voucher.clone(),
            minuto_toml.clone(),
            Some(manifest_json.clone()),
            Some(dels_json.clone()),
            Some("wrong-pw"),
            &guar_state,
        )
        .unwrap_err();
        assert!(
            !wrong_pw_err.message.is_empty(),
            "wrong password must fail hard instead of setting offender_check_unverified"
        );

        // Case C: Active session expires (SessionExpired -> sessionTimeout) -> soft fallback
        {
            let mut s = guar_state.service.lock().unwrap();
            s.logout();
            s.login(&guar_folder, pw, false, "test-id".into()).unwrap();
            s.unlock_session(pw, 1).unwrap();
            s.expire_session_cache_for_test();
            let err_timeout = guar_state
                .get_or_load_server_offender_list(&mut s, None)
                .unwrap_err();
            assert_eq!(err_timeout.code, "error.auth.sessionTimeout");

            // Re-arm expired session cache directly before evaluate_guarantor_anchor_check_impl
            s.unlock_session(pw, 1).unwrap();
            s.expire_session_cache_for_test();
        }
        let check_timeout = evaluate_guarantor_anchor_check_impl(
            fe_voucher.clone(),
            minuto_toml.clone(),
            Some(manifest_json.clone()),
            Some(dels_json.clone()),
            None,
            &guar_state,
        )
        .unwrap();
        assert!(check_timeout.passed);
        assert!(check_timeout.offender_check_unverified);

        // Case D: Corrupt offender list on unlocked session (password=None) must fail closed, not fallback
        {
            let mut s = guar_state.service.lock().unwrap();
            s.unlock_session(pw, 600).unwrap();
            s.save_encrypted_data(
                crate::l2_lifecycle::SERVER_OFFENDER_LIST_KEY,
                b"{corrupt-offenders",
                Some(pw),
            )
            .unwrap();
        }
        let corrupt_err = evaluate_guarantor_anchor_check_impl(
            fe_voucher,
            minuto_toml,
            Some(manifest_json),
            Some(dels_json),
            None,
            &guar_state,
        )
        .unwrap_err();
        assert!(
            corrupt_err.message.contains("key must be a string")
                || corrupt_err.message.contains("JSON")
                || corrupt_err.message.contains("Deserialization"),
            "corrupt offender store must fail closed with parse error, got: {:?}",
            corrupt_err
        );
    }
}
