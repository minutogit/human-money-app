use super::*;
use crate::{
    models::{
        FrontendBackupInspectionDto, MnemonicLanguage, ResolvedSyncTargetDto,
        SyncBackupProbeResultDto,
    },
    services::fs_helpers::{cleanup_stale_tmp_restore_files, write_file_atomic},
    AppState,
};
use human_money_core::Wallet;

struct SyncCandidateTask {
    read_url: String,
    pubkey_hex: String,
    prefix_opt: Option<String>,
    kind: String,
    fallback_target: Option<ResolvedSyncTargetDto>,
}

struct PastedSyncInput {
    write_url: Option<String>,
    read_url: Option<String>,
    drop_pub: Option<String>,
    slot_id: Option<String>,
    register_url: Option<String>,
}

struct CollectedProbeCandidates {
    resolved_sync_targets: Vec<ResolvedSyncTargetDto>,
    candidate_tasks: Vec<SyncCandidateTask>,
    errors: Vec<String>,
}

impl CollectedProbeCandidates {
    fn push_candidate(
        &mut self,
        read_url: String,
        pubkey_hex: String,
        prefix_opt: Option<String>,
        kind: &str,
        fallback_target: Option<ResolvedSyncTargetDto>,
    ) {
        if !read_url.is_empty()
            && !self.candidate_tasks.iter().any(|t| {
                t.read_url == read_url && t.pubkey_hex == pubkey_hex && t.prefix_opt == prefix_opt
            })
        {
            self.candidate_tasks.push(SyncCandidateTask {
                read_url,
                pubkey_hex,
                prefix_opt,
                kind: kind.to_string(),
                fallback_target,
            });
        }
    }
}

fn parse_pasted_sync_input(trimmed: &str) -> PastedSyncInput {
    if trimmed.starts_with('{') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return PastedSyncInput {
                write_url: val
                    .get("writeUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                read_url: val
                    .get("readUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                drop_pub: val
                    .get("dropPub")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                slot_id: val
                    .get("slotId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                register_url: val
                    .get("registerUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            };
        }
    }
    PastedSyncInput {
        write_url: None,
        read_url: None,
        drop_pub: None,
        slot_id: None,
        register_url: None,
    }
}

async fn collect_sync_probe_candidates(
    non_empty_inputs: Vec<String>,
    my_ed_sk: &ed25519_dalek::SigningKey,
    master_seed: &[u8; 32],
    normalized_prefix: Option<&str>,
) -> CollectedProbeCandidates {
    let probe_client = crate::sync_client::create_sync_probe_client();
    let normalized_prefix_owned = normalized_prefix.map(str::to_string);
    let mut collected = CollectedProbeCandidates {
        resolved_sync_targets: Vec::new(),
        candidate_tasks: Vec::new(),
        errors: Vec::new(),
    };

    for trimmed in non_empty_inputs {
        let pasted = parse_pasted_sync_input(&trimmed);
        let raw_url = pasted.write_url.unwrap_or_else(|| trimmed.clone());
        let canonical_write_url = match humoco_market::canonicalize_write_url(&raw_url) {
            Ok(u) => u,
            Err(e) => {
                collected.errors.push(e.to_string());
                continue;
            }
        };

        match crate::sync_client::fetch_sync_info(&probe_client, &canonical_write_url).await {
            Ok(info) => {
                let sanitized_register_url = info
                    .register_url
                    .clone()
                    .filter(|u| crate::sync_client::validate_sync_url(u).is_ok());
                let target = ResolvedSyncTargetDto {
                    write_url: canonical_write_url.clone(),
                    read_url: info.read_url.clone(),
                    drop_pub: info.drop_pub.clone(),
                    register_url: sanitized_register_url,
                };
                if !collected
                    .resolved_sync_targets
                    .iter()
                    .any(|t| t.write_url == target.write_url)
                {
                    collected.resolved_sync_targets.push(target.clone());
                }

                // 1. Current stealth key
                if let Ok(stealth_pk) =
                    humoco_market::derive_member_stealth_pubkey(my_ed_sk, &info.drop_pub)
                {
                    collected.push_candidate(
                        info.read_url.clone(),
                        stealth_pk,
                        normalized_prefix_owned.clone(),
                        "stealth",
                        None,
                    );
                }

                // 2. Grace / previous drop key if descriptor carried a different drop_pub
                if let Some(ref d_pub) = pasted.drop_pub {
                    if !d_pub.is_empty() && d_pub != &info.drop_pub {
                        if let Ok(grace_pk) =
                            humoco_market::derive_member_stealth_pubkey(my_ed_sk, d_pub)
                        {
                            collected.push_candidate(
                                info.read_url.clone(),
                                grace_pk.clone(),
                                normalized_prefix_owned.clone(),
                                "grace",
                                None,
                            );
                            if let Some(ref p_read) = pasted.read_url {
                                collected.push_candidate(
                                    p_read.clone(),
                                    grace_pk,
                                    normalized_prefix_owned.clone(),
                                    "grace",
                                    None,
                                );
                            }
                        }
                    }
                }

                // 3. Public drop anonymous key
                if let Ok((_r_sk, _r_vk, r_hex)) =
                    humoco_market::derive_public_drop_keypair(master_seed, &canonical_write_url)
                {
                    collected.push_candidate(
                        info.read_url.clone(),
                        r_hex,
                        normalized_prefix_owned.clone(),
                        "public",
                        None,
                    );
                }

                // 4. Explicit slot_id from descriptor
                if let Some(ref sid) = pasted.slot_id {
                    let (base_pk, sid_pfx) =
                        humoco_market::parse_slot_id(sid).unwrap_or_else(|_| (sid.clone(), None));
                    let eff_pfx = sid_pfx.or_else(|| normalized_prefix_owned.clone());
                    collected.push_candidate(
                        info.read_url.clone(),
                        base_pk.clone(),
                        eff_pfx.clone(),
                        "slot_id",
                        None,
                    );
                    if let Some(ref p_read) = pasted.read_url {
                        collected.push_candidate(p_read.clone(), base_pk, eff_pfx, "slot_id", None);
                    }
                }

                // 5. Deterministic Own-Drop fallback
                let own_data_dir = humoco_market::derive_drop_data_dir(master_seed, 0);
                let origin_dir = canonical_write_url
                    .rsplit_once('/')
                    .map(|(b, _)| b)
                    .unwrap_or(&canonical_write_url);
                let own_read_url = format!("{}/{}", origin_dir.trim_end_matches('/'), own_data_dir);
                let (admin_sk, admin_vk, _) =
                    humoco_market::derive_drop_admin_keypair(master_seed, 0);
                if let Ok(own_stealth) =
                    humoco_market::derive_member_stealth_pubkey_admin(&admin_sk, &admin_vk)
                {
                    collected.push_candidate(
                        own_read_url,
                        own_stealth.clone(),
                        normalized_prefix_owned.clone(),
                        "own",
                        None,
                    );
                    collected.push_candidate(
                        info.read_url,
                        own_stealth,
                        normalized_prefix_owned.clone(),
                        "own",
                        None,
                    );
                }
            }
            Err(info_err) => {
                if let (Some(r_url), Some(d_pub)) = (pasted.read_url, pasted.drop_pub) {
                    let sanitized_pasted_register_url = pasted
                        .register_url
                        .filter(|u| crate::sync_client::validate_sync_url(u).is_ok());
                    let fallback_target = ResolvedSyncTargetDto {
                        write_url: canonical_write_url.clone(),
                        read_url: r_url.clone(),
                        drop_pub: d_pub.clone(),
                        register_url: sanitized_pasted_register_url,
                    };
                    if let Ok(stealth_pk) =
                        humoco_market::derive_member_stealth_pubkey(my_ed_sk, &d_pub)
                    {
                        collected.push_candidate(
                            r_url.clone(),
                            stealth_pk,
                            normalized_prefix_owned.clone(),
                            "stealth",
                            Some(fallback_target.clone()),
                        );
                    }
                    if let Ok((_r_sk, _r_vk, r_hex)) =
                        humoco_market::derive_public_drop_keypair(master_seed, &canonical_write_url)
                    {
                        collected.push_candidate(
                            r_url.clone(),
                            r_hex,
                            normalized_prefix_owned.clone(),
                            "public",
                            Some(fallback_target.clone()),
                        );
                    }
                    if let Some(ref sid) = pasted.slot_id {
                        let (base_pk, sid_pfx) = humoco_market::parse_slot_id(sid)
                            .unwrap_or_else(|_| (sid.clone(), None));
                        let eff_pfx = sid_pfx.or_else(|| normalized_prefix_owned.clone());
                        collected.push_candidate(
                            r_url,
                            base_pk,
                            eff_pfx,
                            "slot_id",
                            Some(fallback_target),
                        );
                    }
                }
                collected
                    .errors
                    .push(format!("{}: {}", canonical_write_url, info_err));
            }
        }
    }

    collected
}

async fn download_backup_candidates(
    candidate_tasks: Vec<SyncCandidateTask>,
) -> Vec<(Vec<u8>, String, String, Option<ResolvedSyncTargetDto>)> {
    let fetch_client = crate::sync_client::create_sync_http_client();
    let mut set = tokio::task::JoinSet::new();
    for task in candidate_tasks {
        let cl = fetch_client.clone();
        set.spawn(async move {
            let bytes_opt = tokio::time::timeout(
                std::time::Duration::from_millis(5000),
                fetch_and_merge_backup_slot(
                    &cl,
                    &task.read_url,
                    &task.pubkey_hex,
                    task.prefix_opt.as_deref(),
                ),
            )
            .await
            .ok()
            .flatten();
            bytes_opt.map(|bytes| (bytes, task.kind, task.read_url, task.fallback_target))
        });
    }

    let mut downloaded = Vec::new();
    while let Some(res) = set.join_next().await {
        if let Ok(Some(item)) = res {
            downloaded.push(item);
        }
    }
    downloaded
}

type StagedProbeCandidate = (
    PathBuf,
    FrontendBackupInspectionDto,
    String,
    String,
    Option<ResolvedSyncTargetDto>,
);

async fn stage_and_select_best_backup_candidate(
    state: &AppState,
    start_generation: u64,
    tmp_restore_dir: &Path,
    downloaded: Vec<(Vec<u8>, String, String, Option<ResolvedSyncTargetDto>)>,
    credentials: &BackupRestoreCredentials<'_>,
    inspect_ctx: (
        &Path,
        &human_money_core::UserIdentity,
        &HashMap<String, Option<String>>,
    ),
) -> Result<Option<StagedProbeCandidate>, FrontendError> {
    let (wallet_data_dir, identity, user_ids) = inspect_ctx;
    let mut inspected_candidates: Vec<StagedProbeCandidate> = Vec::new();

    if !state.is_generation_current(start_generation) {
        return Err(FrontendError::from(
            "Wallet session changed while probing remote sync backups".to_string(),
        ));
    }

    for (unpadded, kind, source_url, fallback_target) in downloaded {
        let staged_file_name = format!("sync_restore_{}.hmcbackup", uuid::Uuid::new_v4());
        let staged_path = tmp_restore_dir.join(staged_file_name);
        if write_file_atomic(&staged_path, &unpadded).is_ok() {
            if !state.is_generation_current(start_generation) {
                let _ = fs::remove_file(&staged_path);
                for (other_path, _, _, _, _) in inspected_candidates {
                    let _ = fs::remove_file(other_path);
                }
                return Err(FrontendError::from(
                    "Wallet session changed while inspecting downloaded backups".to_string(),
                ));
            }
            let service = match state.lock_service() {
                Ok(s) => s,
                Err(err) => {
                    let _ = fs::remove_file(&staged_path);
                    for (other_path, _, _, _, _) in inspected_candidates {
                        let _ = fs::remove_file(other_path);
                    }
                    return Err(err);
                }
            };
            if let Ok(inspection) = inspect_backup_with_collision_impl(
                &staged_path,
                credentials,
                &service,
                Some(wallet_data_dir),
                Some(identity),
                user_ids,
            ) {
                inspected_candidates.push((
                    staged_path,
                    inspection,
                    kind,
                    source_url,
                    fallback_target,
                ));
            } else {
                let _ = fs::remove_file(&staged_path);
            }
        }
    }

    if inspected_candidates.is_empty() {
        return Ok(None);
    }

    inspected_candidates.sort_by(|a, b| (b.1.epoch, b.1.tx_nonce).cmp(&(a.1.epoch, a.1.tx_nonce)));
    let winner = inspected_candidates.remove(0);
    for (other_path, _, _, _, _) in inspected_candidates {
        let _ = fs::remove_file(other_path);
    }
    Ok(Some(winner))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn probe_sync_backup_impl(
    sync_inputs: Vec<String>,
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    wallet_data_dir: &Path,
    tmp_restore_dir: &Path,
    state: &AppState,
) -> Result<SyncBackupProbeResultDto, FrontendError> {
    let start_generation = state.current_session_generation();
    let cleaned_prefix = user_prefix
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let lang_enum = language.unwrap_or(MnemonicLanguage::English);
    let core_lang: human_money_core::MnemonicLanguage = lang_enum.into();
    let (_, identity) = Wallet::new_from_mnemonic(
        &mnemonic,
        passphrase.as_deref().filter(|p| !p.is_empty()),
        cleaned_prefix,
        core_lang,
        "probe".to_string(),
        false,
    )
    .map_err(FrontendError::from)?;

    let my_ed_sk = identity.signing_key.clone();
    let master_seed = zeroize::Zeroizing::new(my_ed_sk.to_bytes());
    let normalized_prefix = cleaned_prefix.map(str::to_lowercase);
    let user_ids = user_ids_from_metadata_dir(wallet_data_dir);
    let local_collision = {
        let service = state.lock_service()?;
        find_local_profile_collision_impl(
            wallet_data_dir,
            &service,
            &identity.user_id,
            Some(&identity),
            &user_ids,
        )
    };

    let non_empty_inputs: Vec<String> = sync_inputs
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if non_empty_inputs.is_empty() {
        return Err(FrontendError::from(
            "Sync-Dienst-Adresse darf nicht leer sein".to_string(),
        ));
    }
    if non_empty_inputs.len() > MAX_PROBE_SYNC_INPUTS {
        return Err(FrontendError::from(format!(
            "Too many sync inputs ({} > maximum {})",
            non_empty_inputs.len(),
            MAX_PROBE_SYNC_INPUTS
        )));
    }

    let collected = collect_sync_probe_candidates(
        non_empty_inputs,
        &my_ed_sk,
        &master_seed,
        normalized_prefix.as_deref(),
    )
    .await;
    let mut resolved_sync_targets = collected.resolved_sync_targets;
    let errors = collected.errors;
    let downloaded = download_backup_candidates(collected.candidate_tasks).await;

    cleanup_stale_tmp_restore_files(tmp_restore_dir);
    fs::create_dir_all(tmp_restore_dir).map_err(|e| FrontendError::from(e.to_string()))?;

    let credentials = BackupRestoreCredentials::Mnemonic {
        phrase: &mnemonic,
        passphrase: passphrase.as_deref().filter(|p| !p.is_empty()),
        prefix: cleaned_prefix,
        language: core_lang,
    };

    if let Some((best_path, best_inspection, best_kind, best_source, fallback_target)) =
        stage_and_select_best_backup_candidate(
            state,
            start_generation,
            tmp_restore_dir,
            downloaded,
            &credentials,
            (wallet_data_dir, &identity, &user_ids),
        )
        .await?
    {
        if let Some(ft) = fallback_target {
            if !resolved_sync_targets
                .iter()
                .any(|t| t.write_url == ft.write_url)
            {
                resolved_sync_targets.push(ft);
            }
        }

        return Ok(SyncBackupProbeResultDto {
            status: "found".to_string(),
            staged_path: Some(best_path.to_string_lossy().to_string()),
            matched_key_kind: Some(best_kind),
            source_url: Some(best_source),
            inspection: Some(best_inspection),
            user_id: identity.user_id.clone(),
            local_collision,
            resolved_sync_targets,
            error: None,
        });
    }

    if !resolved_sync_targets.is_empty() {
        return Ok(SyncBackupProbeResultDto {
            status: "not_found".to_string(),
            staged_path: None,
            matched_key_kind: None,
            source_url: None,
            inspection: None,
            user_id: identity.user_id.clone(),
            local_collision,
            resolved_sync_targets,
            error: None,
        });
    }

    Ok(SyncBackupProbeResultDto {
        status: "unreachable".to_string(),
        staged_path: None,
        matched_key_kind: None,
        source_url: None,
        inspection: None,
        user_id: identity.user_id.clone(),
        local_collision,
        resolved_sync_targets,
        error: Some(if errors.is_empty() {
            "Sync-Dienst konnte nicht erreicht werden.".to_string()
        } else {
            errors.join(" | ")
        }),
    })
}

pub(crate) async fn probe_sync_backup_for_app_impl(
    sync_inputs: Vec<String>,
    mnemonic: String,
    passphrase: Option<String>,
    user_prefix: Option<String>,
    language: Option<MnemonicLanguage>,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<SyncBackupProbeResultDto, FrontendError> {
    let app_dir = resolve_app_data_dir(app)?;
    let wallet_data_dir = app_dir.join("wallet_data");
    let tmp_restore_dir = app_dir.join("tmp_restore");

    probe_sync_backup_impl(
        sync_inputs,
        mnemonic,
        passphrase,
        user_prefix,
        language,
        &wallet_data_dir,
        &tmp_restore_dir,
        state,
    )
    .await
}
