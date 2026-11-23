use super::storage::*;
use super::*;

use humoco_market::{
    compute_recovery_sas, encode_qr_svg, parse_recovery_request_uri, seal_guardian_release_reply,
    GuardianReleasePayload,
};

use crate::models::{
    ApproveGuardianRecoveryResultDto, FrontendError, GuardianRecoveryRequestInspectionDto,
    GuardianWardDto,
};
use crate::state::AppState;

// ============================================================================
// Friend-Side Logic (Logged in)
// ============================================================================

pub fn inspect_guardian_recovery_request_impl(
    request_uri: String,
    password: Option<&str>,
    state: &AppState,
) -> Result<GuardianRecoveryRequestInspectionDto, FrontendError> {
    let link = parse_recovery_request_uri(&request_uri).map_err(FrontendError::from)?;
    let now_secs = crate::sync_client::current_unix_timestamp();
    let is_stale_48h = link.is_stale_48h(now_secs);
    let sas_code = compute_recovery_sas(&link.pk_temp);

    let (vault, contacts) = {
        let (mut service, resolved_pw) =
            state.authorize_and_lock_secret(crate::state::RequiredAccess::Unlocked, password)?;
        let pw = resolved_pw.as_deref().map(String::as_str);
        let vault = load_guardian_vault_with_cache(state, &mut service, pw)?;
        let contacts = state
            .load_contacts(&mut service, pw)
            .or_else(|_| state.get_cached_contacts())
            .unwrap_or_default();
        (vault, contacts)
    };

    let wards: Vec<GuardianWardDto> = vault
        .wards
        .values()
        .map(|e| GuardianWardDto {
            owner_did: e.owner_did.clone(),
            owner_display_name: resolve_ward_display_name(
                &e.owner_did,
                &e.owner_display_name,
                &contacts,
            ),
            rotation_epoch: e.rotation_epoch,
            threshold_m: e.threshold_m,
            total_n: e.total_n,
            updated_at: e.updated_at,
        })
        .collect();

    Ok(GuardianRecoveryRequestInspectionDto {
        sas_code,
        requested_at: link.ts,
        is_stale_48h,
        name_hint: link.name_hint,
        wards,
    })
}

pub fn approve_guardian_recovery_request_impl(
    request_uri: String,
    owner_did: String,
    password: Option<&str>,
    state: &AppState,
) -> Result<ApproveGuardianRecoveryResultDto, FrontendError> {
    let link = parse_recovery_request_uri(&request_uri).map_err(FrontendError::from)?;
    let sas_code = compute_recovery_sas(&link.pk_temp);

    let (entry, extra_descriptors, resolved_owner_name) = {
        let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Full, password)?;
        let vault = load_guardian_vault_with_cache(state, &mut service, password)?;
        let entry = vault
            .wards
            .values()
            .find(|e| is_same_account_id(&e.owner_did, &owner_did))
            .cloned()
            .ok_or_else(|| {
                FrontendError::from("Kein Schützling mit dieser ID im Tresor gefunden.".to_string())
            })?;
        let contacts = state
            .load_contacts(&mut service, password)
            .unwrap_or_default();
        let resolved_name =
            resolve_ward_display_name(&entry.owner_did, &entry.owner_display_name, &contacts);

        // Collect extra known sync descriptors from own hosted sync
        let mut extras = Vec::new();
        if let Some(admin) =
            crate::services::sync_engine::admin::load_stored_sync_admin_opt(&mut service, password)?
        {
            if !admin.descriptor.write_url.is_empty() {
                extras.push(admin.descriptor.clone());
            }
        }

        (entry, extras, resolved_name)
    };

    let now_secs = crate::sync_client::current_unix_timestamp();
    let payload = GuardianReleasePayload {
        entry,
        extra_sync_descriptors: extra_descriptors,
        released_at: now_secs,
    };

    let reply_uri =
        seal_guardian_release_reply(&link.pk_temp, &payload).map_err(FrontendError::from)?;
    let reply_qr_svg = encode_qr_svg(reply_uri.as_bytes()).map_err(FrontendError::from)?;

    Ok(ApproveGuardianRecoveryResultDto {
        reply_uri,
        reply_qr_svg,
        sas_code,
        owner_display_name: resolved_owner_name,
    })
}
