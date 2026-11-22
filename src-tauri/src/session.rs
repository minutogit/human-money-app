use crate::models::{
    FrontendAddressBook, StoredAppNotices, StoredOutgoingBundles, StoredOwnMarketProfile,
    TransactionRecord,
};
use crate::settings::{InternalLifecycleState, SecurityPolicy, UiPreferences};

/// Generation-pinned identity token captured at the start of an async operation.
///
/// Any post-`.await` mutation must re-verify this token via `AppState::with_session`
/// so logout, profile switch, or same-profile re-login cleanly discards stale results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionToken {
    pub generation: u64,
    pub owner_did: String,
}

/// Authorized session bundle pairing a generation-pinned [`SessionToken`] with an optional
/// zeroizing credential resolved at the Tauri IPC boundary (including consumed one-shot PIN grants).
#[derive(Clone)]
pub struct AuthorizedSession {
    pub token: SessionToken,
    password: Option<zeroize::Zeroizing<String>>,
}

impl std::fmt::Debug for AuthorizedSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizedSession")
            .field("token", &self.token)
            .field("has_password", &self.password.is_some())
            .finish()
    }
}

impl AuthorizedSession {
    pub fn new(token: SessionToken, password: Option<zeroize::Zeroizing<String>>) -> Self {
        Self { token, password }
    }

    pub fn pw(&self) -> Option<&str> {
        self.password.as_deref().map(String::as_str)
    }

    pub fn generation(&self) -> u64 {
        self.token.generation
    }

    pub fn owner_did(&self) -> &str {
        &self.token.owner_did
    }
}

/// Consolidated profile-scoped caches bound to a specific session generation and owner DID.
///
/// Bundling all profile caches under one mutex guarantees atomic clearing on logout/switch and
/// eliminates multi-cache lock-ordering hazards.
#[derive(Clone, Default)]
pub struct ActiveProfileCaches {
    pub generation: u64,
    pub owner_did: String,
    pub history: Option<Vec<TransactionRecord>>,
    pub outgoing_bundles: Option<StoredOutgoingBundles>,
    pub events: Option<Vec<human_money_core::models::wallet_event::WalletEvent>>,
    pub contacts: Option<FrontendAddressBook>,
    pub ui_preferences: Option<UiPreferences>,
    pub security_policy: Option<SecurityPolicy>,
    pub internal_lifecycle: Option<InternalLifecycleState>,
    pub own_market_profile: Option<StoredOwnMarketProfile>,
    pub wot: Option<humoco_wot::TrustRegister>,
    pub recovery_owner: Option<Option<crate::models::recovery::RecoveryOwnerState>>,
    pub guardian_vault: Option<humoco_market::GuardianVaultStore>,
    pub app_notices: Option<StoredAppNotices>,
    pub server_offenders: Option<humoco_wot::ServerOffenderList>,
    pub own_sync_descriptor: Option<Option<humoco_market::DropDescriptor>>,
    pub post_state: Option<crate::state::StoredPostState>,
    pub card_store: Option<humoco_market::CardStore>,
    pub sync_state: Option<crate::state::StoredSyncState>,
    /// Keyed by `(blob_id, normalized_read_url)` storing unix expiration timestamp (`expires_at_secs`).
    pub media_negative_cache: std::collections::HashMap<(String, String), u64>,
}

impl ActiveProfileCaches {
    pub fn new(generation: u64, owner_did: String) -> Self {
        Self {
            generation,
            owner_did,
            ..Default::default()
        }
    }
}
