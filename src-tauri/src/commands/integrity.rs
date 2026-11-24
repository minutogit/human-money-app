use crate::models::{FrontendError, FrontendProofImportResult, FullProofDetails, IntegrityReport};
use crate::services::balance_service::get_proof_of_double_spend_impl;
use crate::state::RequiredAccess;
use crate::AppState;
use log::{error, info};
use tauri::State;

const MAX_IMPORT_PROOF_BYTES: usize = 2 * 1024 * 1024;

#[tauri::command]
pub async fn check_wallet_integrity(
    state: State<'_, AppState>,
) -> Result<IntegrityReport, FrontendError> {
    info!("Checking wallet integrity...");
    let mut service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
    let report = service
        .check_integrity(None)
        .map_err(|e| {
            error!("Integrity check failed: {}", e);
            e
        })
        .map_err(FrontendError::from)?;

    info!("Integrity report: {:?}", report);
    Ok(report.into())
}

#[tauri::command]
pub async fn repair_wallet_integrity(
    state: State<'_, AppState>,
    password: Option<String>,
) -> Result<(), FrontendError> {
    info!("Repairing wallet integrity...");
    let mut service = state.authorize_and_lock(RequiredAccess::Full, password.as_deref())?;
    service
        .repair_integrity(password.as_deref())
        .map_err(|e| {
            error!("Integrity repair failed: {}", e);
            e
        })
        .map_err(FrontendError::from)?;

    info!("Integrity repair successful.");
    Ok(())
}

#[tauri::command]
pub fn export_double_spend_proof(
    proof_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, FrontendError> {
    info!("Exporting double spend proof: {}", proof_id);
    let service = state.authorize_and_lock(RequiredAccess::Unlocked, None)?;
    let proof = service
        .get_proof_of_double_spend(&proof_id)
        .map_err(FrontendError::from)?;
    let bytes =
        serde_json::to_vec_pretty(&proof).map_err(|e| FrontendError::from(e.to_string()))?;
    Ok(bytes)
}

pub fn import_double_spend_proof_impl(
    proof_bytes: Vec<u8>,
    password: Option<String>,
    state: &AppState,
) -> Result<FrontendProofImportResult, FrontendError> {
    info!(
        "Importing double spend proof ({} bytes)...",
        proof_bytes.len()
    );
    if proof_bytes.len() > MAX_IMPORT_PROOF_BYTES {
        return Err(FrontendError::from(
            "Proof payload exceeds maximum allowed size (2 MiB)".to_string(),
        ));
    }
    let (mut service, resolved_pw) =
        state.authorize_and_lock_secret(RequiredAccess::DailyPay, password.as_deref())?;
    let pw = resolved_pw.as_deref().map(String::as_str);
    let proof: human_money_core::models::conflict::ProofOfDoubleSpend =
        serde_json::from_slice(&proof_bytes).map_err(|e| FrontendError::from(e.to_string()))?;

    if state.cache_wot().is_none() {
        let _ = state.get_or_load_wot_register(&mut service, pw);
    }

    let mut candidate_dids: Vec<String> = proof
        .resolutions
        .as_ref()
        .map(|list| list.iter().map(|r| r.endorser_id.clone()).collect())
        .unwrap_or_default();
    if let Ok(existing) = service.get_proof_of_double_spend(&proof.proof_id) {
        if let Some(existing_res) = existing.resolutions {
            candidate_dids.extend(existing_res.into_iter().map(|r| r.endorser_id));
        }
    }

    let trusted_pks = service
        .with_wallet(|wallet| state.collect_trusted_endorser_pks(wallet, &candidate_dids))
        .map_err(FrontendError::from)?;

    let outcome = service
        .import_proof_with_trust_check(
            proof,
            |did| {
                human_money_core::crypto::get_pubkey_from_user_id(did)
                    .ok()
                    .is_some_and(|pk| trusted_pks.contains(&pk.to_bytes()))
            },
            pw,
        )
        .map_err(FrontendError::from)?;

    info!(
        "Double spend proof imported: {} (already_known={}, added_endorsements={})",
        outcome.proof_id, outcome.already_known, outcome.added_endorsements
    );
    Ok(FrontendProofImportResult {
        proof_id: outcome.proof_id,
        already_known: outcome.already_known,
        added_endorsements: outcome.added_endorsements,
    })
}

#[tauri::command]
pub fn import_double_spend_proof(
    proof_bytes: Vec<u8>,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FrontendProofImportResult, FrontendError> {
    import_double_spend_proof_impl(proof_bytes, password, &state)
}

pub fn endorse_conflict_resolution_impl(
    proof_id: String,
    notes: Option<String>,
    also_set_local_override: bool,
    password: Option<String>,
    state: &AppState,
) -> Result<FullProofDetails, FrontendError> {
    info!(
        "Endorsing conflict resolution for {} (also_set_local_override={})",
        proof_id, also_set_local_override
    );
    {
        let (mut service, resolved_pw) =
            state.authorize_and_lock_secret(RequiredAccess::Full, password.as_deref())?;
        let pw = resolved_pw.as_deref().map(String::as_str);
        service
            .endorse_conflict_resolution(&proof_id, notes, also_set_local_override, pw)
            .map_err(FrontendError::from)?;
    }
    get_proof_of_double_spend_impl(proof_id, state)
}

#[tauri::command]
pub fn endorse_conflict_resolution(
    proof_id: String,
    notes: Option<String>,
    also_set_local_override: bool,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<FullProofDetails, FrontendError> {
    endorse_conflict_resolution_impl(proof_id, notes, also_set_local_override, password, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FrontendContact, FrontendUserProfile};
    use crate::services::settings_service::set_conflict_local_override_impl;
    use crate::test_support::{
        setup_test_app, temp_test_dir, ALICE_PHRASE, BOB_PHRASE, CHARLIE_PHRASE, READER1_PHRASE,
    };
    use human_money_core::models::conflict::{ProofOfDoubleSpend, ResolutionEndorsement};
    use human_money_core::models::voucher::Transaction;
    use human_money_core::services::conflict_manager::{
        create_and_sign_resolution_endorsement, create_proof_of_double_spend,
        MAX_RESOLUTION_ENDORSEMENTS,
    };
    use human_money_core::services::crypto;
    use human_money_core::services::trap_manager::generate_sst_trap;
    use human_money_core::test_utils::{disclose_proof_receipt, sign_proof_fingerprints};
    use human_money_core::UserIdentity;

    struct TempDirGuard(std::path::PathBuf);

    impl TempDirGuard {
        fn new(label: &str) -> Self {
            Self(temp_test_dir(label))
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            if self.0.exists() {
                if let Err(err) = std::fs::remove_dir_all(&self.0) {
                    eprintln!(
                        "TempDirGuard cleanup failed for {}: {}",
                        self.0.display(),
                        err
                    );
                }
            }
        }
    }

    struct ConflictTestFixture {
        state: AppState,
        master_pw: &'static str,
        bob_id: UserIdentity,
        charlie_id: UserIdentity,
        proof_base: ProofOfDoubleSpend,
        proof_id: String,
    }

    fn build_attributed_proof_from_service(
        offender_svc: &human_money_core::app_service::AppService,
        victim_svc: &human_money_core::app_service::AppService,
        seed_byte: u8,
    ) -> ProofOfDoubleSpend {
        let offender = offender_svc
            .with_wallet_and_identity(|_, id| id.clone())
            .unwrap();
        let victim = victim_svc
            .with_wallet_and_identity(|_, id| id.clone())
            .unwrap();
        let seed = [seed_byte; 32];
        let input_key = crypto::derive_owned_ephemeral_key(&offender.signing_key, &seed).unwrap();
        let eph = input_key.verifying_key().to_bytes();
        let eph_b58 = bs58::encode(eph).into_string();
        let fork = crypto::get_hash(format!("app-fork-{seed_byte}"));
        let ds_tag = crypto::get_ds_tag(&fork, &eph_b58).unwrap();
        let id1 = crypto::get_hash(format!("app-tx1-{seed_byte}"));
        let id2 = crypto::get_hash(format!("app-tx2-{seed_byte}"));
        let (trap1, _) =
            generate_sst_trap(&offender.signing_key, &ds_tag, &eph, &id1, "", &seed).unwrap();
        let (trap2, _) =
            generate_sst_trap(&offender.signing_key, &ds_tag, &eph, &id2, "", &seed).unwrap();
        let mut txs = vec![
            Transaction {
                t_id: id1,
                prev_hash: fork.clone(),
                t_type: "transfer".into(),
                t_time: "2026-06-01T10:00:00Z".into(),
                recipient_id: victim.user_id.clone(),
                sender_ephemeral_pub: Some(eph_b58.clone()),
                trap_data: Some(trap1),
                ..Default::default()
            },
            Transaction {
                t_id: id2,
                prev_hash: fork.clone(),
                t_type: "transfer".into(),
                t_time: "2026-06-01T11:00:00Z".into(),
                recipient_id: victim.user_id.clone(),
                sender_ephemeral_pub: Some(eph_b58),
                trap_data: Some(trap2),
                ..Default::default()
            },
        ];
        let fps = sign_proof_fingerprints(&mut txs, &input_key);
        let mut proof = create_proof_of_double_spend(
            offender.user_id.clone(),
            fork,
            txs,
            "2099-01-01T00:00:00Z".into(),
            &victim,
            false,
        )
        .unwrap();
        proof.fingerprints = fps;
        disclose_proof_receipt(&mut proof, &offender, &seed, 0);
        proof
    }

    fn setup_conflict_fixture(dir: &std::path::Path) -> ConflictTestFixture {
        let keystore_dir = dir.join("keystore");
        let master_pw = "MasterConflictApp-2026!";

        let alice_svc = setup_test_app(&dir.join("alice"), "Alice", ALICE_PHRASE, master_pw);
        let bob_svc = setup_test_app(&dir.join("bob"), "Bob", BOB_PHRASE, master_pw);
        let charlie_svc =
            setup_test_app(&dir.join("charlie"), "Charlie", CHARLIE_PHRASE, master_pw);
        let offender_svc =
            setup_test_app(&dir.join("offender"), "Mallory", READER1_PHRASE, master_pw);

        let bob_id = bob_svc
            .with_wallet_and_identity(|_, id| id.clone())
            .unwrap();
        let charlie_id = charlie_svc
            .with_wallet_and_identity(|_, id| id.clone())
            .unwrap();

        let proof_base = build_attributed_proof_from_service(&offender_svc, &bob_svc, 51);
        let proof_id = proof_base.proof_id.clone();

        let folder = alice_svc.list_profiles().unwrap()[0].folder_name.clone();
        let state = AppState::new(alice_svc);

        {
            let mut svc = state.service.lock().unwrap();
            state
                .save_contact(
                    &mut svc,
                    FrontendContact {
                        did: bob_id.user_id.clone(),
                        profile: FrontendUserProfile {
                            first_name: Some("Bob".into()),
                            last_name: Some("Trusted".into()),
                            ..Default::default()
                        },
                        tags: vec![],
                        added_at: "2026-01-01T00:00:00Z".into(),
                        notes: None,
                        trust_level: Some(2),
                        desired_hops: Some(2),
                        has_peer_list: false,
                        peer_list_updated_at: None,
                        syncs: vec![],
                    },
                    Some(master_pw),
                )
                .unwrap();
        }

        crate::commands::auth::enable_profile_pin_impl(
            master_pw,
            "135790",
            "dailyPay",
            &folder,
            &keystore_dir,
            "device-drops-test",
            &state,
        )
        .unwrap();
        state.service.lock().unwrap().logout();
        crate::commands::auth::login_with_pin_impl(
            &folder,
            "135790",
            false,
            "device-drops-test".into(),
            &keystore_dir,
            "device-drops-test",
            &state,
            None,
        )
        .unwrap();
        crate::commands::auth::unlock_session_with_pin_impl(
            &folder,
            "135790",
            &keystore_dir,
            "device-drops-test",
            &state,
        )
        .unwrap();

        ConflictTestFixture {
            state,
            master_pw,
            bob_id,
            charlie_id,
            proof_base,
            proof_id,
        }
    }

    fn build_sybil_endorsements(proof_id: &str, count: usize) -> Vec<ResolutionEndorsement> {
        let mut sybil_list = Vec::with_capacity(count);
        for idx in 0..count {
            let (pk, sk) =
                crypto::generate_ed25519_keypair_for_tests(Some(&format!("app-sybil-{idx}")));
            let uid = crypto::create_user_id(&pk, None).unwrap();
            let id = UserIdentity {
                signing_key: sk,
                public_key: pk,
                user_id: uid,
                mnemonic_entropy: None,
                mnemonic_language: None,
                has_bip39_passphrase: false,
            };
            sybil_list.push(
                create_and_sign_resolution_endorsement(proof_id, &id, Some(format!("sybil {idx}")))
                    .unwrap(),
            );
        }
        sybil_list
    }

    #[test]
    fn test_import_double_spend_proof_enforces_2mib_size_limit() {
        let dir_guard = TempDirGuard::new("conflict_size_limit");
        let fixture = setup_conflict_fixture(dir_guard.path());

        // Positive baseline: valid proof within size cap imports cleanly
        let valid_bytes = serde_json::to_vec(&fixture.proof_base).unwrap();
        assert!(valid_bytes.len() < MAX_IMPORT_PROOF_BYTES);
        let ok_res = import_double_spend_proof_impl(valid_bytes, None, &fixture.state).unwrap();
        assert_eq!(ok_res.proof_id, fixture.proof_id);
        assert!(!ok_res.already_known);

        // Exact MAX_IMPORT_PROOF_BYTES passes size gate and reaches JSON parsing
        let exact_err =
            import_double_spend_proof_impl(vec![b' '; MAX_IMPORT_PROOF_BYTES], None, &fixture.state)
                .unwrap_err();
        assert!(
            exact_err.message.contains("EOF while parsing")
                && !exact_err.message.contains("exceeds maximum"),
            "exact 2 MiB boundary must pass size gate and hit JSON parser, got: {:?}",
            exact_err
        );

        // MAX_IMPORT_PROOF_BYTES + 1 fails before JSON parsing with exact size error
        let oversized_err = import_double_spend_proof_impl(
            vec![b' '; MAX_IMPORT_PROOF_BYTES + 1],
            None,
            &fixture.state,
        )
        .unwrap_err();
        assert_eq!(
            oversized_err.message,
            "Proof payload exceeds maximum allowed size (2 MiB)"
        );
    }

    #[test]
    fn test_import_double_spend_proof_wot_trust_evicts_sybils_at_cap_in_dailypay() {
        let dir_guard = TempDirGuard::new("conflict_wot_cap");
        let fixture = setup_conflict_fixture(dir_guard.path());

        // Import proof filled to MAX_RESOLUTION_ENDORSEMENTS with unknown Sybils in DailyPay session
        let mut sybil_proof = fixture.proof_base.clone();
        sybil_proof.resolutions = Some(build_sybil_endorsements(
            &fixture.proof_id,
            MAX_RESOLUTION_ENDORSEMENTS,
        ));
        let import_res1 = import_double_spend_proof_impl(
            serde_json::to_vec(&sybil_proof).unwrap(),
            None,
            &fixture.state,
        )
        .unwrap();
        assert_eq!(import_res1.proof_id, fixture.proof_id);
        assert!(!import_res1.already_known);
        assert_eq!(import_res1.added_endorsements, MAX_RESOLUTION_ENDORSEMENTS);

        // Unknown Charlie cannot evict full cap, whereas trusted Bob evicts one unknown Sybil via WoT priority
        let e_charlie = create_and_sign_resolution_endorsement(
            &fixture.proof_id,
            &fixture.charlie_id,
            Some("Unknown Charlie".into()),
        )
        .unwrap();
        let e_bob = create_and_sign_resolution_endorsement(
            &fixture.proof_id,
            &fixture.bob_id,
            Some("Bob confirms".into()),
        )
        .unwrap();
        let mut merge_proof = fixture.proof_base.clone();
        merge_proof.resolutions = Some(vec![e_charlie, e_bob]);
        let import_res2 = import_double_spend_proof_impl(
            serde_json::to_vec(&merge_proof).unwrap(),
            None,
            &fixture.state,
        )
        .unwrap();
        assert!(import_res2.already_known);
        assert_eq!(import_res2.added_endorsements, 1);

        let details =
            get_proof_of_double_spend_impl(fixture.proof_id.clone(), &fixture.state).unwrap();
        let res = details.proof.resolutions.unwrap();
        assert_eq!(res.len(), MAX_RESOLUTION_ENDORSEMENTS);
        let bob_entry = res
            .iter()
            .find(|r| r.endorser_id == fixture.bob_id.user_id)
            .expect("trusted Bob endorsement must evict an unknown Sybil");
        assert_eq!(bob_entry.endorser_name.as_deref(), Some("Bob Trusted"));
        assert!(bob_entry.wot_trust.trust_percent > 0);
        assert!(!res
            .iter()
            .any(|r| r.endorser_id == fixture.charlie_id.user_id));
    }

    #[test]
    fn test_endorse_conflict_resolution_requires_full_access_and_enriches_details() {
        let dir_guard = TempDirGuard::new("conflict_endorse_full");
        let fixture = setup_conflict_fixture(dir_guard.path());

        let mut sybil_proof = fixture.proof_base.clone();
        sybil_proof.resolutions = Some(build_sybil_endorsements(
            &fixture.proof_id,
            MAX_RESOLUTION_ENDORSEMENTS,
        ));
        import_double_spend_proof_impl(
            serde_json::to_vec(&sybil_proof).unwrap(),
            None,
            &fixture.state,
        )
        .unwrap();

        let e_charlie = create_and_sign_resolution_endorsement(
            &fixture.proof_id,
            &fixture.charlie_id,
            Some("Unknown Charlie".into()),
        )
        .unwrap();
        let e_bob = create_and_sign_resolution_endorsement(
            &fixture.proof_id,
            &fixture.bob_id,
            Some("Bob confirms".into()),
        )
        .unwrap();
        let mut merge_proof = fixture.proof_base.clone();
        merge_proof.resolutions = Some(vec![e_charlie, e_bob]);
        import_double_spend_proof_impl(
            serde_json::to_vec(&merge_proof).unwrap(),
            None,
            &fixture.state,
        )
        .unwrap();

        // DailyPay must reject endorse_conflict_resolution and set_conflict_local_override without master password
        let err_endorse = endorse_conflict_resolution_impl(
            fixture.proof_id.clone(),
            Some("settled".into()),
            true,
            None,
            &fixture.state,
        )
        .unwrap_err();
        assert_eq!(err_endorse.code, "error.auth.insufficientAccessMode");

        let err_override = set_conflict_local_override_impl(
            fixture.proof_id.clone(),
            true,
            None,
            None,
            &fixture.state,
        )
        .unwrap_err();
        assert_eq!(err_override.code, "error.auth.insufficientAccessMode");

        // Master password authorizes endorse_conflict_resolution with also_set_local_override=true
        let details = endorse_conflict_resolution_impl(
            fixture.proof_id.clone(),
            Some("Settled offline".into()),
            true,
            Some(fixture.master_pw.to_string()),
            &fixture.state,
        )
        .unwrap();

        assert!(details.local_override);
        assert!(details.has_own_endorsement);
        assert!(!details.can_endorse);
        let res = details.proof.resolutions.unwrap();
        assert_eq!(res.len(), MAX_RESOLUTION_ENDORSEMENTS);
        assert!(res[0].is_own_identity, "Own endorsement must sort first");
        assert_eq!(res[1].endorser_id, fixture.bob_id.user_id);
        assert_eq!(res[1].endorser_name.as_deref(), Some("Bob Trusted"));
        assert!(
            res[1].wot_trust.trust_percent > 0,
            "Trusted contact Bob must carry positive WoT trust percent"
        );
        assert!(!res
            .iter()
            .any(|r| r.endorser_id == fixture.charlie_id.user_id));
    }

    #[test]
    fn s03_import_double_spend_proof_authorizes_before_parsing_json() {
        let dir_guard = TempDirGuard::new("s03_proof_auth_before_parse");
        let alice_svc = setup_test_app(dir_guard.path(), "Alice", ALICE_PHRASE, "pw123");
        let state = crate::test_support::create_test_state(alice_svc);

        // Positive baseline: identical payload with valid credentials while unlocked reaches JSON parsing
        let unlocked_err = import_double_spend_proof_impl(
            b"{not-valid-json".to_vec(),
            Some("pw123".to_string()),
            &state,
        )
        .unwrap_err();
        assert_ne!(unlocked_err.code, "error.wallet.locked");
        assert!(
            unlocked_err.message.contains("key must be a string"),
            "unlocked invalid JSON must reach parser, got: {:?}",
            unlocked_err
        );

        // Locked session with identical arguments must fail auth before JSON parsing
        state.service.lock().unwrap().logout();
        let locked_err = import_double_spend_proof_impl(
            b"{not-valid-json".to_vec(),
            Some("pw123".to_string()),
            &state,
        )
        .unwrap_err();
        assert_eq!(
            locked_err.code, "error.wallet.locked",
            "Locked session must fail auth before JSON deserialization, got: {:?}",
            locked_err
        );
    }
}
