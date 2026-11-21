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
                    ..Default::default()
                },
                ..Default::default()
            }),
            creator_profile: human_money_core::models::profile::PublicProfile {
                protocol_version: data.creator.protocol_version,
                id: Some(user_id),
                first_name: Some(data.creator.first_name),
                last_name: Some(data.creator.last_name),
                address: Some(human_money_core::models::voucher::Address {
                    street: data.creator.address.street,
                    house_number: data.creator.address.house_number,
                    zip_code: data.creator.address.zip_code,
                    city: data.creator.address.city,
                    country: data.creator.address.country,
                    full_address: data.creator.address.full_address,
                }),
                organization: data.creator.organization,
                community: data.creator.community,
                phone: data.creator.phone,
                email: data.creator.email,
                url: data.creator.url,
                gender: Some(data.creator.gender),
                coordinates: Some(data.creator.coordinates),
                service_offer: data.creator.service_offer,
                needs: data.creator.needs,
                ..Default::default()
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
                            human_money_core::Error::Generic(
                                "Init transaction missing deletable_at".to_string(),
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
        if !data.explicit_offline_only && !voucher_data.non_redeemable_test_voucher {
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
                        let raw_details = w.get_voucher_details(&local_id)?;
                        let mut frontend_details: crate::models::FrontendVoucherDetails =
                            raw_details.clone().into();
                        if let Some(summary) = w
                            .list_vouchers(Some(id), None, None, None)
                            .into_iter()
                            .find(|s| s.local_instance_id == local_id)
                        {
                            frontend_details.current_amount = summary.current_amount;
                        }
                        let creator_id = raw_details
                            .voucher
                            .creator_profile
                            .id
                            .as_deref()
                            .unwrap_or("");
                        frontend_details.trust =
                            state.evaluate_voucher_trust(w, creator_id, &raw_details.voucher.signatures);
                        if let Some(anchor_id) = raw_details.voucher.layer2_anchor {
                            let anchor_hex = hex::encode(anchor_id);
                            if let Some(manifest) = w.profile.l2_anchor_manifests.get(&anchor_hex) {
                                frontend_details.reduced_guarantor_protection =
                                    manifest.has_reduced_guarantor_protection().unwrap_or(false);
                                frontend_details.manifest_revision = Some(manifest.revision);
                            }
                        }
                        Ok(frontend_details)
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
    let raw = service
        .create_signing_request_bundle(local_instance_id, config, password)
        .map_err(FrontendError::from)?;

    let embedded_manifest = service
        .with_wallet(|w| {
            w.get_voucher_details(local_instance_id)
                .ok()
                .and_then(|inst| inst.voucher.layer2_anchor)
                .and_then(|aid| {
                    w.profile
                        .l2_anchor_manifests
                        .get(&hex::encode(aid))
                        .cloned()
                })
        })
        .ok()
        .flatten();

    if let Some(ref manifest) = embedded_manifest {
        if let Ok(manifest_json) = serde_json::to_string(manifest) {
            if let Ok(mut container_val) = serde_json::from_slice::<serde_json::Value>(&raw) {
                if let Some(obj) = container_val.as_object_mut() {
                    obj.insert(
                        "anchor_manifest_json".to_string(),
                        serde_json::Value::String(manifest_json),
                    );
                    if let Some(delegations_json) = probe_cache_opt.and_then(|cache| {
                        crate::services::common::collect_verified_delegations_for_manifest(
                            manifest, cache,
                        )
                    }) {
                        obj.insert(
                            "anchor_delegations_json".to_string(),
                            serde_json::Value::String(delegations_json),
                        );
                    }
                    if let Ok(enriched) = serde_json::to_vec_pretty(&container_val) {
                        return Ok(enriched);
                    }
                }
            }
        }
    }

    Ok(raw)
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
        Ok(v) => {
            info!(
                "Signature request opened successfully. Voucher ID: {}, Creator: {}",
                v.voucher_id,
                v.creator_profile.id.as_deref().unwrap_or("N/A")
            );

            let display_currency = v.nominal_value.unit.clone();
            let display_standard_name = v.voucher_standard.name.clone();
            let is_test_voucher = v.non_redeemable_test_voucher;
            let current_amount = v.nominal_value.amount.clone();

            let container_json = serde_json::from_slice::<serde_json::Value>(container_bytes).ok();
            let embedded_manifest = v.layer2_anchor.and_then(|aid| {
                container_json
                    .as_ref()
                    .and_then(|val| {
                        val.get("anchor_manifest_json")
                            .and_then(|m| m.as_str())
                            .map(str::to_string)
                    })
                    .and_then(|raw_m| {
                        serde_json::from_str::<human_money_core::humoco_proto::AnchorManifest>(
                            &raw_m,
                        )
                        .ok()
                        .filter(|m| m.verify_anchor_id(&aid).is_ok())
                        .map(|m| (raw_m, m))
                    })
            });
            let anchor_delegations_json = v.layer2_anchor.and_then(|_| {
                container_json.as_ref().and_then(|val| {
                    val.get("anchor_delegations_json")
                        .and_then(|d| d.as_str())
                        .map(str::to_string)
                })
            });

            let resolved_manifest =
                embedded_manifest
                    .as_ref()
                    .map(|(_, m)| m.clone())
                    .or_else(|| {
                        v.layer2_anchor.and_then(|aid| {
                            service
                                .with_wallet(|w| {
                                    w.profile
                                        .l2_anchor_manifests
                                        .get(&hex::encode(aid))
                                        .cloned()
                                })
                                .ok()
                                .flatten()
                        })
                    });

            let anchor_manifest_json = embedded_manifest.map(|(raw_m, _)| raw_m).or_else(|| {
                resolved_manifest
                    .as_ref()
                    .and_then(|m| serde_json::to_string(m).ok())
            });

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
    state.authorize(crate::state::RequiredAccess::Full, password.as_deref())?;

    let core_voucher_preview = human_money_core::models::voucher::Voucher::from(voucher.clone());
    if role.trim().eq_ignore_ascii_case("guarantor") && core_voucher_preview.layer2_anchor.is_some()
    {
        let all_standards = crate::services::common::load_all_standard_tomls(app_opt, None);
        let std_toml = all_standards
            .get(&core_voucher_preview.voucher_standard.uuid)
            .ok_or_else(|| {
                FrontendError::with_code(
                    "error.voucher.guarantorAnchorCheckFailed",
                    "Guarantor anchor verification failed (missing standard definition for anchored voucher)",
                )
            })?;
        let check = evaluate_guarantor_anchor_check_impl(
            voucher,
            std_toml.clone(),
            anchor_manifest_json.clone(),
            anchor_delegations_json,
            password.as_deref(),
            state,
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

    let mut service =
        state.authorize_and_lock(crate::state::RequiredAccess::Full, password.as_deref())?;

    let own_did = service
        .with_wallet(|w| w.get_user_id().to_string())
        .map_err(FrontendError::from)?;
    let mut offender_dids = vec![own_did.as_str()];
    if let Some(creator_id) = core_voucher_preview.creator_profile.id.as_deref() {
        offender_dids.push(creator_id);
    }
    let preflight = crate::services::preflight::ValueOutflowPreflight {
        require_restore_reconciled: true,
        check_known_offender_dids: &offender_dids,
        check_blocking_intent_local_ids: &[],
        check_anchor_id_quorum_health: core_voucher_preview.layer2_anchor,
    };
    crate::services::preflight::run_value_outflow_preflight(
        state,
        &mut service,
        password.as_deref(),
        &preflight,
    )?;

    if let (Some(aid), Some(raw_manifest)) = (
        core_voucher_preview.layer2_anchor,
        anchor_manifest_json.as_deref(),
    ) {
        if let Ok(manifest) = serde_json::from_str::<human_money_core::humoco_proto::AnchorManifest>(
            raw_manifest.trim(),
        ) {
            if manifest.verify_anchor_id(&aid).is_ok() {
                let anchor_hex = hex::encode(aid);
                let should_cache = service
                    .with_wallet(|w| {
                        w.profile
                            .l2_anchor_manifests
                            .get(&anchor_hex)
                            .is_none_or(|existing| manifest.revision > existing.revision)
                    })
                    .unwrap_or(false);
                if should_cache {
                    let _ = service.update_l2_anchor_manifest(manifest, password.as_deref());
                }
            }
        }
    }

    service
        .create_detached_signature_response_bundle(
            &core_voucher_preview,
            &role,
            include_details,
            config,
            password.as_deref(),
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
    service
        .process_and_attach_signature(
            &container_bytes,
            &all_standards,
            container_password.as_deref(),
            pw,
        )
        .map_err(FrontendError::from)
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
        .map_err(FrontendError::from)
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
    state.authorize(crate::state::RequiredAccess::Unlocked, None)?;
    let (std_def, _) =
        human_money_core::models::voucher_standard_definition::VoucherStandardDefinition::from_toml(
            &standard_toml_content,
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

    let core_voucher = human_money_core::models::voucher::Voucher::from(voucher);
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

    let anchor_hex = hex::encode(anchor_id);
    let supplied_manifest_json = manifest_json
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let manifest_opt: Option<human_money_core::humoco_proto::AnchorManifest> =
        if let Some(raw) = supplied_manifest_json {
            Some(
                serde_json::from_str(raw)
                    .map_err(|e| FrontendError::from(format!("Invalid manifest JSON: {}", e)))?,
            )
        } else {
            let service = state.service.lock().unwrap();
            service
                .with_wallet(|w| w.profile.l2_anchor_manifests.get(&anchor_hex).cloned())
                .map_err(FrontendError::from)?
        };

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

    let security_policy = state.get_or_load_security_policy_locked(password)?;
    let probe_cache = state
        .get_or_load_internal_lifecycle_locked(password)
        .map(|lc| lc.l2_probe_cache)
        .unwrap_or_default();
    let embedded_delegations = crate::services::common::parse_and_verify_embedded_delegations(
        anchor_delegations_json.as_deref(),
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
    let offenders = {
        let mut service = state.lock_service()?;
        match state.get_or_load_server_offender_list(&mut service, password) {
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
        }
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
    use crate::settings::{InternalLifecycleState, SecurityPolicy};
    use ed25519_dalek::SigningKey;
    use human_money_core::{
        humoco_proto::{encode_quarter, AnchorAuthority, AnchorManifest, NominatedBy, ServerEntry},
        models::secure_container::{ContainerConfig, PrivacyMode},
        test_utils::{self, ACTORS},
        wallet::Wallet,
    };
    use uuid::Uuid;

    #[test]
    fn test_ask_signing_request_embeds_and_validates_anchor_manifest_for_guarantor() {
        let dir = std::env::temp_dir().join(format!("vouchers-ask-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let pw = "pw-ask-test";

        let (mut svc_creator, _) =
            test_utils::setup_service_with_profile(&dir.join("c"), &ACTORS.alice, "Alice", pw);
        svc_creator.unlock_session(pw, 600).unwrap();
        let (svc_guarantor, _) =
            test_utils::setup_service_with_profile(&dir.join("g"), &ACTORS.bob, "Bob", pw);
        let guar_id = svc_guarantor
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let creator_id = svc_creator
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let s1_sk = SigningKey::from_bytes(&[11u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[12u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[13u8; 32]);
        let guar_mig_sk = SigningKey::from_bytes(&[14u8; 32]);
        let manifest = AnchorManifest {
            revision: 1,
            threshold: 2,
            servers: vec![
                ServerEntry {
                    write_url: "https://sync1.example/sync.php".into(),
                    read_url: "https://sync1.example/d".into(),
                    admin_pub: s1_sk.verifying_key().to_bytes(),
                    lock_pub: s1_sk.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Creator,
                },
                ServerEntry {
                    write_url: "https://sync2.example/sync.php".into(),
                    read_url: "https://sync2.example/d".into(),
                    admin_pub: s2_sk.verifying_key().to_bytes(),
                    lock_pub: s2_sk.verifying_key().to_bytes(),
                    nominated_by: NominatedBy::Guarantor(0),
                },
            ],
            successors: vec![],
            authority: AnchorAuthority {
                creator_mig_pub: creator_mig_sk.verifying_key().to_bytes(),
                guarantor_mig_pubs: vec![guar_mig_sk.verifying_key().to_bytes()],
            },
            migration_chain: vec![],
            issued_at_ms: 1_760_000_000_000,
            retain_until_quarter: encode_quarter(2035, 4).unwrap(),
        };
        let anchor_id = manifest.rev1_anchor_id().unwrap();
        let mut creator_probe_cache = std::collections::BTreeMap::new();
        for (srv, sk) in manifest.servers.iter().zip([&s1_sk, &s2_sk]) {
            let admin_hex = hex::encode(srv.admin_pub);
            let lock_hex = hex::encode(srv.lock_pub);
            let msg = humoco_market::format_lock_delegate_signature_message(
                &admin_hex,
                &lock_hex,
                &srv.write_url,
                &srv.read_url,
                1_760_000_000,
            );
            let sig = hex::encode(ed25519_dalek::Signer::sign(sk, msg.as_bytes()).to_bytes());
            creator_probe_cache.insert(
                lock_hex.clone(),
                crate::sync_client::FrontendLockServerProbeResult {
                    write_url: srv.write_url.clone(),
                    read_url: srv.read_url.clone(),
                    drop_pub: admin_hex.clone(),
                    admin_pub: admin_hex.clone(),
                    lock_pub: lock_hex.clone(),
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
                    probed_at_secs: 1_760_000_000,
                    delegation: Some(crate::sync_client::SyncLockDelegationDto {
                        admin_pub: admin_hex,
                        lock_pub: lock_hex,
                        write_url: srv.write_url.clone(),
                        read_url: srv.read_url.clone(),
                        issued_at: 1_760_000_000,
                        sig,
                    }),
                    error: None,
                },
            );
        }
        svc_creator
            .update_l2_anchor_manifest(manifest, Some(pw))
            .unwrap();

        let minuto_toml = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../voucher_standards/minuto_v1/standard.toml"
        ))
        .unwrap();
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
        assert!(
            details.anchor_manifest_json.is_some(),
            ".ask bundle should preserve creator's AnchorManifest JSON for offline guarantor checks"
        );
        assert_eq!(details.manifest_revision, Some(1));

        let guar_policy = SecurityPolicy::default();
        let guar_state = AppState::new(svc_guarantor);
        guar_state.set_cached_security_policy(Some(guar_policy));
        guar_state.set_cached_internal_lifecycle(Some(InternalLifecycleState::default()));

        // Backend signing command MUST reject guarantor signing without valid anchor verification
        let creator_id_for_cfg = creator_id.clone();
        let reject_err = create_detached_signature_response_bundle_impl(
            details.voucher.clone(),
            "guarantor".to_string(),
            false,
            ContainerConfig::TargetDid(creator_id_for_cfg, PrivacyMode::TrialDecryption),
            Some(pw.to_string()),
            None,
            None,
            None,
            &guar_state,
        )
        .unwrap_err();
        assert_eq!(reject_err.code, "error.voucher.guarantorAnchorCheckFailed");

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
            "guarantor anchor check should pass from embedded .ask manifest, got {:?}",
            check.issues
        );
        assert_eq!(check.untrusted_servers_count, 0);
        assert_eq!(check.servers.len(), 2);
        assert_eq!(check.sybil_independent_count, 2);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_restore_reconcile_and_daily_pay_access_gates_for_issuance_and_detached_signing() {
        let dir = crate::test_support::temp_test_dir("vouchers_phase2_gates");
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

        let ft_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let minuto_toml =
            test_utils::generate_signed_standard_toml("voucher_standards/minuto_v1/standard.toml");

        let new_voucher_data = FrontendNewVoucherData {
            nominal_value: crate::models::NominalValueData {
                amount: "50.00".into(),
                unit: "FT".into(),
            },
            creator: crate::models::FrontendCreatorData::from(creator_profile.clone()),
            validity_duration: Some("P1Y".into()),
            non_redeemable_test_voucher: false,
            collateral: crate::models::FrontendCollateralData {
                amount: "50.00".into(),
                unit: "FT".into(),
                abbreviation: "FT".into(),
            },
            layer2_anchor: None,
            allow_offline_only: false,
            expected_committee: None,
            force_reprobe: false,
            explicit_offline_only: false,
        };
        let issue_err = create_new_voucher_impl(
            ft_toml.clone(),
            new_voucher_data,
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

        // Clear restore flag on creator and issue offline Minuto draft voucher
        {
            let mut s = creator_state.service.lock().unwrap();
            creator_state
                .save_internal_lifecycle(&mut s, InternalLifecycleState::default(), Some(pw))
                .unwrap();
        }
        let minuto_voucher: crate::models::FrontendVoucher = {
            let mut s = creator_state.service.lock().unwrap();
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

        // Signer with pending_restore_l2_reconciliation = true MUST block detached signing
        {
            let mut s = signer_state.service.lock().unwrap();
            signer_state
                .save_security_policy(
                    &mut s,
                    SecurityPolicy {
                        session_timeout_seconds: 300,
                        ..Default::default()
                    },
                    Some(pw),
                )
                .unwrap();
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
            minuto_voucher.clone(),
            "guarantor".into(),
            false,
            ContainerConfig::TargetDid(creator_id.clone(), PrivacyMode::TrialDecryption),
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

        // Clear restore flag and log signer in with DailyPay secondary PIN -> MUST be rejected (Full required)
        {
            let mut s = signer_state.service.lock().unwrap();
            signer_state
                .save_internal_lifecycle(&mut s, InternalLifecycleState::default(), Some(pw))
                .unwrap();
            let folder = s.list_profiles().unwrap()[0].folder_name.clone();
            s.enable_secondary_auth(pw, "123456", human_money_core::SecondaryScope::DailyPay)
                .unwrap();
            s.logout();
            let mode = s
                .login_with_secondary(&folder, "123456", false, "device-drops-test".into())
                .unwrap();
            assert_eq!(mode, human_money_core::AccessMode::DailyPay);
        }
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

        // Providing master password steps up to Full and succeeds!
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
        );
        assert!(
            ok_sig.is_ok(),
            "Full access detached sign should succeed: {:?}",
            ok_sig.err()
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_h10_create_voucher_rejects_silent_offline_fallback_unless_confirmed() {
        let dir = crate::test_support::temp_test_dir("h10_offline_guard");
        let pw = "StrongPassword123!";
        let svc = crate::test_support::setup_test_app(
            &dir,
            "Alice",
            crate::test_support::ALICE_PHRASE,
            pw,
        );
        let profile = svc.with_wallet(|w| w.profile.to_public_profile()).unwrap();
        let state = crate::test_support::create_test_state(svc);
        let ft_toml = human_money_core::test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );

        let make_data = |allow_offline_only: bool| FrontendNewVoucherData {
            nominal_value: crate::models::NominalValueData {
                amount: "25.00".into(),
                unit: "FT".into(),
            },
            creator: crate::models::FrontendCreatorData::from(profile.clone()),
            validity_duration: Some("P1Y".into()),
            non_redeemable_test_voucher: false,
            collateral: crate::models::FrontendCollateralData {
                amount: "25.00".into(),
                unit: "FT".into(),
                abbreviation: "FT".into(),
            },
            layer2_anchor: None,
            allow_offline_only,
            expected_committee: None,
            force_reprobe: false,
            explicit_offline_only: false,
        };

        let err =
            create_new_voucher_impl(ft_toml.clone(), make_data(false), Some(pw.into()), &state)
                .await
                .unwrap_err();
        assert_eq!(err.code, "error.voucher.l2AnchorUnavailable");

        let created = create_new_voucher_impl(ft_toml, make_data(true), Some(pw.into()), &state)
            .await
            .expect("explicit offline-only confirmation must succeed at N=0");
        assert!(created.voucher.layer2_anchor.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn k4_evaluate_guarantor_anchor_check_enforces_n2_for_guarantor_and_verifies_rev2() {
        let dir = crate::test_support::temp_test_dir("vouchers_k4_rev2");
        let pw = "pw-k4-test";
        let (mut svc_creator, _) =
            test_utils::setup_service_with_profile(&dir.join("c"), &ACTORS.alice, "Alice", pw);
        svc_creator.unlock_session(pw, 600).unwrap();
        let creator_id = svc_creator
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();

        let (mut svc_guar, _) =
            test_utils::setup_service_with_profile(&dir.join("g"), &ACTORS.bob, "Bob", pw);
        svc_guar.unlock_session(pw, 600).unwrap();
        let guar_id = svc_guar
            .with_wallet(|w| w.get_user_id().to_string())
            .unwrap();
        let guar_state = AppState::new(svc_guar);

        let s1_sk = SigningKey::from_bytes(&[21u8; 32]);
        let s2_sk = SigningKey::from_bytes(&[22u8; 32]);
        let creator_mig_sk = SigningKey::from_bytes(&[23u8; 32]);
        let retain_q = encode_quarter(2035, 4).unwrap();

        let manifest_n1 = AnchorManifest {
            revision: 1,
            threshold: 1,
            servers: vec![ServerEntry {
                write_url: "https://sync1.example/sync.php".into(),
                read_url: "https://sync1.example/d".into(),
                admin_pub: s1_sk.verifying_key().to_bytes(),
                lock_pub: s1_sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            }],
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

        let minuto_toml = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../voucher_standards/minuto_v1/standard.toml"
        ))
        .unwrap();
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

        // N=1 manifest on guarantor-required standard MUST fail guarantor anchor check
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
        assert!(
            !res_n1.passed,
            "N=1 must be rejected for guarantor standards"
        );

        // Migrated rev=2 N=2 manifest signed with genuine migration chain MUST pass verify_anchor_id check
        let new_servers = vec![
            manifest_n1.servers[0].clone(),
            ServerEntry {
                write_url: "https://sync2.example/sync.php".into(),
                read_url: "https://sync2.example/d".into(),
                admin_pub: s2_sk.verifying_key().to_bytes(),
                lock_pub: s2_sk.verifying_key().to_bytes(),
                nominated_by: NominatedBy::Creator,
            },
        ];
        let prop = human_money_core::services::l2_rotation::build_creator_rotation_proposal(
            anchor_id,
            &manifest_n1,
            2,
            new_servers,
            Vec::new(),
            1_760_000_010,
            &creator_mig_sk,
        )
        .unwrap();
        let imported_state_hash = human_money_core::humoco_proto::compute_import_union_state_hash(
            &anchor_id,
            prop.to_rev,
            &std::collections::BTreeMap::new(),
        )
        .unwrap();
        let prop_hash = prop.core_digest().unwrap();
        let import_acks: Vec<human_money_core::humoco_proto::ImportAck> = [&s1_sk, &s2_sk]
            .iter()
            .map(|sk| {
                let mut ack = human_money_core::humoco_proto::ImportAck {
                    anchor_id,
                    proposal_hash: prop_hash,
                    to_rev: 2,
                    imported_state_hash,
                    retain_until_quarter: retain_q,
                    lock_pub: sk.verifying_key().to_bytes(),
                    acked_at: 1_760_000_010,
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
        let commit_sig = human_money_core::humoco_proto::CommitteeSignature {
            lock_pub: s1_sk.verifying_key().to_bytes(),
            committed_at: 1_760_000_010,
            sig: ed25519_dalek::Signer::sign(&s1_sk, &commit_d).to_bytes(),
        };
        let (rev2_manifest, _) =
            human_money_core::services::l2_rotation::assemble_migrated_manifest(
                &manifest_n1,
                &anchor_id,
                prop,
                imported_state_hash,
                import_acks,
                vec![commit_sig],
            )
            .unwrap();
        let rev2_json = serde_json::to_string(&rev2_manifest).unwrap();
        let rev2_delegations: Vec<crate::sync_client::SyncLockDelegationDto> = rev2_manifest
            .servers
            .iter()
            .zip([&s1_sk, &s2_sk])
            .map(|(srv, sk)| {
                let admin_hex = hex::encode(srv.admin_pub);
                let lock_hex = hex::encode(srv.lock_pub);
                let msg = humoco_market::format_lock_delegate_signature_message(
                    &admin_hex,
                    &lock_hex,
                    &srv.write_url,
                    &srv.read_url,
                    1_760_000_010,
                );
                let sig = hex::encode(ed25519_dalek::Signer::sign(sk, msg.as_bytes()).to_bytes());
                crate::sync_client::SyncLockDelegationDto {
                    admin_pub: admin_hex,
                    lock_pub: lock_hex,
                    write_url: srv.write_url.clone(),
                    read_url: srv.read_url.clone(),
                    issued_at: 1_760_000_010,
                    sig,
                }
            })
            .collect();
        let rev2_del_json = serde_json::to_string(&rev2_delegations).unwrap();
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

        // S04-F4 Fail-First: stripping anchor_delegations_json with empty probe cache MUST fail
        // with GUARANTOR_ISSUE_SYBIL_BELOW_THRESHOLD (empty write_url candidates excluded)
        let res_no_del = evaluate_guarantor_anchor_check_impl(
            details_n1.voucher,
            minuto_toml,
            Some(rev2_json),
            None,
            Some(pw),
            &guar_state,
        )
        .unwrap();
        assert!(
            !res_no_del.passed
                && res_no_del
                    .issues
                    .contains(&GUARANTOR_ISSUE_SYBIL_BELOW_THRESHOLD.to_string()),
            "S04-F4: stripping anchor_delegations_json must fail Sybil check, got: {:?}",
            res_no_del.issues
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
