use crate::models::FrontendError;
use crate::AppState;

pub const FIXED_SCALE: i128 = 100_000_000;

pub fn get_active_asset_classes_impl(
    state: &AppState,
) -> Result<Vec<crate::models::FrontendAssetClassSummary>, FrontendError> {
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let asset_classes = service
        .with_wallet(|w| w.get_active_asset_classes())
        .map_err(FrontendError::from)?;
    Ok(asset_classes.into_iter().map(|s| s.into()).collect())
}

pub fn get_user_profile_impl(
    state: &AppState,
) -> Result<crate::models::FrontendUserProfile, FrontendError> {
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let profile = service
        .with_wallet(|w| w.profile.to_public_profile())
        .map_err(FrontendError::from)?;
    Ok(profile.into())
}

pub fn get_user_id_impl(state: &AppState) -> Result<String, FrontendError> {
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    service
        .with_wallet(|w| w.get_user_id().to_string())
        .map_err(FrontendError::from)
}

pub fn parse_fixed_point(amount_str: &str) -> Option<i128> {
    let s = amount_str.trim().replace(',', ".");
    if s.is_empty() {
        return None;
    }
    let (is_negative, num_str) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = s.strip_prefix('+') {
        (false, rest)
    } else {
        (false, s.as_str())
    };
    if num_str.is_empty() {
        return None;
    }

    let mut parts = num_str.splitn(2, '.');
    let int_part_str = parts.next()?;
    let frac_part_str = parts.next().unwrap_or("");

    if int_part_str.is_empty() && frac_part_str.is_empty() {
        return None;
    }
    if !int_part_str.bytes().all(|b| b.is_ascii_digit())
        || !frac_part_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }

    let int_part = if int_part_str.is_empty() {
        0i128
    } else {
        int_part_str.parse::<i128>().ok()?
    };

    let mut frac_val = 0i128;
    if !frac_part_str.is_empty() {
        let mut padded = frac_part_str[..frac_part_str.len().min(8)].to_string();
        while padded.len() < 8 {
            padded.push('0');
        }
        frac_val = padded.parse::<i128>().ok()?;
    }

    let total = int_part.checked_mul(FIXED_SCALE)?.checked_add(frac_val)?;
    if is_negative {
        total.checked_neg()
    } else {
        Some(total)
    }
}

pub fn format_fixed_point(val: i128) -> String {
    if val == 0 {
        return "0".to_string();
    }
    let is_negative = val < 0;
    let scale = FIXED_SCALE as u128;
    let abs_val = val.unsigned_abs();
    let int_part = abs_val / scale;
    let frac_part = abs_val % scale;

    let sign = if is_negative { "-" } else { "" };
    if frac_part == 0 {
        format!("{}{}", sign, int_part)
    } else {
        let frac_str = format!("{:08}", frac_part);
        let trimmed_frac = frac_str.trim_end_matches('0');
        format!("{}{}.{}", sign, int_part, trimmed_frac)
    }
}

fn canonical_asset_unit(nv: &human_money_core::models::voucher::ValueDefinition) -> String {
    let trimmed_unit = nv.unit.trim();
    if !trimmed_unit.is_empty() {
        trimmed_unit.to_string()
    } else {
        nv.abbreviation.as_deref().unwrap_or("").trim().to_string()
    }
}

pub fn calculate_aggregated_balances(
    w: &human_money_core::wallet::Wallet,
    id: &human_money_core::models::profile::UserIdentity,
) -> Vec<crate::models::FrontendAggregatedBalance> {
    use std::collections::{HashMap, HashSet};

    struct AssetClassMeta {
        standard_name: String,
        standard_uuid: String,
        unit: String,
        is_test_voucher: bool,
        display_currency: String,
        display_standard_name: String,
    }

    #[derive(Default, Clone)]
    struct Amounts {
        received_amount: i128,
        own_available_amount: i128,
    }

    let mut meta_map: HashMap<(String, String, bool), AssetClassMeta> = HashMap::new();
    let mut amounts_map: HashMap<(String, String, bool), Amounts> = HashMap::new();

    // 1. Sum up active vouchers using list_vouchers
    let summaries = w.list_vouchers(Some(id), None, None, None);
    for s in summaries {
        if !matches!(
            s.status,
            human_money_core::wallet::instance::VoucherStatus::Active
        ) {
            continue;
        }

        let unit = w
            .voucher_store
            .vouchers
            .get(&s.local_instance_id)
            .map(|inst| canonical_asset_unit(&inst.voucher.nominal_value))
            .unwrap_or_else(|| s.unit.clone());

        let key = (
            s.voucher_standard_uuid.clone(),
            unit.clone(),
            s.is_test_voucher,
        );
        meta_map
            .entry(key.clone())
            .or_insert_with(|| AssetClassMeta {
                standard_name: s.raw_standard_name.clone(),
                standard_uuid: s.voucher_standard_uuid.clone(),
                unit: unit.clone(),
                is_test_voucher: s.is_test_voucher,
                display_currency: s.display_currency.clone(),
                display_standard_name: s.display_standard_name.clone(),
            });

        let amount = parse_fixed_point(&s.current_amount).unwrap_or(0);
        let entry = amounts_map.entry(key).or_default();
        if s.creator_id == id.user_id {
            entry.own_available_amount = entry.own_available_amount.saturating_add(amount);
        } else {
            entry.received_amount = entry.received_amount.saturating_add(amount);
        }
    }

    // 2. Calculate own circulating amounts across all root vouchers created by this user
    let mut root_nominal_by_asset: HashMap<(String, String, bool), HashMap<String, i128>> =
        HashMap::new();
    let mut redeemed_by_asset: HashMap<(String, String, bool), i128> = HashMap::new();
    let mut seen_redeemed_tx: HashMap<(String, String, bool), HashSet<(String, String)>> =
        HashMap::new();

    for instance in w.voucher_store.vouchers.values() {
        let is_own = instance.voucher.creator_profile.id.as_deref() == Some(&id.user_id);
        if !is_own {
            continue;
        }
        if matches!(
            instance.status,
            human_money_core::wallet::instance::VoucherStatus::Incomplete { .. }
                | human_money_core::wallet::instance::VoucherStatus::Expired
        ) {
            continue;
        }
        let not_expired = chrono::DateTime::parse_from_rfc3339(&instance.voucher.valid_until)
            .map(|dt| dt > chrono::Utc::now())
            .unwrap_or(false);
        if !not_expired {
            continue;
        }

        let unit = canonical_asset_unit(&instance.voucher.nominal_value);
        let is_test = instance.voucher.non_redeemable_test_voucher;
        let key = (
            instance.voucher.voucher_standard.uuid.clone(),
            unit.clone(),
            is_test,
        );

        meta_map.entry(key.clone()).or_insert_with(|| {
            let std_name = instance.voucher.voucher_standard.name.clone();
            let raw_display_currency = instance
                .voucher
                .nominal_value
                .abbreviation
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| unit.clone());
            let display_currency = if is_test && !raw_display_currency.starts_with("TEST-") {
                format!("TEST-{}", raw_display_currency)
            } else {
                raw_display_currency
            };
            let display_standard_name = if is_test && !std_name.starts_with("TEST-") {
                format!("TEST-{}", std_name)
            } else {
                std_name.clone()
            };
            AssetClassMeta {
                standard_name: std_name,
                standard_uuid: instance.voucher.voucher_standard.uuid.clone(),
                unit: unit.clone(),
                is_test_voucher: is_test,
                display_currency,
                display_standard_name,
            }
        });

        // Record unique root nominal amount
        let nominal = parse_fixed_point(&instance.voucher.nominal_value.amount).unwrap_or(0);
        root_nominal_by_asset
            .entry(key.clone())
            .or_default()
            .entry(instance.voucher.voucher_id.clone())
            .or_insert(nominal);

        // Record redeemed txs
        for tx in &instance.voucher.transactions {
            if tx.t_type == "redeem" {
                let tx_key = (instance.voucher.voucher_id.clone(), tx.t_id.clone());
                if seen_redeemed_tx
                    .entry(key.clone())
                    .or_default()
                    .insert(tx_key)
                {
                    let redeemed = redeemed_by_asset.entry(key.clone()).or_default();
                    *redeemed = redeemed.saturating_add(parse_fixed_point(&tx.amount).unwrap_or(0));
                }
            }
        }
    }

    // 3. Assemble frontend aggregated balance entries
    let mut result = Vec::new();
    for (key, meta) in meta_map {
        let amounts = amounts_map.get(&key).cloned().unwrap_or_default();
        let received_amount = amounts.received_amount;
        let own_available_amount = amounts.own_available_amount;

        let nominal_total: i128 = root_nominal_by_asset
            .get(&key)
            .map(|m| m.values().copied().fold(0i128, i128::saturating_add))
            .unwrap_or(0);
        let redeemed_amount = redeemed_by_asset.get(&key).copied().unwrap_or(0);

        let own_circulating_amount = nominal_total
            .saturating_sub(redeemed_amount)
            .saturating_sub(own_available_amount)
            .max(0);
        let total_amount = received_amount.saturating_add(own_available_amount);
        let net_balance = received_amount.saturating_sub(own_circulating_amount);

        if total_amount > 0 || own_circulating_amount > 0 {
            result.push(crate::models::FrontendAggregatedBalance {
                standard_name: meta.standard_name,
                standard_uuid: meta.standard_uuid,
                unit: meta.unit,
                total_amount: format_fixed_point(total_amount),
                display_currency: meta.display_currency,
                display_standard_name: meta.display_standard_name,
                is_test_voucher: meta.is_test_voucher,
                received_amount: format_fixed_point(received_amount),
                own_available_amount: format_fixed_point(own_available_amount),
                own_circulating_amount: format_fixed_point(own_circulating_amount),
                net_balance: format_fixed_point(net_balance),
            });
        }
    }

    result.sort_by(|a, b| a.standard_name.cmp(&b.standard_name));
    result
}

pub fn get_total_balance_by_currency_impl(
    state: &AppState,
) -> Result<Vec<crate::models::FrontendAggregatedBalance>, FrontendError> {
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let balances = service
        .with_wallet_and_identity(calculate_aggregated_balances)
        .map_err(FrontendError::from)?;
    Ok(balances)
}

pub fn get_voucher_summaries_impl(
    test_filter: Option<bool>,
    state: &AppState,
) -> Result<Vec<crate::models::FrontendVoucherSummary>, FrontendError> {
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let summaries = service
        .with_wallet_and_identity(|w, id| {
            let raw_summaries = w.list_vouchers(Some(id), None, None, test_filter);
            let mut enriched = Vec::with_capacity(raw_summaries.len());
            for s in raw_summaries {
                let mut summary: crate::models::FrontendVoucherSummary = s.into();
                if summary.status == "active"
                    && parse_fixed_point(&summary.current_amount) == Some(0)
                {
                    summary.status = "archived".to_string();
                }
                if let Some(instance) = w.voucher_store.vouchers.get(&summary.local_instance_id) {
                    summary.has_layer2_anchor = instance.voucher.layer2_anchor.is_some();
                    summary.description = instance
                        .voucher
                        .nominal_value
                        .description
                        .clone()
                        .or_else(|| instance.voucher.creator_profile.service_offer.clone())
                        .unwrap_or_default();
                    summary.trust = state.evaluate_voucher_trust(
                        w,
                        &summary.creator_id,
                        &instance.voucher.signatures,
                    );
                }
                enriched.push(summary);
            }
            enriched
        })
        .map_err(FrontendError::from)?;
    Ok(summaries)
}

pub(crate) fn build_enriched_voucher_details(
    state: &AppState,
    w: &human_money_core::wallet::Wallet,
    id: &human_money_core::models::profile::UserIdentity,
    local_id: &str,
) -> Result<crate::models::FrontendVoucherDetails, human_money_core::Error> {
    let raw_details = w.get_voucher_details(local_id)?;
    let mut frontend_details: crate::models::FrontendVoucherDetails = raw_details.clone().into();
    if let Some(summary) = w
        .list_vouchers(Some(id), None, None, None)
        .into_iter()
        .find(|s| s.local_instance_id == local_id)
    {
        frontend_details.current_amount = summary.current_amount;
    }
    if frontend_details.status == "active"
        && parse_fixed_point(&frontend_details.current_amount) == Some(0)
    {
        frontend_details.status = "archived".to_string();
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
}

pub fn get_voucher_details_impl(
    local_id: String,
    state: &AppState,
) -> Result<crate::models::FrontendVoucherDetails, FrontendError> {
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let details = service
        .with_wallet_and_identity(|w, id| build_enriched_voucher_details(state, w, id, &local_id))
        .map_err(FrontendError::from)?
        .map_err(FrontendError::from)?;
    Ok(details)
}

pub fn get_double_spend_conflicts_impl(
    state: &AppState,
) -> Result<Vec<crate::models::FrontendProofOfDoubleSpendSummary>, FrontendError> {
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let conflicts = service.list_conflicts().map_err(FrontendError::from)?;
    Ok(conflicts.into_iter().map(|s| s.into()).collect())
}

pub(crate) fn conflict_role_str(
    role: human_money_core::models::conflict::ConflictRole,
) -> &'static str {
    match role {
        human_money_core::models::conflict::ConflictRole::Victim => "victim",
        human_money_core::models::conflict::ConflictRole::Witness => "witness",
    }
}

pub fn get_proof_of_double_spend_impl(
    proof_id: String,
    state: &AppState,
) -> Result<crate::models::FullProofDetails, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    if state.cache_wot().is_none() {
        let _ = state.get_or_load_wot_register(&mut service, None);
    }
    if state.cache_contacts().is_none() {
        let _ = state.load_contacts(&mut service, None);
    }
    let summaries = service.list_conflicts().map_err(FrontendError::from)?;
    let summary = summaries
        .into_iter()
        .find(|s| s.proof_id == proof_id)
        .ok_or_else(|| FrontendError::from(format!("Conflict proof {} not found", proof_id)))?;

    let proof = service
        .get_proof_of_double_spend(&proof_id)
        .map_err(FrontendError::from)?;

    let conflict_role = conflict_role_str(summary.conflict_role);
    let mut frontend_proof: crate::models::FrontendProofOfDoubleSpend = proof.into();
    frontend_proof.affected_voucher_name = summary
        .display_affected_voucher_name
        .or(summary.affected_voucher_name);
    frontend_proof.voucher_standard_uuid = summary.voucher_standard_uuid;
    frontend_proof.non_redeemable_test_voucher = summary.is_test_voucher;

    let (has_own_endorsement, can_endorse) = service
        .with_wallet(|wallet| {
            if let Some(ref mut resolutions) = frontend_proof.resolutions {
                for res in resolutions.iter_mut() {
                    let (endorser_name, is_own_identity, wot_trust) =
                        state.evaluate_did_trust_for_endorsement(wallet, &res.endorser_id);
                    res.endorser_name = endorser_name;
                    res.is_own_identity = is_own_identity;
                    res.wot_trust = wot_trust;
                }
                resolutions.sort_by(|a, b| {
                    b.is_own_identity
                        .cmp(&a.is_own_identity)
                        .then_with(|| {
                            a.wot_trust
                                .blocked_by_offender
                                .cmp(&b.wot_trust.blocked_by_offender)
                        })
                        .then_with(|| b.wot_trust.trust_percent.cmp(&a.wot_trust.trust_percent))
                        .then_with(|| {
                            a.wot_trust
                                .shortest_hops
                                .unwrap_or(u8::MAX)
                                .cmp(&b.wot_trust.shortest_hops.unwrap_or(u8::MAX))
                        })
                        .then_with(|| b.resolution_timestamp.cmp(&a.resolution_timestamp))
                });
            }

            let has_own = frontend_proof
                .resolutions
                .as_ref()
                .is_some_and(|r| r.iter().any(|item| item.is_own_identity));
            let own_pk =
                human_money_core::crypto::get_pubkey_from_user_id(wallet.get_user_id()).ok();
            let is_own_offender = own_pk.is_some()
                && (human_money_core::crypto::get_pubkey_from_user_id(&frontend_proof.offender_id)
                    .ok()
                    == own_pk
                    || frontend_proof
                        .suspected_identity
                        .as_deref()
                        .and_then(|s| human_money_core::crypto::get_pubkey_from_user_id(s).ok())
                        == own_pk);
            let can_end =
                frontend_proof.is_cryptographically_attributed && !is_own_offender && !has_own;
            (has_own, can_end)
        })
        .map_err(FrontendError::from)?;

    Ok(crate::models::FullProofDetails {
        proof: frontend_proof,
        local_override: summary.local_override,
        local_note: summary.local_note,
        conflict_role: conflict_role.to_string(),
        can_endorse,
        has_own_endorsement,
    })
}

pub fn get_proof_id_for_voucher_impl(
    local_id: String,
    state: &AppState,
) -> Result<Option<String>, FrontendError> {
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    service
        .get_proof_id_for_voucher(&local_id)
        .map_err(FrontendError::from)
}

pub fn check_reputation_impl(
    offender_id: String,
    state: &AppState,
) -> Result<crate::models::FrontendTrustStatus, FrontendError> {
    let offender_id = offender_id.trim().to_string();
    let service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let status = service
        .with_wallet(|w| w.check_reputation(&offender_id))
        .map_err(FrontendError::from)?;
    Ok(status.into())
}

const EVENTS_CACHE_PAGE_SIZE: usize = 50;

pub fn get_event_history_impl(
    offset: usize,
    limit: usize,
    state: &AppState,
) -> Result<Vec<crate::models::FrontendWalletEvent>, FrontendError> {
    state.authorize(crate::state::RequiredAccess::Unlocked, None)?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    let requested_end = offset.saturating_add(limit);
    let events_cache = state.cache_events();
    if let Some(events) = events_cache.as_ref() {
        let cache_covers_page =
            requested_end <= events.len() || events.len() < EVENTS_CACHE_PAGE_SIZE;
        if cache_covers_page {
            let start = offset.min(events.len());
            let end = requested_end.min(events.len());
            return Ok(events[start..end]
                .iter()
                .map(|e| e.clone().into())
                .collect());
        }
    }
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
    let events = service
        .get_event_history(offset, limit, None)
        .map_err(FrontendError::from)?;
    Ok(events.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use human_money_core::app_service::AppService;
    use human_money_core::models::wallet_event::{WalletEvent, WalletEventType};

    #[test]
    fn test_get_event_history_uses_cache() {
        use std::path::PathBuf;
        // Mock AppState
        let service = AppService::new(&PathBuf::from("/tmp")).unwrap();
        let state_raw = AppState::new(service);
        state_raw.set_cached_events(Some(vec![WalletEvent {
            event_id: "test_event_1".to_string(),
            event_type: WalletEventType::VoucherActivated,
            timestamp: Utc::now(),
            local_instance_id: "voucher_1".to_string(),
            voucher_id: "vid_1".to_string(),
            bff_data: Default::default(),
        }]));

        let events = state_raw.cache_events();
        assert!(events.is_some());
        assert_eq!(events.as_ref().unwrap().len(), 1);
        assert_eq!(events.as_ref().unwrap()[0].event_id, "test_event_1");
    }

    #[test]
    fn test_fixed_point_parse_and_format() {
        assert_eq!(parse_fixed_point("100"), Some(100 * FIXED_SCALE));
        assert_eq!(format_fixed_point(100 * FIXED_SCALE), "100");

        assert_eq!(parse_fixed_point("12.345"), Some(1234500000));
        assert_eq!(format_fixed_point(1234500000), "12.345");

        assert_eq!(parse_fixed_point("-300"), Some(-300 * FIXED_SCALE));
        assert_eq!(format_fixed_point(-300 * FIXED_SCALE), "-300");

        assert_eq!(parse_fixed_point("0"), Some(0));
        assert_eq!(format_fixed_point(0), "0");

        assert_eq!(parse_fixed_point("-0.5"), Some(-50000000));
        assert_eq!(format_fixed_point(-50000000), "-0.5");

        assert_eq!(parse_fixed_point("10,50"), Some(1050000000));
        assert_eq!(format_fixed_point(1050000000), "10.5");
    }

    #[test]
    fn test_balance_arithmetic_scenarios() {
        // Scenario 1: Own voucher minted 1000, not yet spent
        let nominal = 1000 * FIXED_SCALE;
        let own_available = 1000 * FIXED_SCALE;
        let redeemed = 0;
        let received = 0;
        let own_circulating = (nominal - redeemed - own_available).max(0);
        let total = received + own_available;
        let net = received - own_circulating;

        assert_eq!(own_circulating, 0);
        assert_eq!(total, 1000 * FIXED_SCALE);
        assert_eq!(net, 0);

        // Scenario 2: Spent 300 own vouchers, 700 remaining in wallet
        let own_available = 700 * FIXED_SCALE;
        let own_circulating = (nominal - redeemed - own_available).max(0);
        let total = received + own_available;
        let net = received - own_circulating;

        assert_eq!(own_circulating, 300 * FIXED_SCALE);
        assert_eq!(total, 700 * FIXED_SCALE);
        assert_eq!(net, -300 * FIXED_SCALE);

        // Scenario 3: Received 40 from someone else, while 300 own are circulating
        let received = 40 * FIXED_SCALE;
        let total = received + own_available;
        let net = received - own_circulating;

        assert_eq!(total, 740 * FIXED_SCALE);
        assert_eq!(net, -260 * FIXED_SCALE);
    }

    #[test]
    fn test_conflict_role_str_is_lowercase() {
        use human_money_core::models::conflict::ConflictRole;
        assert_eq!(conflict_role_str(ConflictRole::Victim), "victim");
        assert_eq!(conflict_role_str(ConflictRole::Witness), "witness");
    }

    #[test]
    fn s06_f5_fixed_point_rejects_malformed_utf8_and_formats_i128_min() {
        assert_eq!(parse_fixed_point(""), None);
        assert_eq!(parse_fixed_point("."), None);
        assert_eq!(parse_fixed_point("-."), None);
        assert_eq!(parse_fixed_point("+."), None);
        assert_eq!(parse_fixed_point("1.2.3"), None);
        // Must NOT panic on multi-byte UTF-8 boundary slicing
        assert_eq!(parse_fixed_point("1.éééééééé"), None);
        assert_eq!(parse_fixed_point("1.23é456789"), None);
        // Must NOT overflow on i128::MIN
        let formatted_min = format_fixed_point(i128::MIN);
        assert!(
            formatted_min.starts_with("-1701411834604692317316873037158."),
            "unexpected i128::MIN formatting: {}",
            formatted_min
        );
    }

    #[test]
    fn s06_f6_calculate_aggregated_balances_unit_key_test_prefix_and_expired_handling() {
        use human_money_core::test_utils::{self, ACTORS};
        use human_money_core::wallet::instance::VoucherStatus;
        use human_money_core::NewVoucherData;

        let dir = crate::test_support::temp_test_dir("s06_f6_balances");
        let pw = "pw-s06-f6";
        let (mut svc, _) = test_utils::setup_service_with_profile(&dir, &ACTORS.alice, "Alice", pw);
        svc.unlock_session(pw, 600).unwrap();

        let minuto_toml =
            test_utils::generate_signed_standard_toml("voucher_standards/minuto_v1/standard.toml");
        let ft_toml = test_utils::generate_signed_standard_toml(
            "voucher_standards/freetaler_v1/standard.toml",
        );
        let user_id = svc.with_wallet(|w| w.get_user_id().to_string()).unwrap();

        // Mint an own voucher with unit != abbreviation ("Minuto" vs "m") and activate it directly.
        let mut v_data = test_utils::create_minuto_voucher_data(
            human_money_core::models::profile::PublicProfile {
                id: Some(user_id.clone()),
                ..Default::default()
            },
        );
        v_data.nominal_value.amount = "60".into();
        let voucher = svc
            .create_new_voucher(&minuto_toml, v_data, Some(pw))
            .unwrap();
        let local_id =
            human_money_core::wallet::Wallet::calculate_local_instance_id(&voucher, &user_id)
                .unwrap();

        // Also mint an own test voucher to check TEST- prefix and an expired/corrupt-date voucher
        let test_v = svc
            .create_new_voucher(
                &ft_toml,
                NewVoucherData {
                    creator_profile: human_money_core::models::profile::PublicProfile {
                        id: Some(user_id.clone()),
                        ..Default::default()
                    },
                    nominal_value: human_money_core::models::voucher::ValueDefinition {
                        amount: "50".into(),
                        ..Default::default()
                    },
                    non_redeemable_test_voucher: true,
                    ..Default::default()
                },
                Some(pw),
            )
            .unwrap();
        let test_local_id =
            human_money_core::wallet::Wallet::calculate_local_instance_id(&test_v, &user_id)
                .unwrap();

        let balances = svc
            .with_wallet_and_identity(|w, id| {
                let mut w = w.clone();
                // Make the minuto voucher Active and give it distinct unit vs abbreviation
                if let Some(inst) = w.voucher_store.vouchers.get_mut(&local_id) {
                    inst.status = VoucherStatus::Active;
                    inst.voucher.nominal_value.unit = "Minutos".into();
                    inst.voucher.nominal_value.abbreviation = Some("MIN".into());
                }
                // Make an expired clone of the own test voucher + corrupt-date clone (both Archived)
                // plus archive the original test voucher so Loop 2 creates the meta entry.
                let mut archived_test = w
                    .voucher_store
                    .vouchers
                    .get(&test_local_id)
                    .unwrap()
                    .clone();
                archived_test.status = VoucherStatus::Archived;
                w.voucher_store
                    .vouchers
                    .insert(test_local_id.clone(), archived_test.clone());

                let mut expired_v = archived_test.clone();
                expired_v.status = VoucherStatus::Expired;
                expired_v.voucher.voucher_id = "expired-vid".into();
                w.voucher_store
                    .vouchers
                    .insert("expired-local".into(), expired_v);

                let mut corrupt_date_v = archived_test;
                corrupt_date_v.status = VoucherStatus::Archived;
                corrupt_date_v.voucher.voucher_id = "corrupt-date-vid".into();
                corrupt_date_v.voucher.valid_until = "not-an-rfc3339-date".into();
                w.voucher_store
                    .vouchers
                    .insert("corrupt-date-local".into(), corrupt_date_v);

                calculate_aggregated_balances(&w, id)
            })
            .unwrap();

        // Minuto must have a SINGLE entry (keyed by unit "Minutos", NOT duplicated under "MIN")
        let minuto_entries: Vec<_> = balances
            .iter()
            .filter(|b| b.standard_uuid == voucher.voucher_standard.uuid)
            .collect();
        assert_eq!(
            minuto_entries.len(),
            1,
            "Loop 1 and Loop 2 must share the exact same unit key; got: {:?}",
            minuto_entries
        );
        assert_eq!(minuto_entries[0].own_available_amount, "60");
        assert_eq!(minuto_entries[0].own_circulating_amount, "0");

        // Test voucher: must use "TEST-" hyphen prefix, and exclude Expired + unparseable valid_until
        let test_entries: Vec<_> = balances
            .iter()
            .filter(|b| b.standard_uuid == test_v.voucher_standard.uuid && b.is_test_voucher)
            .collect();
        assert_eq!(test_entries.len(), 1);
        assert!(
            test_entries[0].display_currency.starts_with("TEST-"),
            "Expected TEST- hyphen prefix, got: {}",
            test_entries[0].display_currency
        );
        assert!(
            test_entries[0].display_standard_name.starts_with("TEST-"),
            "Expected TEST- hyphen prefix, got: {}",
            test_entries[0].display_standard_name
        );
        assert_eq!(
            test_entries[0].own_circulating_amount, "50",
            "Expired and corrupt-date vouchers must NOT inflate own_circulating_amount"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
