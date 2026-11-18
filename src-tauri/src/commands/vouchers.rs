// src-tauri/src/commands/vouchers.rs
use crate::{
    models::{FrontendError, FrontendNewVoucherData},
    AppState,
};
use human_money_core::{models::secure_container::ContainerConfig, NewVoucherData};
use log::{error, info};

#[tauri::command]
pub fn create_new_voucher(
    standard_toml_content: String,
    data: FrontendNewVoucherData,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendVoucherDetails, FrontendError> {
    info!("Attempting to create a new voucher...");
    let mut service = state.service.lock().unwrap();
    let user_id = service
        .with_wallet(|w| w.get_user_id().to_string())
        .map_err(FrontendError::from)?;

    let anchor_bytes = match data
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

    let voucher_data = NewVoucherData {
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
        layer2_anchor: anchor_bytes,
    };

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
                    .ok_or_else(|| FrontendError::from("anchor_template_not_found".to_string()))?;
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
        .create_new_voucher(&standard_toml_content, voucher_data, password.as_deref())
        .map_err(FrontendError::from)?;

    if let Err(e) = state.refresh_events_cache(&mut service, password.as_deref()) {
        error!(
            "Failed to refresh events cache after create_new_voucher: {}",
            e
        );
    }

    let created_voucher_id = voucher.voucher_id.clone();
    let details_res = service
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
        .map_err(FrontendError::from)?;

    Ok(details_res)
}

pub(crate) fn create_signing_request_bundle_impl(
    local_instance_id: &str,
    config: ContainerConfig,
    service: &human_money_core::app_service::AppService,
) -> Result<Vec<u8>, FrontendError> {
    let raw = service
        .create_signing_request_bundle(local_instance_id, config)
        .map_err(FrontendError::from)?;

    let embedded_manifest_json = service
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
        .flatten()
        .and_then(|m| serde_json::to_string(&m).ok());

    if let Some(manifest_json) = embedded_manifest_json {
        if let Ok(mut container_val) = serde_json::from_slice::<serde_json::Value>(&raw) {
            if let Some(obj) = container_val.as_object_mut() {
                obj.insert(
                    "anchor_manifest_json".to_string(),
                    serde_json::Value::String(manifest_json),
                );
                if let Ok(enriched) = serde_json::to_vec_pretty(&container_val) {
                    return Ok(enriched);
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
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    info!(
        "Creating signing request bundle for voucher {} with config {:?}",
        local_instance_id, config
    );
    let service = state.service.lock().unwrap();
    create_signing_request_bundle_impl(&local_instance_id, config, &service)
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

            let embedded_manifest = v.layer2_anchor.and_then(|aid| {
                serde_json::from_slice::<serde_json::Value>(container_bytes)
                    .ok()
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
                        .filter(|m| {
                            m.rev1_anchor_id().ok() == Some(aid) && m.verify_anchor_id(&aid).is_ok()
                        })
                        .map(|m| (raw_m, m))
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
    let service = state.service.lock().unwrap();
    open_voucher_signing_request_impl(&container_bytes, password.as_deref(), &service)
}

pub(crate) fn create_detached_signature_response_bundle_impl(
    voucher: crate::models::FrontendVoucher,
    role: String,
    include_details: bool,
    config: ContainerConfig,
    password: Option<String>,
    anchor_manifest_json: Option<String>,
    app_opt: Option<&tauri::AppHandle>,
    state: &AppState,
) -> Result<Vec<u8>, FrontendError> {
    info!(
        "Creating detached signature response for role {} with config {:?}",
        role, config
    );

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
            anchor_manifest_json,
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

    let mut service = state.service.lock().unwrap();

    if !service.is_wallet_unlocked() {
        error!(
            "SIGNING ERROR: AppService is in LOCKED state. No profile is currently loaded in the backend."
        );
        return Err(FrontendError::from(
            "Wallet is completely locked. Please log out and log in again to refresh your session."
                .to_string(),
        ));
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
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    create_detached_signature_response_bundle_impl(
        voucher,
        role,
        include_details,
        config,
        password,
        anchor_manifest_json,
        Some(&app),
        &state,
    )
}

#[tauri::command]
pub fn process_and_attach_signature(
    container_bytes: Vec<u8>,
    standard_toml_content: String,
    container_password: Option<String>,
    wallet_password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<String, FrontendError> {
    info!("Processing and attaching signature from bundle...");
    let mut service = state.service.lock().unwrap();
    service
        .process_and_attach_signature(
            &container_bytes,
            &standard_toml_content,
            container_password.as_deref(),
            wallet_password.as_deref(),
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
    let mut service = state.service.lock().unwrap();
    service
        .remove_voucher_signature(&local_instance_id, &signature_id, password.as_deref())
        .map_err(FrontendError::from)
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FrontendGuarantorAnchorCheckDto {
    pub required: bool,
    pub passed: bool,
    pub has_layer2_anchor: bool,
    pub retention_ok: Option<bool>,
    pub own_server_present: Option<bool>,
    pub sybil_independent_count: usize,
    pub threshold: u8,
    pub server_count: usize,
    pub issues: Vec<String>,
}

/// Evaluates guarantor pre-signing L2 security requirements (Concept §5.3).
/// Active **only** for vouched voucher standards (`additional_signatures_range[0] > 0`
/// and `"guarantor"` allowed). Non-guarantor / `N=0` voucher flows stay unaffected.
pub(crate) fn evaluate_guarantor_anchor_check_impl(
    voucher: crate::models::FrontendVoucher,
    standard_toml_content: String,
    manifest_json: Option<String>,
    state: &AppState,
) -> Result<FrontendGuarantorAnchorCheckDto, FrontendError> {
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
            own_server_present: None,
            sybil_independent_count: 0,
            threshold: 0,
            server_count: 0,
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
            own_server_present: None,
            sybil_independent_count: 0,
            threshold: 0,
            server_count: 0,
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
        issues.push("missing_anchor_manifest".to_string());
        return Ok(FrontendGuarantorAnchorCheckDto {
            required: true,
            passed: false,
            has_layer2_anchor: true,
            retention_ok: None,
            own_server_present: None,
            sybil_independent_count: 0,
            threshold: 0,
            server_count: 0,
            issues,
        });
    };

    let anchor_valid = manifest.rev1_anchor_id().ok() == Some(anchor_id)
        && manifest.verify_anchor_id(&anchor_id).is_ok();
    if !anchor_valid {
        issues.push("manifest_anchor_mismatch".to_string());
    } else if supplied_manifest_json.is_some() {
        if let Ok(mut service) = state.service.lock() {
            let should_cache = service
                .with_wallet(|w| {
                    w.profile
                        .l2_anchor_manifests
                        .get(&anchor_hex)
                        .is_none_or(|existing| manifest.revision > existing.revision)
                })
                .unwrap_or(false);
            if should_cache {
                let _ = service.update_l2_anchor_manifest(manifest.clone(), None);
            }
        }
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
        issues.push("retention_does_not_cover_deletable_at".to_string());
    }

    let settings = state.get_cached_settings().unwrap_or_default();
    let own_server_present = if let Some(ref own) = settings.own_nominated_lock_server {
        let own_norm = crate::services::common::normalize_sync_url(&own.write_url);
        let present = manifest.servers.iter().any(|s| {
            crate::services::common::normalize_sync_url(&s.write_url) == own_norm
                && hex::encode(s.lock_pub) == own.lock_pub.to_ascii_lowercase()
                && matches!(
                    s.nominated_by,
                    human_money_core::humoco_proto::NominatedBy::Guarantor(_)
                )
        });
        if !present {
            issues.push("own_nominated_server_missing".to_string());
        }
        Some(present)
    } else {
        issues.push("no_own_nominated_server_configured".to_string());
        Some(false)
    };

    let candidates: Vec<crate::gateway_resolver::SybilServerCandidate> = manifest
        .servers
        .iter()
        .map(|s| {
            let norm = crate::services::common::normalize_sync_url(&s.write_url);
            let asn = settings
                .own_nominated_lock_server
                .as_ref()
                .filter(|o| crate::services::common::normalize_sync_url(&o.write_url) == norm)
                .and_then(|o| {
                    o.asn
                        .as_deref()
                        .and_then(|s| s.trim().trim_start_matches("AS").parse::<u32>().ok())
                });
            crate::gateway_resolver::SybilServerCandidate {
                write_url: s.write_url.clone(),
                lock_pub: s.lock_pub,
                admin_pub: Some(s.admin_pub),
                asn,
            }
        })
        .collect();
    let folded = crate::gateway_resolver::sybil_fold_committee(&candidates);
    if folded.len() < manifest.threshold as usize {
        issues.push("sybil_independent_servers_below_threshold".to_string());
    }

    let passed = issues.is_empty();
    Ok(FrontendGuarantorAnchorCheckDto {
        required: true,
        passed,
        has_layer2_anchor: true,
        retention_ok: Some(retention_ok),
        own_server_present,
        sybil_independent_count: folded.len(),
        threshold: manifest.threshold,
        server_count: manifest.servers.len(),
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
    state: tauri::State<AppState>,
) -> Result<FrontendGuarantorAnchorCheckDto, FrontendError> {
    evaluate_guarantor_anchor_check_impl(voucher, standard_toml_content, manifest_json, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{AppSettings, NominatedLockServerMeta};
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

        let guar_settings = AppSettings {
            own_nominated_lock_server: Some(NominatedLockServerMeta {
                write_url: "https://sync2.example/sync.php".into(),
                read_url: "https://sync2.example/d".into(),
                drop_pub: "drop_pub_2".into(),
                admin_pub: hex::encode(s2_sk.verifying_key().to_bytes()),
                lock_pub: hex::encode(s2_sk.verifying_key().to_bytes()),
                asn: Some("AS64512".into()),
                probed_at_secs: 1_760_000_000,
            }),
            ..Default::default()
        };
        let guar_state = AppState::new(svc_guarantor);
        *guar_state.settings.lock().unwrap() = Some(guar_settings);

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
            &guar_state,
        )
        .unwrap_err();
        assert_eq!(reject_err.code, "error.voucher.guarantorAnchorCheckFailed");

        let check = evaluate_guarantor_anchor_check_impl(
            details.voucher,
            minuto_toml,
            details.anchor_manifest_json,
            &guar_state,
        )
        .unwrap();
        assert!(
            check.passed,
            "guarantor anchor check should pass from embedded .ask manifest, got {:?}",
            check.issues
        );
        assert_eq!(check.own_server_present, Some(true));
        assert_eq!(check.sybil_independent_count, 2);

        let _ = std::fs::remove_dir_all(dir);
    }
}
