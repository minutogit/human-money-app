//! Recipient-owned cold authority and one-gateway productive checkout.
use crate::{AppState,settings::{AppSettings,RegionalSourceDto},models::FrontendError};
use human_money_core::humoco_proto::{mesh::{RegionalMeshPolicy,RegionalMeshView,regional_committee},live::LiveStateProof};
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
        if source.url.len()>2048 || crate::gateway_resolver::endpoint(&source.url,"v1/operator/regional-mesh-view").is_none() {return Err(fail("Invalid regional source URL"));}
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
        let url=crate::gateway_resolver::endpoint(&source.url,"v1/operator/regional-mesh-view").ok_or_else(||fail("Invalid regional source"))?;
        let mut request=client.get(url);
        if let Some(token)=&source.bearer_token {request=request.bearer_auth(token);}
        let response=tokio::time::timeout_at(deadline,request.send()).await.map_err(|_|fail("Regional source timeout"))?.map_err(|_|fail("Regional source unavailable"))?;
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
#[derive(serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct PurchaseCompletion {
    pub purchase_id:String,
    pub newly_committed:bool,
    pub verdict:crate::models::FrontendCheckoutReleaseVerdict,
}
/// Host orchestration uses the exact stored recipient nonce. No network request
/// for membership is made on this hot path; Core revalidates the cold authority.
pub(crate) async fn checkout_bundle(state:&AppState,purchase_id:&str,bundle:&[u8],standards:&HashMap<String,String>,password:Option<&str>)->Result<PurchaseCompletion,FrontendError> {
    // Physical release idempotency & offline replay check:
    // If the purchase is already durably completed in the wallet, return
    // newly_committed=false with authorized verdict WITHOUT requiring a new network proof.
    {
        let service = state.service.lock().map_err(|_| fail("Wallet service unavailable"))?;
        if let Some(completion) = service.replay_checkout_purchase(purchase_id, bundle).map_err(FrontendError::from)? {
            log::info!("Purchase {} already completed; returning idempotent replay result without network request", purchase_id);
            return Ok(PurchaseCompletion {
                purchase_id: completion.purchase_id,
                newly_committed: completion.newly_committed,
                verdict: completion.verdict.into(),
            });
        }
    }

    let (owner,views)=ensure_cached_views(state).await?;
    let context={let mut service=state.service.lock().map_err(|_|fail("Wallet service unavailable"))?;
        service.begin_checkout_purchase(purchase_id,bundle,&views,now()?,password).map_err(FrontendError::from)?};
    let settings=state.get_cached_settings().map_err(FrontendError::from)?;
    let candidates=crate::gateway_resolver::cached_candidates(&settings);
    let proof:LiveStateProof=crate::gateway_resolver::fetch_live_proof(&candidates,&context).await.map_err(|_|fail("Live verification unavailable"))?
        .ok_or_else(||fail("Payment remains pending: current quorum unavailable"))?;
    let mut service=state.service.lock().map_err(|_|fail("Wallet service unavailable"))?;
    if service.with_wallet(|wallet|wallet.get_user_id().to_string()).map_err(FrontendError::from)?!=owner {return Err(fail("Wallet changed during checkout"));}
    let completion=service.complete_checkout_purchase(purchase_id,bundle,&proof,&views,standards,now()?,password).map_err(FrontendError::from)?;
    if let Err(error)=state.refresh_events_cache(&mut service,password) {log::warn!("Checkout committed; event cache refresh failed: {}",error);}
    Ok(PurchaseCompletion {purchase_id:completion.purchase_id,newly_committed:completion.newly_committed,verdict:completion.verdict.into()})
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
}
