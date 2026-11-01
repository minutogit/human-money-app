//! Recipient-owned cold authority and one-gateway productive checkout.
use crate::{AppState,settings::{AppSettings,RegionalSourceDto},models::FrontendError};
use human_money_core::humoco_proto::mesh::{RegionalMeshPolicy,RegionalMeshView,regional_committee};
use std::collections::HashMap;

const REGIONAL_VIEW_RENEWAL_WINDOW_MS:u64=10_000;

#[derive(Clone)]
pub(crate) struct RegionalViewCache {
    pub owner:String,
    pub policy:RegionalMeshPolicy,
    pub views:Vec<RegionalMeshView>,
    pub expires_at_ms:u64,
}
impl RegionalViewCache {
    fn is_expired(&self,now_ms:u64)->bool {now_ms>=self.expires_at_ms}
    fn is_expiring_within(&self,now_ms:u64,window_ms:u64)->bool {self.expires_at_ms<=now_ms.saturating_add(window_ms)}
}
fn fail(message:&str)->FrontendError {FrontendError::from(message.to_string())}
fn now()->Result<u64,FrontendError> {u64::try_from(chrono::Utc::now().timestamp_millis()).map_err(|_|fail("Invalid verification time"))}
pub(crate) fn configured_policy(settings:&AppSettings)->Result<Option<RegionalMeshPolicy>,FrontendError> {
    if settings.regional_mesh_sources.is_empty() {return Ok(None);}
    if settings.regional_mesh_sources.len()>8 {return Err(fail("Regional source limit exceeded"));}
    let network=match settings.active_network.to_lowercase().as_str() {"mainnet"=>0,"testnet"=>1,_=>return Err(fail("Invalid regional network"))};
    let mut keys=Vec::new();
    for source in &settings.regional_mesh_sources {
        if source.url.len()>2048 || (crate::gateway_resolver::endpoint(&source.url,"v1/regional-mesh-view").is_none() && crate::gateway_resolver::endpoint(&source.url,"v1/operator/regional-mesh-view").is_none()) {return Err(fail("Invalid regional source URL"));}
        let key = if source.public_key.len()==64 {human_money_core::hex::decode(&source.public_key).map_err(|_|fail("Invalid source identity"))?.try_into().map_err(|_|fail("Invalid source identity"))?}
            else {human_money_core::services::crypto::decode_bs58_fixed::<32>(&source.public_key,"regional_source").map_err(FrontendError::from)?};
        keys.push(key);
    }
    let policy=RegionalMeshPolicy {network,source_keys:keys}; policy.validate().map_err(fail)?;
    Ok(Some(policy))
}
/// Cold path only. Tokens are attached solely to the explicitly configured
/// source URL; redirects are forbidden. All signatures are verified before use.
pub(crate) async fn fetch_regional_views(sources:&[RegionalSourceDto],policy:&RegionalMeshPolicy)->Result<Vec<RegionalMeshView>,FrontendError> {
    let client=reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().map_err(|_|fail("HTTP client unavailable"))?;
    let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(2);
    let mut views=Vec::new(); let mut used=0usize;
    for source in sources {
        let url=crate::gateway_resolver::endpoint(&source.url,"v1/regional-mesh-view").ok_or_else(||fail("Invalid regional source"))?;
        let mut request=client.get(url);
        if let Some(token)=&source.bearer_token {request=request.bearer_auth(token);}
        let mut response=tokio::time::timeout_at(deadline,request.send()).await.map_err(|_|fail("Regional source timeout"))?.map_err(|_|fail("Regional source unavailable"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            if let Some(op_url) = crate::gateway_resolver::endpoint(&source.url, "v1/operator/regional-mesh-view") {
                let mut op_req = client.get(op_url);
                if let Some(token) = &source.bearer_token {
                    op_req = op_req.bearer_auth(token);
                }
                if let Ok(Ok(op_resp)) = tokio::time::timeout_at(deadline, op_req.send()).await {
                    response = op_resp;
                }
            }
        }
        if !response.status().is_success() {return Err(fail("Regional source rejected request"));}
        let bytes=tokio::time::timeout_at(deadline,crate::gateway_resolver::bounded_body(response,2*1024*1024)).await
            .map_err(|_|fail("Regional source timeout"))?.ok_or_else(||fail("Regional view ceiling exceeded"))?;
        used=used.saturating_add(bytes.len()); if used>4*1024*1024 {return Err(fail("Regional cache ceiling exceeded"));}
        views.push(serde_json::from_slice(&bytes).map_err(|_|fail("Invalid regional view"))?);
    }
    // Conservative cache admission checks all configured sources. Every actual
    // purchase independently re-derives its own shard committee again in Core.
    regional_committee(policy,&views,0,now()?).map_err(fail)?;
    Ok(views)
}
pub(crate) async fn refresh_views(state:&AppState)->Result<bool,FrontendError> {
    let settings=state.get_cached_settings().map_err(FrontendError::from)?;
    let Some(policy)=configured_policy(&settings)? else {return Ok(false);};
    let owner={let service=state.service.lock().map_err(|_|fail("Wallet service unavailable"))?;
        service.with_wallet(|wallet|(wallet.get_user_id().to_string(),wallet.profile.l2_regional_mesh_policy.clone())).map_err(FrontendError::from)?};
    if owner.1.as_ref()!=Some(&policy) {return Err(fail("Regional source configuration is not authenticated"));}
    let views=fetch_regional_views(&settings.regional_mesh_sources,&policy).await?;
    let expires_at_ms=views.iter().map(|view|view.valid_until_ms).min().ok_or_else(||fail("Regional view expiry unavailable"))?;
    let service=state.service.lock().map_err(|_|fail("Wallet service unavailable"))?;
    let current=service.with_wallet(|wallet|(wallet.get_user_id().to_string(),wallet.profile.l2_regional_mesh_policy.clone())).map_err(FrontendError::from)?;
    if current!=owner {return Err(fail("Wallet authority changed during refresh"));}
    let current_settings=state.get_cached_settings().map_err(FrontendError::from)?;
    if configured_policy(&current_settings)?.as_ref()!=Some(&policy) {return Err(fail("Source settings changed during refresh"));}
    *state.regional_views.lock().map_err(|_|fail("Regional cache unavailable"))?=Some(RegionalViewCache {owner:owner.0,policy,views,expires_at_ms});
    Ok(true)
}
pub(crate) fn cached_views(state:&AppState)->Result<(String,Vec<RegionalMeshView>),FrontendError> {
    let service=state.service.lock().map_err(|_|fail("Wallet service unavailable"))?;
    let cache=state.regional_views.lock().map_err(|_|fail("Regional cache unavailable"))?;
    let cache=cache.as_ref().ok_or_else(||fail("Authenticated regional view cache unavailable"))?;
    if cache.is_expired(now()?) {return Err(fail("Authenticated regional view cache expired"));}
    let owner=service.with_wallet(|wallet|(wallet.get_user_id().to_string(),wallet.profile.l2_regional_mesh_policy.clone())).map_err(FrontendError::from)?;
    if cache.owner!=owner.0 || owner.1.as_ref()!=Some(&cache.policy) {return Err(fail("Regional cache belongs to another authority"));}
    Ok((owner.0,cache.views.clone()))
}
pub(crate) async fn ensure_cached_views(state:&AppState)->Result<(String,Vec<RegionalMeshView>),FrontendError> {
    let now_ms=now()?;
    let needs_refresh={
        let cache=state.regional_views.lock().map_err(|_|fail("Regional cache unavailable"))?;
        match cache.as_ref() {
            None=>true,
            Some(cache)=>cache.is_expiring_within(now_ms,REGIONAL_VIEW_RENEWAL_WINDOW_MS),
        }
    };
    if needs_refresh {
        if let Err(error)=refresh_views(state).await {log::warn!("Regional view cache refresh failed: {}",error.message);}
    }
    cached_views(state)
}
#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all="camelCase")]
pub struct PurchaseCompletion {
    pub purchase_id: String,
    pub newly_committed: bool,
    pub verdict: crate::models::FrontendCheckoutReleaseVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receive_payload: Option<crate::models::ReceiveSuccessPayload>,
}

fn format_bff_name(raw_name: &str, is_test: bool) -> String {
    if is_test {
        if raw_name.starts_with("TEST-") {
            raw_name.to_string()
        } else {
            format!("TEST-{}", raw_name)
        }
    } else {
        raw_name.to_string()
    }
}

#[allow(clippy::too_many_arguments)]
fn build_receive_payload_and_record(
    state: &AppState,
    service: &mut human_money_core::app_service::AppService,
    purchase_id: &str,
    bundle_data: &[u8],
    standards: &HashMap<String, String>,
    newly_committed: bool,
    verdict: crate::models::FrontendCheckoutReleaseVerdict,
    password: Option<&str>,
) -> Result<(crate::models::ReceiveSuccessPayload, Option<crate::models::TransactionRecord>), FrontendError> {
    let (opened_bundle, recipient_id) = service.with_wallet_and_identity(|wallet, identity| {
        let bundle = human_money_core::services::bundle_processor::open_and_verify_bundle(identity, bundle_data)?;
        Ok::<_, human_money_core::Error>((bundle, wallet.get_user_id().to_string()))
    }).map_err(FrontendError::from)?
    .map_err(FrontendError::from)?;

    let registry = human_money_core::services::voucher_validation::StandardRegistry::from_toml_map(standards)
        .map_err(FrontendError::from)?;

    let mut transfer_summary = human_money_core::wallet::types::TransferSummary::default();
    let mut involved_vouchers_ids = Vec::new();
    let mut involved_vouchers_details = Vec::new();

    for voucher in &opened_bundle.vouchers {
        let local_id = human_money_core::wallet::Wallet::calculate_local_instance_id(voucher, &recipient_id)
            .map_err(FrontendError::from)?;
        involved_vouchers_ids.push(local_id.clone());

        let last_tx = voucher.transactions.last().ok_or_else(|| {
            fail("purchase transaction missing")
        })?;

        let display_currency = format_bff_name(
            voucher
                .nominal_value
                .abbreviation
                .as_deref()
                .unwrap_or(&voucher.nominal_value.unit),
            voucher.non_redeemable_test_voucher,
        );

        let standard_opt = registry.get(&voucher.voucher_standard.uuid);
        let balances_are_summable = standard_opt.map(|s| s.immutable.features.balances_are_summable).unwrap_or(true);
        let standard_name = standard_opt.map(|s| s.immutable.identity.name.clone()).unwrap_or_else(|| voucher.voucher_standard.name.clone());
        let allow_partial = standard_opt.map(|s| s.immutable.features.allow_partial_transfers).unwrap_or(false);

        if balances_are_summable {
            transfer_summary
                .summable_amounts
                .insert(display_currency.clone(), last_tx.amount.clone());
        } else {
            transfer_summary
                .countable_items
                .insert(display_currency.clone(), 1);
        }

        involved_vouchers_details.push(human_money_core::wallet::types::InvolvedVoucherInfo {
            local_instance_id: local_id,
            voucher_id: voucher.voucher_id.clone(),
            standard_name,
            unit: voucher.nominal_value.unit.clone(),
            amount: last_tx.amount.clone(),
            allow_partial_transfers: allow_partial,
            is_test_voucher: voucher.non_redeemable_test_voucher,
            display_currency,
            display_standard_name: format_bff_name(
                &voucher.voucher_standard.name,
                voucher.non_redeemable_test_voucher,
            ),
            counterparty_id: Some(opened_bundle.sender_id.clone()),
            counterparty_name: opened_bundle.sender_profile_name.clone(),
        });
    }

    let conflict_summaries = service.list_conflicts().map_err(FrontendError::from)?.into_iter().map(|s| s.into()).collect();

    let record = crate::models::TransactionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        direction: "received".to_string(),
        recipient_id,
        sender_id: opened_bundle.sender_id.clone(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        summable_amounts: transfer_summary.summable_amounts.clone(),
        countable_items: transfer_summary.countable_items.clone(),
        involved_vouchers: involved_vouchers_ids.clone(),
        involved_sources_details: Some(involved_vouchers_details.clone()),
        bundle_data: Vec::new(),
        bundle_id: opened_bundle.bundle_id.clone(),
        notes: opened_bundle.notes.clone(),
        sender_profile_name: opened_bundle.sender_profile_name.clone(),
    };

    let mut wot_imported_lists = None;
    if newly_committed {
        if let Some(ref wot_json) = opened_bundle.wot_sync_json {
            wot_imported_lists = state.try_import_piggybacked_wot(service, wot_json, &opened_bundle.sender_id, password);
        }
    }

    let payload = crate::models::ReceiveSuccessPayload {
        sender_id: opened_bundle.sender_id,
        sender_profile_name: opened_bundle.sender_profile_name,
        notes: opened_bundle.notes,
        transfer_summary: transfer_summary.into(),
        involved_vouchers: involved_vouchers_ids,
        involved_vouchers_details: involved_vouchers_details.into_iter().map(|d| d.into()).collect(),
        verifiable_conflicts: Default::default(),
        conflict_summaries,
        checkout_verdict: Some(verdict),
        purchase_id: Some(purchase_id.to_string()),
        newly_committed: Some(newly_committed),
        is_replay: Some(!newly_committed),
        wot_imported_lists,
    };

    Ok((payload, Some(record)))
}

/// Host orchestration uses the exact stored recipient nonce. No network request
/// for membership is made on this hot path; Core revalidates the cold authority.
pub(crate) async fn checkout_bundle(state:&AppState,purchase_id:&str,bundle:&[u8],standards:&HashMap<String,String>,password:Option<&str>)->Result<PurchaseCompletion,FrontendError> {
    // Physical release idempotency & offline replay check:
    // If the purchase is already durably completed in the wallet, return
    // newly_committed=false with authorized verdict WITHOUT requiring a new network proof.
    {
        let mut service = state.service.lock().map_err(|_| fail("Wallet service unavailable"))?;
        if let Some(completion) = service.replay_checkout_purchase(purchase_id, bundle).map_err(FrontendError::from)? {
            log::info!("Purchase {} already completed; returning idempotent replay result without network request", purchase_id);
            let receive_payload = build_receive_payload_and_record(
                state,
                &mut service,
                purchase_id,
                bundle,
                standards,
                false,
                completion.verdict.clone().into(),
                password,
            ).map(|(payload, _)| payload).ok();

            return Ok(PurchaseCompletion {
                purchase_id: completion.purchase_id,
                newly_committed: completion.newly_committed,
                verdict: completion.verdict.into(),
                receive_payload,
            });
        }
    }

    let settings = state.get_cached_settings().map_err(FrontendError::from)?;
    let candidates = crate::gateway_resolver::cached_candidates(&settings);
    if settings.regional_mesh_sources.is_empty() || candidates.is_empty() {
        return Err(FrontendError::from("pos.noServersConfigured".to_string()));
    }

    let (owner,views)=ensure_cached_views(state).await?;
    let checkout_req = {
        let mut service = state.service.lock().map_err(|_| fail("Wallet service unavailable"))?;
        service.prepare_checkout_request(purchase_id, bundle, &views, now()?, password).map_err(FrontendError::from)?
    };

    let checkout_resp = match crate::gateway_resolver::fetch_checkout_proof(&settings, &candidates, &checkout_req).await {
        Ok(Some(resp)) => resp,
        _ => return Err(FrontendError::from("pos.quorumUnavailable".to_string())),
    };

    let mut service = state.service.lock().map_err(|_| fail("Wallet service unavailable"))?;
    if service.with_wallet(|wallet| wallet.get_user_id().to_string()).map_err(FrontendError::from)? != owner {
        return Err(fail("Wallet changed during checkout"));
    }

    if let Some(ref conflict_entry) = checkout_resp.conflicting_lock {
        let conflict_recorded = service.record_checkout_upstream_conflict(bundle, conflict_entry, password).unwrap_or(false);
        if checkout_resp.proof.occupancy == human_money_core::humoco_proto::live::LiveSlotOccupancy::Unoccupied && conflict_recorded {
            return Ok(PurchaseCompletion {
                purchase_id: purchase_id.to_string(),
                newly_committed: false,
                verdict: crate::models::FrontendCheckoutReleaseVerdict {
                    outcome: "Conflict".to_string(),
                    can_release_goods_automatically: false,
                    requires_merchant_approval: false,
                    is_goods_release_authorized: false,
                    merchant_warning: Some("Upstream double-spend conflict proven on Layer 2".to_string()),
                },
                receive_payload: None,
            });
        }
    }

    if checkout_resp.proof.occupancy == human_money_core::humoco_proto::live::LiveSlotOccupancy::Unoccupied {
        return Err(FrontendError::from("pos.quorumUnavailable".to_string()));
    }

    let completion = service.complete_checkout_purchase(purchase_id, bundle, &checkout_resp.proof, &views, standards, now()?, password).map_err(FrontendError::from)?;
    if let Err(error) = state.refresh_events_cache(&mut service, password) { log::warn!("Checkout committed; event cache refresh failed: {}", error); }

    let (receive_payload, record_opt) = build_receive_payload_and_record(
        state,
        &mut service,
        purchase_id,
        bundle,
        standards,
        completion.newly_committed,
        completion.verdict.clone().into(),
        password,
    )?;

    if completion.newly_committed {
        if let Some(record) = record_opt {
            if let Err(e) = state.append_to_history(&mut service, record, password) {
                log::warn!("Checkout committed; transaction history recording failed: {}", e);
            }
        }
    }

    Ok(PurchaseCompletion {
        purchase_id: completion.purchase_id,
        newly_committed: completion.newly_committed,
        verdict: completion.verdict.into(),
        receive_payload: Some(receive_payload),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use human_money_core::app_service::AppService;
    use std::path::Path;
    use std::sync::Mutex;

    fn cache(expires_at_ms:u64)->RegionalViewCache {
        RegionalViewCache {
            owner:"owner".to_string(),
            policy:RegionalMeshPolicy {network:0,source_keys:Vec::new()},
            views:Vec::new(),
            expires_at_ms,
        }
    }

    fn test_state(cache:Option<RegionalViewCache>,path:&Path)->AppState {
        AppState {
            service:Mutex::new(AppService::new(path).unwrap()),
            history:Mutex::new(None),
            events:Mutex::new(None),
            contacts:Mutex::new(None),
            settings:Mutex::new(None),
            regional_views:Mutex::new(cache),
            wot: Mutex::new(None),
        }
    }

    #[test]
    fn cached_views_rejects_expired_cache() {
        let path=std::env::temp_dir().join(format!("human-money-app-checkout-{}",uuid::Uuid::new_v4()));
        let state=test_state(Some(cache(0)),&path);
        let error=cached_views(&state).expect_err("expired cache must be rejected");
        assert_eq!(error.message,"Authenticated regional view cache expired");
        drop(state);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn is_expiring_within_detects_impending_expiry() {
        let cache=cache(15_000);
        assert!(cache.is_expiring_within(5_000,10_000));
        assert!(!cache.is_expiring_within(4_999,10_000));
    }

    #[tokio::test]
    async fn checkout_bundle_rejects_when_no_servers_configured() {
        let path = std::env::temp_dir().join(format!("human-money-app-checkout-test-{}", uuid::Uuid::new_v4()));
        let mut service = AppService::new(&path).unwrap();
        const PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        service.create_profile(
            "Alice",
            PHRASE,
            None,
            Some("pc"),
            "password123",
            human_money_core::MnemonicLanguage::English,
            "device-pos-test".into(),
        ).unwrap();
        let state = AppState {
            service: Mutex::new(service),
            history: Mutex::new(None),
            events: Mutex::new(None),
            contacts: Mutex::new(None),
            settings: Mutex::new(Some(AppSettings::default())),
            regional_views: Mutex::new(None),
            wot: Mutex::new(None),
        };
        let error = checkout_bundle(&state, "p1", b"fake_bundle", &HashMap::new(), None).await.expect_err("must fail");
        assert_eq!(error.message, "pos.noServersConfigured");
        drop(state);
        let _ = std::fs::remove_dir_all(path);
    }
}
