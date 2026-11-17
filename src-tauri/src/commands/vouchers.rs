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
) -> Result<crate::models::FrontendVoucher, FrontendError> {
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
            amount: data.nominal_value.amount,
            unit: data.nominal_value.unit,
            ..Default::default()
        },
        collateral: Some(human_money_core::models::voucher::Collateral {
            value: human_money_core::models::voucher::ValueDefinition {
                amount: data.collateral.amount,
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

    Ok(voucher.into())
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
    service
        .create_signing_request_bundle(&local_instance_id, config)
        .map_err(FrontendError::from)
}

#[tauri::command]
pub fn open_voucher_signing_request(
    container_bytes: Vec<u8>,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<crate::models::FrontendVoucherDetails, FrontendError> {
    info!("Opening voucher signing request bundle...");
    let service = state.service.lock().unwrap();
    match service.open_voucher_signing_request(&container_bytes, password.as_deref()) {
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

            Ok(crate::models::FrontendVoucherDetails {
                local_instance_id: format!(
                    "external-{}",
                    v.voucher_id.chars().take(8).collect::<String>()
                ),
                status: "Active".to_string(),
                current_amount,
                voucher: v.into(),
                display_currency,
                display_standard_name,
                is_test_voucher,
                trust: Default::default(),
            })
        }
        Err(e) => Err(FrontendError::from(e)),
    }
}

#[tauri::command]
pub fn create_detached_signature_response_bundle(
    voucher: crate::models::FrontendVoucher,
    role: String,
    include_details: bool,
    config: ContainerConfig,
    password: Option<String>,
    state: tauri::State<AppState>,
) -> Result<Vec<u8>, FrontendError> {
    info!(
        "Creating detached signature response for role {} with config {:?}",
        role, config
    );
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

    if let Some(ref pwd) = password {
        if let Err(e) = service.unlock_session(pwd, 300) {
            error!("Failed to unlock session during signature creation: {}", e);
        }
    }

    let core_voucher = human_money_core::models::voucher::Voucher::from(voucher);
    service
        .create_detached_signature_response_bundle(
            &core_voucher,
            &role,
            include_details,
            config,
            password.as_deref(),
        )
        .map_err(FrontendError::from)
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
#[tauri::command]
pub fn evaluate_guarantor_anchor_check(
    voucher: crate::models::FrontendVoucher,
    standard_toml_content: String,
    manifest_json: Option<String>,
    state: tauri::State<AppState>,
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
        issues.push("missing_layer2_anchor".to_string());
        return Ok(FrontendGuarantorAnchorCheckDto {
            required: true,
            passed: false,
            has_layer2_anchor: false,
            retention_ok: Some(false),
            own_server_present: Some(false),
            sybil_independent_count: 0,
            threshold: 0,
            server_count: 0,
            issues,
        });
    };

    let anchor_hex = hex::encode(anchor_id);
    let manifest_opt: Option<human_money_core::humoco_proto::AnchorManifest> = if let Some(raw) =
        manifest_json
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    {
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

    if manifest.rev1_anchor_id().ok() != Some(anchor_id) {
        issues.push("manifest_anchor_mismatch".to_string());
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
        let own_norm = crate::checkout::normalize_sync_write_url(&own.write_url);
        let present = manifest.servers.iter().any(|s| {
            crate::checkout::normalize_sync_write_url(&s.write_url) == own_norm
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
            let norm = crate::checkout::normalize_sync_write_url(&s.write_url);
            let asn = settings
                .own_nominated_lock_server
                .as_ref()
                .filter(|o| crate::checkout::normalize_sync_write_url(&o.write_url) == norm)
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
