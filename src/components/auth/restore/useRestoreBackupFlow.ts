import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { SeedPhraseValue } from '../SeedPhraseInput';
import { backupService } from '../../../services/backupService';
import { recoveryService } from '../../../services/recoveryService';
import { profileService } from '../../../services/profileService';
import { authService } from '../../../services/authService';
import { useOptionalSession } from '../../../context/SessionContext';
import {
    ActiveRecoverySessionDto,
    BackupProbeDto,
    BackupInspectionDto,
    LocalProfileCollisionDto,
    ResolvedSyncTargetDto,
    RestoreOutcomeDto,
    SyncBackupProbeResultDto,
} from '../../../types';
import { translateError, stringifyError } from '../../../utils/errorHelper';
import { logger } from '../../../utils/log';
import {
    EMPTY_SEED_STATE,
    RestoreFeedback,
    TopMethod,
    WordsSubPhase,
    buildBackupCredentialArgs,
    parseEffectiveSyncInputs,
    resolveDefaultCollisionChoice,
    validateDevicePasswordPair,
} from './restoreBackupHelpers';

export interface UseRestoreBackupFlowParams {
    onRestoreSuccess: (outcome: RestoreOutcomeDto) => void;
    onBack: () => void;
    initialPayload?: string;
    initialBackupPath?: string;
    initialMethod?: 'social' | 'words_or_sheet' | 'file_or_folder';
}

/**
 * State and async orchestration hook for the three-branch RestoreBackupView
 * (social recovery, recovery words / emergency sheet / sync, and backup file/folder).
 */
export function useRestoreBackupFlow({
    onRestoreSuccess,
    onBack,
    initialPayload,
    initialBackupPath,
    initialMethod,
}: UseRestoreBackupFlowParams) {
    const { t } = useTranslation();
    const session = useOptionalSession();

    const [method, setMethod] = useState<TopMethod>(() => {
        if (initialMethod) return initialMethod;
        if (initialBackupPath) return 'file_or_folder';
        if (initialPayload?.includes('humoco://recovery-reply')) return 'social';
        if (initialPayload?.includes('humoco://kit')) return 'words_or_sheet';
        return 'choose';
    });

    const [activeSocialSession, setActiveSocialSession] =
        useState<ActiveRecoverySessionDto | null>(null);

    const [wordsPhase, setWordsPhase] = useState<WordsSubPhase>('input');
    const [seedState, setSeedState] = useState<SeedPhraseValue>(EMPTY_SEED_STATE);
    const [syncInput, setSyncInput] = useState<string>('');
    const [syncEdited, setSyncEdited] = useState<boolean>(false);
    const [lastSyncInputs, setLastSyncInputs] = useState<string[]>([]);
    const [syncProbe, setSyncProbe] = useState<SyncBackupProbeResultDto | null>(null);
    const [resolvedSyncTargets, setResolvedSyncTargets] = useState<ResolvedSyncTargetDto[]>([]);
    const [matchedSyncKeyKind, setMatchedSyncKeyKind] = useState<string | null>(null);
    const [focusPrefixTrigger, setFocusPrefixTrigger] = useState<number>(0);

    const [sourcePath, setSourcePath] = useState<string>('');
    const [probe, setProbe] = useState<BackupProbeDto | null>(null);
    const [credentialMode, setCredentialMode] = useState<'password' | 'mnemonic'>('password');
    const [backupPassword, setBackupPassword] = useState<string>('');
    const [fileSeedState, setFileSeedState] = useState<SeedPhraseValue>(EMPTY_SEED_STATE);
    const [isDraggingBackup, setIsDraggingBackup] = useState<boolean>(false);

    const [inspection, setInspection] = useState<BackupInspectionDto | null>(null);
    const [activeCollision, setActiveCollision] = useState<LocalProfileCollisionDto | null>(null);
    const [collisionChoice, setCollisionChoice] = useState<'reset_pw' | 'replace'>('reset_pw');
    const [profileName, setProfileName] = useState<string>('');
    const [newPassword, setNewPassword] = useState<string>('');
    const [confirmNewPassword, setConfirmNewPassword] = useState<string>('');
    const [restoreOutcome, setRestoreOutcome] = useState<RestoreOutcomeDto | null>(null);

    const [isBusy, setIsBusy] = useState<boolean>(false);
    const [isReconcilingL2, setIsReconcilingL2] = useState<boolean>(false);
    const [feedback, setFeedback] = useState<RestoreFeedback | null>(null);

    useEffect(() => {
        if (method !== 'choose') return;
        let active = true;
        recoveryService
            .getActiveRecoverySession()
            .then((activeSess) => {
                if (!active) return;
                if (
                    activeSess &&
                    (!activeSess.sessionMode || activeSess.sessionMode === 'restore_new_device')
                ) {
                    setActiveSocialSession(activeSess);
                } else {
                    setActiveSocialSession(null);
                }
            })
            .catch(() => {
                // Ignore session load failure
            });
        return () => {
            active = false;
        };
    }, [method]);

    useEffect(() => {
        if (initialMethod) {
            setMethod(initialMethod);
        } else if (initialPayload?.includes('humoco://recovery-reply')) {
            setMethod('social');
        } else if (initialPayload?.includes('humoco://kit')) {
            setMethod('words_or_sheet');
        }
    }, [initialMethod, initialPayload]);

    const probeAndAdvanceFile = useCallback(
        async (path: string) => {
            setFeedback(null);
            setIsBusy(true);
            try {
                const probeResult = await backupService.probeBackup(path);
                setSourcePath(path);
                setProbe(probeResult);
                setInspection(null);
                setActiveCollision(null);
                setMatchedSyncKeyKind(null);
                setCredentialMode(probeResult.supportsPasswordRestore ? 'password' : 'mnemonic');
                setMethod('file_or_folder');
            } catch (err) {
                logger.error(`Failed to probe backup at ${path}: ${stringifyError(err)}`);
                setFeedback({ type: 'error', message: translateError(err, t) });
            } finally {
                setIsBusy(false);
            }
        },
        [t]
    );

    useEffect(() => {
        if (initialBackupPath) {
            void probeAndAdvanceFile(initialBackupPath);
        }
    }, [initialBackupPath, probeAndAdvanceFile]);

    const handleSeedStateChange = useCallback(
        (next: SeedPhraseValue) => {
            setSeedState(next);
            if (!syncEdited && next.detectedKit && next.detectedKit.syncUrls.length > 0) {
                setSyncInput(next.detectedKit.syncUrls.join(', '));
            }
        },
        [syncEdited]
    );

    const handleHeaderBack = () => {
        setFeedback(null);
        if (method === 'words_or_sheet') {
            if (wordsPhase !== 'input') {
                setWordsPhase('input');
                setInspection(null);
                setActiveCollision(null);
                return;
            }
            setMethod('choose');
            return;
        }
        if (method === 'file_or_folder') {
            if (inspection) {
                setInspection(null);
                setActiveCollision(null);
                return;
            }
            if (probe) {
                setProbe(null);
                setSourcePath('');
                return;
            }
            setMethod('choose');
            return;
        }
        if (method === 'social') {
            setMethod('choose');
            return;
        }
        onBack();
    };

    const runSyncProbe = async (inputsToProbe: string[]) => {
        setIsBusy(true);
        setFeedback(null);
        try {
            const probeRes = await backupService.probeSyncBackup({
                syncInputs: inputsToProbe,
                mnemonic: seedState.mnemonic,
                passphrase: seedState.passphrase || null,
                userPrefix: seedState.userPrefix.trim() || null,
                language: seedState.language,
            });
            setLastSyncInputs(inputsToProbe);
            setSyncProbe(probeRes);
            setResolvedSyncTargets(probeRes.resolvedSyncTargets);
            setNewPassword('');
            setConfirmNewPassword('');

            if (probeRes.status === 'found' && probeRes.stagedPath && probeRes.inspection) {
                setSourcePath(probeRes.stagedPath);
                setInspection(probeRes.inspection);
                setMatchedSyncKeyKind(probeRes.matchedKeyKind);
                setCredentialMode('mnemonic');
                setProbe({
                    sourceKind: 'file',
                    protectionMode: probeRes.inspection.protectionMode,
                    supportsPasswordRestore: false,
                    supportsMnemonicRestore: true,
                    blobCount: probeRes.inspection.blobCount,
                    customStandardCount: probeRes.inspection.customStandardCount,
                });
                const coll = probeRes.inspection.localCollision ?? probeRes.localCollision;
                setActiveCollision(coll);
                if (coll) {
                    setCollisionChoice(resolveDefaultCollisionChoice(coll, probeRes.inspection));
                }
                setWordsPhase('sync_found');
            } else {
                setInspection(null);
                setActiveCollision(probeRes.localCollision);
                setWordsPhase(probeRes.status === 'not_found' ? 'sync_not_found' : 'sync_unreachable');
            }
        } catch (err) {
            logger.error(`Probe sync backup failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsBusy(false);
        }
    };

    const handleWordsContinue = async (e?: React.FormEvent) => {
        e?.preventDefault();
        setFeedback(null);
        if (!seedState.isValid) {
            setFeedback({ type: 'error', message: t('auth.enterValidSeed') });
            return;
        }
        const effectiveSyncInputs = parseEffectiveSyncInputs(
            syncInput,
            syncEdited,
            seedState.detectedKit
        );
        if (effectiveSyncInputs.length === 0) {
            setIsBusy(true);
            try {
                const preview = await backupService.previewMnemonicIdentity({
                    mnemonic: seedState.mnemonic,
                    passphrase: seedState.passphrase || null,
                    userPrefix: seedState.userPrefix.trim() || null,
                    language: seedState.language,
                });
                setSyncProbe(null);
                setResolvedSyncTargets([]);
                setInspection(null);
                setActiveCollision(preview.localCollision);
                setNewPassword('');
                setConfirmNewPassword('');
                setWordsPhase('direct_setup');
            } catch (err) {
                setFeedback({ type: 'error', message: translateError(err, t) });
            } finally {
                setIsBusy(false);
            }
            return;
        }
        await runSyncProbe(effectiveSyncInputs);
    };

    const handleDirectOpenProfile = async (e: React.FormEvent) => {
        e.preventDefault();
        setFeedback(null);

        const effectiveProfileName = profileName.trim() || seedState.userPrefix.trim();
        if (!effectiveProfileName) {
            setFeedback({ type: 'error', message: t('profile.profileNameRequired') });
            return;
        }
        const pwErr = validateDevicePasswordPair(newPassword, confirmNewPassword, t);
        if (pwErr) {
            setFeedback({ type: 'error', message: pwErr });
            return;
        }

        setIsBusy(true);
        try {
            const localInstanceId = await authService.getLocalInstanceId();
            await profileService.createProfile({
                profileName: effectiveProfileName,
                mnemonic: seedState.mnemonic,
                passphrase: seedState.passphrase || undefined,
                userPrefix: seedState.userPrefix.trim() || undefined,
                password: newPassword,
                localInstanceId,
                language: seedState.language,
                retainMnemonic: true,
                initialReachPreset: 'medium',
                initialSyncTargets:
                    resolvedSyncTargets.length > 0 ? resolvedSyncTargets : undefined,
            });
            onRestoreSuccess({
                folderName: '',
                profileName: effectiveProfileName,
                activeVoucherQueriesCount: 0,
                reconciledSpentCount: 0,
                restoredStandardsCount: 0,
                l2ReconciliationStatus: 'no_servers',
            });
        } catch (err) {
            logger.error(`Direct profile open failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsBusy(false);
        }
    };

    const handleDirectCollisionResetPassword = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!activeCollision) return;
        setFeedback(null);

        const pwErr = validateDevicePasswordPair(newPassword, confirmNewPassword, t);
        if (pwErr) {
            setFeedback({ type: 'error', message: pwErr });
            return;
        }

        setIsBusy(true);
        try {
            const localInstanceId = await authService.getLocalInstanceId();
            await profileService.recoverWallet({
                folderName: activeCollision.folderName,
                mnemonic: seedState.mnemonic,
                passphrase: seedState.passphrase || undefined,
                newPassword,
                localInstanceId,
                language: seedState.language,
            });
            onRestoreSuccess({
                folderName: activeCollision.folderName,
                profileName: activeCollision.profileName,
                activeVoucherQueriesCount: 0,
                reconciledSpentCount: 0,
                restoredStandardsCount: 0,
                l2ReconciliationStatus: 'reconciled',
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsBusy(false);
        }
    };

    const handleSwitchToSubaccountFromCollision = () => {
        setFeedback(null);
        setActiveCollision(null);
        setWordsPhase('input');
        setFocusPrefixTrigger((n) => n + 1);
    };

    const handleSelectSource = async (directory: boolean) => {
        setFeedback(null);
        try {
            const selected = await open({
                multiple: false,
                directory,
                ...(directory
                    ? {}
                    : { filters: [{ name: 'Human Money Backup', extensions: ['hmcbackup'] }] }),
            });
            if (!selected) return;
            const path = typeof selected === 'string' ? selected : selected[0];
            if (path) await probeAndAdvanceFile(path);
        } catch (err) {
            logger.error(
                `Failed to open backup ${directory ? 'folder' : 'file'}: ${stringifyError(err)}`
            );
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleInlineDrop = async (e: React.DragEvent) => {
        e.preventDefault();
        e.stopPropagation();
        setIsDraggingBackup(false);
        const file = e.dataTransfer?.files?.[0];
        if (!file) return;
        setFeedback(null);
        setIsBusy(true);
        try {
            const buf = await file.arrayBuffer();
            const staged = await backupService.stageDroppedBackupBytes(
                Array.from(new Uint8Array(buf))
            );
            await probeAndAdvanceFile(staged);
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsBusy(false);
        }
    };

    const handleInspectFileBackup = async () => {
        setFeedback(null);
        if (credentialMode === 'password') {
            if (!backupPassword) {
                setFeedback({ type: 'error', message: t('settings.backupPasswordMinLength') });
                return;
            }
        } else if (!fileSeedState.isValid) {
            setFeedback({ type: 'error', message: t('auth.enterValidSeed') });
            return;
        }

        setIsBusy(true);
        try {
            const result = await backupService.inspectBackup({
                sourcePath,
                ...buildBackupCredentialArgs(credentialMode, backupPassword, fileSeedState, true),
            });
            setInspection(result);
            setActiveCollision(result.localCollision);
            if (result.localCollision) {
                setCollisionChoice(resolveDefaultCollisionChoice(result.localCollision, result));
            }
        } catch (err) {
            logger.error(`Inspect backup failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsBusy(false);
        }
    };

    const handleRestoreFromInspectedBackup = async (e: React.FormEvent) => {
        e.preventDefault();
        setFeedback(null);
        const pwErr = validateDevicePasswordPair(newPassword, confirmNewPassword, t);
        if (pwErr) {
            setFeedback({ type: 'error', message: pwErr });
            return;
        }

        const activeCredSeed = method === 'words_or_sheet' ? seedState : fileSeedState;
        const effectiveCredMode = method === 'words_or_sheet' ? 'mnemonic' : credentialMode;

        setIsBusy(true);
        try {
            const localInstanceId = await authService.getLocalInstanceId();
            if (activeCollision && collisionChoice === 'reset_pw') {
                await backupService.resetLocalPasswordViaBackup({
                    targetFolderName: activeCollision.folderName,
                    sourcePath,
                    ...buildBackupCredentialArgs(
                        effectiveCredMode,
                        backupPassword,
                        activeCredSeed,
                        false
                    ),
                    newPassword,
                    localInstanceId,
                });
                onRestoreSuccess({
                    folderName: activeCollision.folderName,
                    profileName: activeCollision.profileName,
                    activeVoucherQueriesCount: 0,
                    reconciledSpentCount: 0,
                    restoredStandardsCount: 0,
                    l2ReconciliationStatus: 'reconciled',
                });
                return;
            }

            const outcome = await backupService.restoreBackup({
                sourcePath,
                ...buildBackupCredentialArgs(
                    effectiveCredMode,
                    backupPassword,
                    activeCredSeed,
                    true
                ),
                newPassword,
                localInstanceId,
                replaceFolderName:
                    activeCollision && collisionChoice === 'replace'
                        ? activeCollision.folderName
                        : null,
            });
            setRestoreOutcome(outcome);
            setMethod('completed');
        } catch (err) {
            logger.error(`Restore backup failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsBusy(false);
        }
    };

    const handleReconcileL2 = async () => {
        setIsReconcilingL2(true);
        setFeedback(null);
        try {
            const res = newPassword
                ? await backupService.reconcileRestoredWalletL2(newPassword)
                : session?.protectAction
                  ? await session.protectAction(async (pwd) => {
                        return await backupService.reconcileRestoredWalletL2(pwd || undefined);
                    })
                  : await backupService.reconcileRestoredWalletL2(undefined);
            if (!res) return;
            if (restoreOutcome) {
                setRestoreOutcome({
                    ...restoreOutcome,
                    reconciledSpentCount: res.reconciledSpentCount,
                    l2ReconciliationStatus: res.l2ReconciliationStatus,
                });
            }
            setFeedback(
                res.l2ReconciliationStatus === 'reconciled'
                    ? { type: 'success', message: t('profile.backupRestoreReconcileSuccess') }
                    : { type: 'error', message: t('profile.backupRestoreReconcileOffline') }
            );
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsReconcilingL2(false);
        }
    };

    return {
        method,
        setMethod,
        wordsPhase,
        setWordsPhase,
        activeSocialSession,
        seedState,
        syncInput,
        setSyncInput,
        setSyncEdited,
        syncProbe,
        lastSyncInputs,
        setResolvedSyncTargets,
        matchedSyncKeyKind,
        focusPrefixTrigger,
        sourcePath,
        probe,
        setProbe,
        setSourcePath,
        credentialMode,
        setCredentialMode,
        backupPassword,
        setBackupPassword,
        setFileSeedState,
        isDraggingBackup,
        setIsDraggingBackup,
        inspection,
        setInspection,
        activeCollision,
        setActiveCollision,
        collisionChoice,
        setCollisionChoice,
        profileName,
        setProfileName,
        newPassword,
        setNewPassword,
        confirmNewPassword,
        setConfirmNewPassword,
        restoreOutcome,
        setRestoreOutcome,
        isBusy,
        isReconcilingL2,
        feedback,
        setFeedback,
        handleSeedStateChange,
        handleHeaderBack,
        runSyncProbe,
        handleWordsContinue,
        handleDirectOpenProfile,
        handleDirectCollisionResetPassword,
        handleSwitchToSubaccountFromCollision,
        handleSelectFile: () => handleSelectSource(false),
        handleSelectFolder: () => handleSelectSource(true),
        handleInlineDrop,
        handleInspectFileBackup,
        handleRestoreFromInspectedBackup,
        handleReconcileL2,
    };
}
