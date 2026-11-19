// src-tauri/src/commands/profile.rs
use crate::models::{FrontendError, FrontendQuickContact};
use crate::AppState;

#[tauri::command]
pub fn get_my_quick_contact(
    state: tauri::State<AppState>,
) -> Result<FrontendQuickContact, FrontendError> {
    get_my_quick_contact_impl(&state)
}

fn compact_sync_url_for_contact_uri(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_https = if let Some(rest) = trimmed.strip_prefix("https://") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("HTTPS://") {
        rest
    } else {
        trimmed
    };
    let no_trailing = without_https.trim_end_matches('/');
    no_trailing
        .strip_suffix("/sync.php")
        .unwrap_or(no_trailing)
        .trim_end_matches('/')
        .to_string()
}

pub fn get_my_quick_contact_impl(state: &AppState) -> Result<FrontendQuickContact, FrontendError> {
    let mut service = state.authorize_and_lock(crate::state::RequiredAccess::Unlocked, None)?;
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

    let can_did = crate::state::canonicalize_did(&raw_did)
        .map_err(|e| FrontendError::from(format!("Invalid own user ID: {}", e)))?;
    let trimmed_raw = raw_did.trim();
    let own_did = if let Some((prefix_part, _)) = trimmed_raw.rsplit_once('@') {
        format!("{}@{}", prefix_part, can_did)
    } else {
        can_did
    };

    let policy = state
        .get_or_load_security_policy(&mut service, None)
        .map_err(FrontendError::from)?;
    let mut sync_urls = Vec::new();
    for s in policy.market_selected_syncs {
        if !s.write_url.is_empty()
            && (s.write_url.starts_with("https://") || s.write_url.starts_with("http://"))
        {
            let compact = compact_sync_url_for_contact_uri(&s.write_url);
            if !compact.is_empty() && !sync_urls.contains(&compact) {
                sync_urls.push(compact);
                if sync_urls.len() >= 2 {
                    break;
                }
            }
        }
    }
    if sync_urls.len() < 2 {
        for ps in policy.public_syncs {
            if ps.enabled
                && !ps.write_url.is_empty()
                && (ps.write_url.starts_with("https://") || ps.write_url.starts_with("http://"))
            {
                let compact = compact_sync_url_for_contact_uri(&ps.write_url);
                if !compact.is_empty() && !sync_urls.contains(&compact) {
                    sync_urls.push(compact);
                    if sync_urls.len() >= 2 {
                        break;
                    }
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

/// Encodes a short arbitrary text payload (e.g. a humoco://pay URI or base64 bundle)
/// into a crisp black-and-white QR code SVG string using humoco_market.
#[tauri::command]
pub fn encode_text_qr_svg(text: String) -> Result<String, FrontendError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(FrontendError::from(
            "QR payload must not be empty".to_string(),
        ));
    }
    if trimmed.len() > humoco_market::MAX_QR_SHARE_BYTES {
        return Err(FrontendError::from(format!(
            "QR payload exceeds maximum QR capacity of {} bytes",
            humoco_market::MAX_QR_SHARE_BYTES
        )));
    }
    humoco_market::encode_qr_svg(trimmed.as_bytes()).map_err(FrontendError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_my_quick_contact_formats_uri_and_svg() {
        let temp_dir = std::env::temp_dir().join(format!(
            "humoco_quick_contact_test_{}",
            uuid::Uuid::new_v4()
        ));
        let password = "test-password-123";
        let mut alice_service = crate::test_support::setup_test_app(
            &temp_dir,
            "Alice Schnell",
            crate::test_support::ALICE_PHRASE,
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

        let state = crate::test_support::create_test_state(alice_service);

        // Add both a selected friend/own sync and a public sync with distinct read_url vs write_url
        {
            let mut s = state.security_policy.lock().unwrap();
            *s = Some(crate::settings::SecurityPolicy {
                market_selected_syncs: vec![crate::models::FrontendSyncDescriptor {
                    write_url: "https://own.example.com/sync.php".to_string(),
                    read_url: "https://own.example.com/d-own12345".to_string(),
                    drop_pub: "11".repeat(32),
                    status: "active".to_string(),
                    ..Default::default()
                }],
                public_syncs: vec![crate::settings::StoredPublicSync {
                    write_url: "https://sync.example.com/sync.php".to_string(),
                    read_url: "https://sync.example.com/d-pub67890".to_string(),
                    access_pubkey: "abc".to_string(),
                    drop_pub: "def".to_string(),
                    enabled: true,
                    authorized_channels: vec!["card".to_string()],
                    register_url: None,
                    ..Default::default()
                }],
                ..Default::default()
            });
        }

        let quick = get_my_quick_contact_impl(&state).expect("get_my_quick_contact should succeed");

        assert!(
            quick.did.starts_with("pc:") && quick.did.contains("@did:key:"),
            "Expected subaccount prefix from setup_test_app in quick.did, got {}",
            quick.did
        );
        assert_eq!(quick.name, "Alice Schnell");
        assert_eq!(
            quick.sync_urls,
            vec![
                "own.example.com".to_string(),
                "sync.example.com".to_string(),
            ],
            "QuickContact must export compact write_url without https:// and /sync.php"
        );
        assert!(quick.uri.starts_with("humoco://contact?"));
        assert!(quick.uri.contains("did=pc%3A") && quick.uri.contains("%40did%3Akey%3A"));
        assert!(quick.uri.contains("name=Alice+Schnell"));
        assert!(quick.uri.contains("sync=own.example.com"));
        assert!(quick.uri.contains("sync=sync.example.com"));
        assert!(!quick.uri.contains("d-own12345"));
        assert!(!quick.uri.contains("d-pub67890"));
        assert!(quick.qr_svg.is_some());
        let svg = quick.qr_svg.unwrap();
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn test_encode_text_qr_svg_valid_and_limits() {
        let valid = encode_text_qr_svg("humoco://pay?to=did:key:z6MkpAlice&amount=10".to_string())
            .expect("valid URI should encode as QR SVG");
        assert!(valid.contains("<svg"));

        let empty_err = encode_text_qr_svg("   ".to_string()).unwrap_err();
        assert!(empty_err.message.contains("empty"));

        let oversized = "x".repeat(humoco_market::MAX_QR_SHARE_BYTES + 1);
        let size_err = encode_text_qr_svg(oversized).unwrap_err();
        assert!(size_err.message.contains("capacity"));
    }

    #[test]
    fn test_get_my_quick_contact_preserves_subaccount_prefix() {
        let temp_dir =
            std::env::temp_dir().join(format!("humoco_quick_sub_test_{}", uuid::Uuid::new_v4()));
        let password = "test-password-123";
        let mut service = human_money_core::app_service::AppService::new(&temp_dir).unwrap();
        service
            .create_profile(
                "Alice Laden",
                crate::test_support::ALICE_PHRASE,
                None,
                Some("laden"),
                password,
                human_money_core::MnemonicLanguage::English,
                "inst-sub-1".to_string(),
                true,
            )
            .unwrap();
        service.unlock_session(password, 3600).unwrap();

        let state = crate::test_support::create_test_state(service);
        let quick =
            get_my_quick_contact_impl(&state).expect("subaccount quick contact should succeed");
        assert!(
            quick.did.starts_with("laden:"),
            "Expected subaccount prefix in quick.did, got {}",
            quick.did
        );
        assert!(quick.did.contains("@did:key:"));
        assert!(
            quick.uri.contains("did=laden%3A"),
            "Expected URL-encoded prefix in URI, got {}",
            quick.uri
        );
        let _ = std::fs::remove_dir_all(temp_dir);

        // Also explicitly verify a pure root profile (prefix = None) exports bare did:key:...
        let root_dir =
            std::env::temp_dir().join(format!("humoco_quick_root_test_{}", uuid::Uuid::new_v4()));
        let mut root_service = human_money_core::app_service::AppService::new(&root_dir).unwrap();
        root_service
            .create_profile(
                "Alice Pure Root",
                crate::test_support::ALICE_PHRASE,
                None,
                None,
                password,
                human_money_core::MnemonicLanguage::English,
                "inst-root-1".to_string(),
                true,
            )
            .unwrap();
        root_service.unlock_session(password, 3600).unwrap();
        let root_state = crate::test_support::create_test_state(root_service);
        let root_quick =
            get_my_quick_contact_impl(&root_state).expect("root quick contact should succeed");
        assert!(
            root_quick.did.starts_with("did:key:"),
            "Root profile must export bare did:key, got {}",
            root_quick.did
        );
        let _ = std::fs::remove_dir_all(root_dir);
    }
}
