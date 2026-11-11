// src-tauri/src/commands/profile.rs
use crate::models::{FrontendError, FrontendQuickContact};
use crate::AppState;

#[tauri::command]
pub fn get_my_quick_contact(state: tauri::State<AppState>) -> Result<FrontendQuickContact, FrontendError> {
    get_my_quick_contact_impl(&state)
}

pub fn get_my_quick_contact_impl(state: &AppState) -> Result<FrontendQuickContact, FrontendError> {
    let service = state.service.lock().unwrap();
    let (raw_did, full_name) = service
        .with_wallet(|w| {
            let did = w.get_user_id().to_string();
            let p = w.profile.to_public_profile();
            let mut parts = Vec::new();
            if let Some(ref f) = p.first_name {
                let trimmed = f.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
            if let Some(ref l) = p.last_name {
                let trimmed = l.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
            if parts.is_empty() {
                if let Some(ref org) = p.organization {
                    let trimmed = org.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                }
            }
            (did, parts.join(" "))
        })
        .map_err(FrontendError::from)?;

    let own_did = crate::state::canonicalize_did(&raw_did).unwrap_or(raw_did);

    let settings = state.get_cached_settings().unwrap_or_default();
    let mut sync_urls = Vec::new();
    for s in settings.market_selected_syncs {
        if !s.read_url.is_empty()
            && (s.read_url.starts_with("https://") || s.read_url.starts_with("http://"))
            && !sync_urls.contains(&s.read_url)
        {
            sync_urls.push(s.read_url);
            if sync_urls.len() >= 2 {
                break;
            }
        }
    }
    if sync_urls.len() < 2 {
        for ps in settings.public_syncs {
            if ps.enabled
                && !ps.read_url.is_empty()
                && (ps.read_url.starts_with("https://") || ps.read_url.starts_with("http://"))
                && !sync_urls.contains(&ps.read_url)
            {
                sync_urls.push(ps.read_url);
                if sync_urls.len() >= 2 {
                    break;
                }
            }
        }
    }

    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("did", &own_did);
    if !full_name.is_empty() {
        serializer.append_pair("name", &full_name);
    }
    for u in &sync_urls {
        serializer.append_pair("sync", u);
    }
    let query = serializer.finish();
    let uri = format!("humoco://contact?{}", query);

    let qr_svg = humoco_market::encode_qr_svg(uri.as_bytes()).ok();

    Ok(FrontendQuickContact {
        did: own_did,
        name: full_name,
        sync_urls,
        uri,
        qr_svg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_my_quick_contact_formats_uri_and_svg() {
        let temp_dir = std::env::temp_dir().join(format!("humoco_quick_contact_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let mut alice_service = crate::commands::sync::tests::setup_test_app(
            &temp_dir,
            "Alice Schnell",
            crate::commands::sync::tests::ALICE_PHRASE,
            password,
        );

        alice_service
            .update_public_profile(
                human_money_core::models::profile::PublicProfile {
                    first_name: Some("Alice".to_string()),
                    last_name: Some("Schnell".to_string()),
                    ..Default::default()
                },
                Some(password),
            )
            .unwrap();

        let state = crate::commands::sync::tests::create_test_state(alice_service);

        // Add a public sync to cached settings
        {
            let mut s = state.settings.lock().unwrap();
            *s = Some(crate::settings::AppSettings {
                public_syncs: vec![crate::settings::StoredPublicSync {
                    write_url: "https://sync.example.com/sync.php".to_string(),
                    read_url: "https://sync.example.com/sync.php".to_string(),
                    access_pubkey: "abc".to_string(),
                    drop_pub: "def".to_string(),
                    enabled: true,
                    authorized_channels: vec!["card".to_string()],
                    register_url: None,
                }],
                ..Default::default()
            });
        }

        let quick = get_my_quick_contact_impl(&state).expect("get_my_quick_contact should succeed");

        assert!(quick.did.starts_with("did:key:"));
        assert_eq!(quick.name, "Alice Schnell");
        assert_eq!(quick.sync_urls, vec!["https://sync.example.com/sync.php".to_string()]);
        assert!(quick.uri.starts_with("humoco://contact?"));
        assert!(quick.uri.contains("did=did%3Akey%3A"));
        assert!(quick.uri.contains("name=Alice+Schnell"));
        assert!(quick.uri.contains("sync=https%3A%2F%2Fsync.example.com%2Fsync.php"));
        assert!(quick.qr_svg.is_some());
        let svg = quick.qr_svg.unwrap();
        assert!(svg.contains("<svg"));
    }
}
