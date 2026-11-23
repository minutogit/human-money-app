use crate::models::{FrontendError, StoredAppNotices};
use crate::state::AppState;

pub fn get_app_notices_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<StoredAppNotices, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::Unlocked, password.as_deref())?;
    Ok(state.load_app_notices(&mut service, resolved_pw.as_deref().map(String::as_str)))
}

pub fn mark_app_notices_read_impl(
    notice_id: Option<String>,
    password: Option<String>,
    state: &AppState,
) -> Result<StoredAppNotices, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut store: StoredAppNotices = crate::state::load_encrypted_json_or_default(
        &mut service,
        crate::state::APP_NOTICES_KEY,
        pw,
    )
    .map_err(FrontendError::from)?;
    let mut changed = false;
    for notice in &mut store.notices {
        let matches = notice_id
            .as_deref()
            .map(|id| notice.id == id)
            .unwrap_or(true);
        if matches && !notice.read {
            notice.read = true;
            changed = true;
        }
    }
    if changed {
        state.save_app_notices(&mut service, &store, pw)?;
    }
    Ok(store)
}

pub fn clear_app_notices_impl(
    password: Option<String>,
    state: &AppState,
) -> Result<StoredAppNotices, FrontendError> {
    let (mut service, resolved_pw) = state
        .authorize_and_lock_secret(crate::state::RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let mut store: StoredAppNotices = crate::state::load_encrypted_json_or_default(
        &mut service,
        crate::state::APP_NOTICES_KEY,
        pw,
    )
    .map_err(FrontendError::from)?;
    if !store.notices.is_empty() {
        store.notices.clear();
        state.save_app_notices(&mut service, &store, pw)?;
    }
    Ok(store)
}
