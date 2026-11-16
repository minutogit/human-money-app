// src/components/RestoreBackupView.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { open } from '@tauri-apps/plugin-dialog';
import { AuthLayout } from './AuthLayout';
import { Card } from './ui/Card';
import { Button } from './ui/Button';
import { BackButton } from './ui/BackButton';
import { AlertBanner } from './ui/AlertBanner';
import { Input } from './ui/Input';
import { SeedPhraseInput, SeedPhraseValue } from './ui/SeedPhraseInput';
import { SocialRecoveryFlow } from './SocialRecoveryFlow';
import { backupService } from '../services/backupService';
import { recoveryService } from '../services/recoveryService';
import { profileService } from '../services/profileService';
import { authService } from '../services/authService';
import { useOptionalSession } from '../context/SessionContext';
import {
    ActiveRecoverySessionDto,
    BackupProbeDto,
    BackupInspectionDto,
    LocalProfileCollisionDto,
    ResolvedSyncTargetDto,
    RestoreOutcomeDto,
    SyncBackupProbeResultDto,
} from '../types';
import { translateError, stringifyError } from '../utils/errorHelper';
import { logger } from '../utils/log';
import {
    Users,
    KeyRound,
    FileArchive,
    FolderSync,
    Lock,
    CheckCircle2,
    ShieldCheck,
    ArrowRight,
    ArrowLeft,
    RefreshCw,
    Folder,
    FileText,
    History,
    Layers,
    Hash,
    Server,
    AlertTriangle,
    UploadCloud,
    Sparkles,
} from 'lucide-react';

export interface RestoreBackupViewProps {
    onRestoreSuccess: (outcome: RestoreOutcomeDto) => void;
    onBack: () => void;
    initialPayload?: string;
    initialBackupPath?: string;
    initialMethod?: 'social' | 'words_or_sheet' | 'file_or_folder';
}

type TopMethod = 'choose' | 'social' | 'words_or_sheet' | 'file_or_folder' | 'completed';
type WordsSubPhase = 'input' | 'direct_setup' | 'sync_found' | 'sync_not_found' | 'sync_unreachable';

const EMPTY_SEED_STATE: SeedPhraseValue = {
    mnemonic: '',
    wordCount: 12,
    language: 'english',
    passphrase: '',
    userPrefix: '',
    isValid: false,
    detectedKit: null,
};

export const RestoreBackupView: React.FC<RestoreBackupViewProps> = ({
    onRestoreSuccess,
    onBack,
    initialPayload,
    initialBackupPath,
    initialMethod,
}) => {
    const { t } = useTranslation();
    const session = useOptionalSession();

    const [method, setMethod] = useState<TopMethod>(() => {
        if (initialMethod) return initialMethod;
        if (initialBackupPath) return 'file_or_folder';
        if (initialPayload?.includes('humoco://recovery-reply')) return 'social';
        if (initialPayload?.includes('humoco://kit')) return 'words_or_sheet';
        return 'choose';
    });

    // Active social session banner state
    const [activeSocialSession, setActiveSocialSession] =
        useState<ActiveRecoverySessionDto | null>(null);

    // Words / Emergency Sheet flow state
    const [wordsPhase, setWordsPhase] = useState<WordsSubPhase>('input');
    const [seedState, setSeedState] = useState<SeedPhraseValue>(EMPTY_SEED_STATE);
    const [syncInput, setSyncInput] = useState<string>('');
    const [syncEdited, setSyncEdited] = useState<boolean>(false);
    const [lastSyncInputs, setLastSyncInputs] = useState<string[]>([]);
    const [syncProbe, setSyncProbe] = useState<SyncBackupProbeResultDto | null>(null);
    const [resolvedSyncTargets, setResolvedSyncTargets] = useState<ResolvedSyncTargetDto[]>([]);
    const [matchedSyncKeyKind, setMatchedSyncKeyKind] = useState<string | null>(null);
    const [focusPrefixTrigger, setFocusPrefixTrigger] = useState<number>(0);

    // File / Folder flow state
    const [sourcePath, setSourcePath] = useState<string>('');
    const [probe, setProbe] = useState<BackupProbeDto | null>(null);
    const [credentialMode, setCredentialMode] = useState<'password' | 'mnemonic'>('password');
    const [backupPassword, setBackupPassword] = useState<string>('');
    const [fileSeedState, setFileSeedState] = useState<SeedPhraseValue>(EMPTY_SEED_STATE);
    const [isDraggingBackup, setIsDraggingBackup] = useState<boolean>(false);

    // Shared Inspection / Collision / Credentials / Completion state
    const [inspection, setInspection] = useState<BackupInspectionDto | null>(null);
    const [activeCollision, setActiveCollision] = useState<LocalProfileCollisionDto | null>(null);
    const [collisionChoice, setCollisionChoice] = useState<'reset_pw' | 'replace'>('reset_pw');
    const [profileName, setProfileName] = useState<string>('');
    const [newPassword, setNewPassword] = useState<string>('');
    const [confirmNewPassword, setConfirmNewPassword] = useState<string>('');
    const [restoreOutcome, setRestoreOutcome] = useState<RestoreOutcomeDto | null>(null);

    // Busy & feedback flags
    const [isBusy, setIsBusy] = useState<boolean>(false);
    const [isReconcilingL2, setIsReconcilingL2] = useState<boolean>(false);
    const [feedback, setFeedback] = useState<{ type: 'error' | 'success'; message: string } | null>(
        null
    );

    // Load active social recovery session for resume banner
    useEffect(() => {
        if (method !== 'choose') return;
        let active = true;
        recoveryService
            .getActiveRecoverySession()
            .then((session) => {
                if (!active) return;
                if (session && (!session.sessionMode || session.sessionMode === 'restore_new_device')) {
                    setActiveSocialSession(session);
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

    // React to dynamic prop updates (deep links or dropped files)
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

    const resolveDefaultCollisionChoice = useCallback(
        (coll: LocalProfileCollisionDto, insp: BackupInspectionDto): 'reset_pw' | 'replace' => {
            const localEpoch = coll.localEpoch ?? 0;
            const localNonce = coll.localTxNonce ?? 0;
            const localNewerOrEqual =
                localEpoch > insp.epoch || (localEpoch === insp.epoch && localNonce >= insp.txNonce);
            if (localNewerOrEqual && !coll.isLockedOrConflicted) {
                return 'reset_pw';
            }
            return 'replace';
        },
        []
    );

    // --- Header Back Navigation ---
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

    // --- Card 2: Words / Sheet / Subaccount / Sync Handlers ---
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
            } else if (probeRes.status === 'not_found') {
                setInspection(null);
                setActiveCollision(probeRes.localCollision);
                setWordsPhase('sync_not_found');
            } else {
                setInspection(null);
                setActiveCollision(probeRes.localCollision);
                setWordsPhase('sync_unreachable');
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

        const trimmedSync = syncInput.trim();
        let effectiveSyncInputs: string[] = [];
        if (trimmedSync) {
            if (
                !syncEdited &&
                seedState.detectedKit &&
                seedState.detectedKit.syncDescriptorsJson.length > 0 &&
                trimmedSync === seedState.detectedKit.syncUrls.join(', ')
            ) {
                effectiveSyncInputs = seedState.detectedKit.syncDescriptorsJson;
            } else if (trimmedSync.startsWith('{')) {
                effectiveSyncInputs = [trimmedSync];
            } else {
                effectiveSyncInputs = trimmedSync
                    .split(/[\s,;\n]+/)
                    .map((s) => s.trim())
                    .filter(Boolean);
            }
        }

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
        if (newPassword.length < 8) {
            setFeedback({ type: 'error', message: t('profile.passwordMinLength') });
            return;
        }
        if (newPassword !== confirmNewPassword) {
            setFeedback({ type: 'error', message: t('profile.passwordMismatch') });
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

        if (newPassword.length < 8) {
            setFeedback({ type: 'error', message: t('profile.passwordMinLength') });
            return;
        }
        if (newPassword !== confirmNewPassword) {
            setFeedback({ type: 'error', message: t('profile.passwordMismatch') });
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

    // --- Card 3: File / Folder Handlers ---
    const handleSelectFile = async () => {
        setFeedback(null);
        try {
            const selected = await open({
                multiple: false,
                directory: false,
                filters: [{ name: 'Human Money Backup', extensions: ['hmcbackup'] }],
            });
            if (!selected) return;
            const path = typeof selected === 'string' ? selected : selected[0];
            await probeAndAdvanceFile(path);
        } catch (err) {
            logger.error(`Failed to open backup file: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleSelectFolder = async () => {
        setFeedback(null);
        try {
            const selected = await open({
                multiple: false,
                directory: true,
            });
            if (!selected) return;
            const path = typeof selected === 'string' ? selected : selected[0];
            await probeAndAdvanceFile(path);
        } catch (err) {
            logger.error(`Failed to open backup folder: ${stringifyError(err)}`);
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
                credentialMode,
                backupPassword: credentialMode === 'password' ? backupPassword : null,
                mnemonic: credentialMode === 'mnemonic' ? fileSeedState.mnemonic : null,
                passphrase:
                    credentialMode === 'mnemonic' && fileSeedState.passphrase
                        ? fileSeedState.passphrase
                        : null,
                userPrefix:
                    credentialMode === 'mnemonic' && fileSeedState.userPrefix.trim()
                        ? fileSeedState.userPrefix.trim()
                        : null,
                language: credentialMode === 'mnemonic' ? fileSeedState.language : null,
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

    // Shared restore / collision submit when an inspected backup exists (Sync or File/Folder)
    const handleRestoreFromInspectedBackup = async (e: React.FormEvent) => {
        e.preventDefault();
        setFeedback(null);

        if (newPassword.length < 8) {
            setFeedback({ type: 'error', message: t('profile.passwordMinLength') });
            return;
        }
        if (newPassword !== confirmNewPassword) {
            setFeedback({ type: 'error', message: t('profile.passwordMismatch') });
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
                    credentialMode: effectiveCredMode,
                    backupPassword: effectiveCredMode === 'password' ? backupPassword : null,
                    mnemonic: effectiveCredMode === 'mnemonic' ? activeCredSeed.mnemonic : null,
                    passphrase:
                        effectiveCredMode === 'mnemonic' && activeCredSeed.passphrase
                            ? activeCredSeed.passphrase
                            : null,
                    userPrefix:
                        effectiveCredMode === 'mnemonic' && activeCredSeed.userPrefix.trim()
                            ? activeCredSeed.userPrefix.trim()
                            : null,
                    language: effectiveCredMode === 'mnemonic' ? activeCredSeed.language : null,
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
                credentialMode: effectiveCredMode,
                backupPassword: effectiveCredMode === 'password' ? backupPassword : null,
                mnemonic: effectiveCredMode === 'mnemonic' ? activeCredSeed.mnemonic : null,
                passphrase:
                    effectiveCredMode === 'mnemonic' && activeCredSeed.passphrase
                        ? activeCredSeed.passphrase
                        : null,
                userPrefix:
                    effectiveCredMode === 'mnemonic' && activeCredSeed.userPrefix.trim()
                        ? activeCredSeed.userPrefix.trim()
                        : null,
                language: effectiveCredMode === 'mnemonic' ? activeCredSeed.language : null,
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
            if (res.l2ReconciliationStatus === 'reconciled') {
                setFeedback({
                    type: 'success',
                    message: t('profile.backupRestoreReconcileSuccess'),
                });
            } else {
                setFeedback({
                    type: 'error',
                    message: t('profile.backupRestoreReconcileOffline'),
                });
            }
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsReconcilingL2(false);
        }
    };

    const renderCollisionWithBackupCard = (coll: LocalProfileCollisionDto, insp: BackupInspectionDto) => {
        const localEpoch = coll.localEpoch ?? 0;
        const localNonce = coll.localTxNonce ?? 0;
        const localNewerOrEqual =
            localEpoch > insp.epoch || (localEpoch === insp.epoch && localNonce >= insp.txNonce);

        return (
            <div
                data-testid="local-collision-card"
                className="p-5 bg-amber-50 border border-amber-200 rounded-2xl space-y-4"
            >
                <div className="flex items-start gap-2.5">
                    <AlertTriangle size={18} className="text-amber-600 shrink-0 mt-0.5" />
                    <div className="space-y-1">
                        <h4 className="text-sm font-black text-amber-900">
                            {t('backup.collisionDetectedTitle', { name: coll.profileName })}
                        </h4>
                        <p className="text-xs text-amber-800 leading-relaxed">
                            {t('backup.collisionComparisonDesc', {
                                name: coll.profileName,
                                localEpoch,
                                localNonce,
                                backupEpoch: insp.epoch,
                                backupNonce: insp.txNonce,
                            })}
                        </p>
                    </div>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <button
                        type="button"
                        data-testid="collision-choice-reset-pw"
                        onClick={() => setCollisionChoice('reset_pw')}
                        className={`p-3.5 rounded-xl border-2 text-left transition-all cursor-pointer ${
                            collisionChoice === 'reset_pw'
                                ? 'bg-white border-theme-primary shadow-sm'
                                : 'bg-white/60 border-amber-200 hover:bg-white'
                        }`}
                    >
                        <div className="flex items-center justify-between gap-2 mb-1">
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('backup.collisionResetPasswordBtn', { name: coll.profileName })}
                            </span>
                            {localNewerOrEqual && !coll.isLockedOrConflicted && (
                                <span className="px-2 py-0.5 text-[10px] font-bold rounded-full bg-emerald-100 text-emerald-800">
                                    {t('common.recommended')}
                                </span>
                            )}
                        </div>
                        <p className="text-[11px] text-theme-light leading-relaxed">
                            {t('backup.collisionResetPasswordDesc')}
                        </p>
                    </button>

                    <button
                        type="button"
                        data-testid="collision-choice-replace"
                        onClick={() => setCollisionChoice('replace')}
                        className={`p-3.5 rounded-xl border-2 text-left transition-all cursor-pointer ${
                            collisionChoice === 'replace'
                                ? 'bg-white border-theme-primary shadow-sm'
                                : 'bg-white/60 border-amber-200 hover:bg-white'
                        }`}
                    >
                        <div className="flex items-center justify-between gap-2 mb-1">
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('backup.collisionReplaceProfileBtn', { name: coll.profileName })}
                            </span>
                            {(!localNewerOrEqual || coll.isLockedOrConflicted) && (
                                <span className="px-2 py-0.5 text-[10px] font-bold rounded-full bg-emerald-100 text-emerald-800">
                                    {t('common.recommended')}
                                </span>
                            )}
                        </div>
                        <p className="text-[11px] text-theme-light leading-relaxed">
                            {t('backup.collisionReplaceProfileDesc')}
                        </p>
                    </button>
                </div>
            </div>
        );
    };

    const renderCollisionWithoutBackupCard = (coll: LocalProfileCollisionDto) => (
        <div
            data-testid="local-collision-card"
            className="p-5 bg-amber-50 border border-amber-200 rounded-2xl space-y-5"
        >
            <div className="flex items-start gap-2.5">
                <AlertTriangle size={18} className="text-amber-600 shrink-0 mt-0.5" />
                <div className="space-y-1">
                    <h4 className="text-sm font-black text-amber-900">
                        {t('backup.collisionDetectedTitle', { name: coll.profileName })}
                    </h4>
                    <p className="text-xs text-amber-800 leading-relaxed">
                        {t('backup.collisionWithoutBackupDesc', { name: coll.profileName })}
                    </p>
                </div>
            </div>

            <form onSubmit={handleDirectCollisionResetPassword} className="space-y-3 bg-white p-4 rounded-2xl border border-amber-200">
                <h5 className="text-xs font-black uppercase tracking-wider text-theme-secondary">
                    {t('backup.collisionResetPasswordBtn', { name: coll.profileName })}
                </h5>
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    <Input
                        type="password"
                        showToggle
                        data-testid="collision-new-password-input"
                        value={newPassword}
                        onChange={(e) => setNewPassword(e.target.value)}
                        placeholder={t('profile.backupRestoreNewPasswordLabel')}
                        autoComplete="new-password"
                    />
                    <Input
                        type="password"
                        showToggle
                        data-testid="collision-confirm-password-input"
                        value={confirmNewPassword}
                        onChange={(e) => setConfirmNewPassword(e.target.value)}
                        placeholder={t('profile.backupRestoreConfirmPasswordLabel')}
                        autoComplete="new-password"
                    />
                </div>
                <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
                    <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        data-testid="collision-create-subaccount-btn"
                        onClick={handleSwitchToSubaccountFromCollision}
                    >
                        {t('backup.collisionCreateSubaccountBtn')}
                    </Button>
                    <Button
                        type="submit"
                        variant="primary"
                        size="sm"
                        data-testid="collision-reset-pw-direct-btn"
                        disabled={isBusy}
                    >
                        {isBusy ? t('common.loading') : t('backup.collisionResetPasswordAction')}
                    </Button>
                </div>
            </form>
        </div>
    );

    const renderInspectedManifestCard = (insp: BackupInspectionDto) => (
        <Card className="border-emerald-200 bg-emerald-50/50">
            <div className="space-y-4">
                <div className="flex items-center justify-between">
                    <span className="px-2.5 py-1 rounded-full text-[10px] font-black uppercase tracking-wider bg-emerald-600 text-white flex items-center gap-1.5 shadow-sm">
                        <CheckCircle2 size={12} />
                        {t('profile.backupRestoreVerifiedBadge')}
                    </span>
                    <span className="text-xs text-emerald-800 font-mono">
                        {new Date(insp.sealTimestamp).toLocaleString()}
                    </span>
                </div>

                <div>
                    <h3 className="text-base font-black text-theme-secondary">{insp.profileName}</h3>
                    <p className="text-[11px] font-mono text-theme-light truncate">
                        {t('profile.userId')}: {insp.userId}
                    </p>
                </div>

                <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 pt-2">
                    <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                        <FileText size={16} className="text-emerald-600 shrink-0" />
                        <div>
                            <p className="text-xs font-black text-theme-secondary">{insp.blobCount}</p>
                            <p className="text-[10px] text-theme-light">{t('backup.dataBlobs')}</p>
                        </div>
                    </div>
                    <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                        <History size={16} className="text-emerald-600 shrink-0" />
                        <div>
                            <p className="text-xs font-black text-theme-secondary">{insp.epoch}</p>
                            <p className="text-[10px] text-theme-light">{t('backup.epoch')}</p>
                        </div>
                    </div>
                    <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                        <Hash size={16} className="text-emerald-600 shrink-0" />
                        <div>
                            <p className="text-xs font-black text-theme-secondary">{insp.txNonce}</p>
                            <p className="text-[10px] text-theme-light">{t('backup.txNonce')}</p>
                        </div>
                    </div>
                    <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                        <Layers size={16} className="text-emerald-600 shrink-0" />
                        <div>
                            <p className="text-xs font-black text-theme-secondary">
                                {insp.customStandardCount}
                            </p>
                            <p className="text-[10px] text-theme-light">{t('backup.standards')}</p>
                        </div>
                    </div>
                </div>
            </div>
        </Card>
    );

    const renderDevicePasswordRestoreForm = (coll: LocalProfileCollisionDto | null) => (
        <form
            onSubmit={handleRestoreFromInspectedBackup}
            className="space-y-4 p-5 bg-white border border-theme-subtle rounded-3xl shadow-sm"
        >
            <div className="space-y-1">
                <h3 className="text-sm font-black uppercase tracking-widest text-theme-secondary flex items-center gap-2">
                    <Lock size={16} className="text-theme-primary" />
                    {t('profile.backupRestoreDevicePasswordTitle')}
                </h3>
                <p className="text-xs text-theme-light">
                    {t('profile.backupRestoreDevicePasswordDesc')}
                </p>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-2">
                <div>
                    <label className="block text-xs font-bold text-theme-secondary mb-1">
                        {t('profile.backupRestoreNewPasswordLabel')}
                    </label>
                    <Input
                        data-testid="new-device-password-input"
                        type="password"
                        showToggle
                        value={newPassword}
                        onChange={(e) => setNewPassword(e.target.value)}
                        placeholder={t('auth.passwordMinChars')}
                        autoComplete="new-password"
                    />
                </div>
                <div>
                    <label className="block text-xs font-bold text-theme-secondary mb-1">
                        {t('profile.backupRestoreConfirmPasswordLabel')}
                    </label>
                    <Input
                        data-testid="confirm-device-password-input"
                        type="password"
                        showToggle
                        value={confirmNewPassword}
                        onChange={(e) => setConfirmNewPassword(e.target.value)}
                        placeholder={t('auth.repeatPassword')}
                        autoComplete="new-password"
                    />
                </div>
            </div>

            <Button
                data-testid="restore-submit-btn"
                type="submit"
                variant="primary"
                disabled={isBusy}
                className="w-full mt-2"
            >
                {isBusy ? (
                    <>
                        <RefreshCw size={14} className="animate-spin mr-2" />
                        {t('profile.backupRestoreRestoring')}
                    </>
                ) : (
                    <>
                        <ShieldCheck size={16} className="mr-2" />
                        {coll && collisionChoice === 'reset_pw'
                            ? t('backup.collisionResetPasswordBtn', { name: coll.profileName })
                            : coll && collisionChoice === 'replace'
                              ? t('backup.collisionReplaceProfileBtn', { name: coll.profileName })
                              : t('profile.backupRestoreSubmitButton')}
                    </>
                )}
            </Button>
        </form>
    );

    const renderDirectProfileSetupForm = () => (
        <form
            onSubmit={handleDirectOpenProfile}
            className="space-y-4 p-5 bg-white border border-theme-subtle rounded-3xl shadow-sm"
        >
            <div className="space-y-1">
                <h3 className="text-sm font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                    <Sparkles size={16} className="text-theme-primary" />
                    {seedState.userPrefix.trim()
                        ? t('backup.directSubaccountTitle', { prefix: seedState.userPrefix.trim() })
                        : t('backup.directOpenProfileTitle')}
                </h3>
                <p className="text-xs text-theme-light">
                    {t('backup.directOpenProfileDesc')}
                </p>
            </div>

            {wordsPhase === 'direct_setup' && !seedState.userPrefix.trim() && !syncInput.trim() && (
                <AlertBanner
                    variant="info"
                    data-testid="direct-open-no-sync-hint"
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            size="xs"
                            data-testid="switch-to-social-from-words-btn"
                            onClick={() => setMethod('social')}
                        >
                            {t('backup.restorePrimaryContacts')}
                        </Button>
                    }
                >
                    {t('backup.directOpenNoSyncWarning')}
                </AlertBanner>
            )}

            <div className="space-y-1.5">
                <label className="block text-xs font-bold text-theme-secondary">
                    {t('backup.profileNameLabel')}
                </label>
                <Input
                    data-testid="direct-profile-name-input"
                    type="text"
                    value={profileName}
                    onChange={(e) => setProfileName(e.target.value)}
                    placeholder={t('profile.profileNamePlaceholder')}
                />
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                <div>
                    <label className="block text-xs font-bold text-theme-secondary mb-1">
                        {t('profile.backupRestoreNewPasswordLabel')}
                    </label>
                    <Input
                        data-testid="new-device-password-input"
                        type="password"
                        showToggle
                        value={newPassword}
                        onChange={(e) => setNewPassword(e.target.value)}
                        placeholder={t('auth.passwordMinChars')}
                        autoComplete="new-password"
                    />
                </div>
                <div>
                    <label className="block text-xs font-bold text-theme-secondary mb-1">
                        {t('profile.backupRestoreConfirmPasswordLabel')}
                    </label>
                    <Input
                        data-testid="confirm-device-password-input"
                        type="password"
                        showToggle
                        value={confirmNewPassword}
                        onChange={(e) => setConfirmNewPassword(e.target.value)}
                        placeholder={t('auth.repeatPassword')}
                        autoComplete="new-password"
                    />
                </div>
            </div>

            <Button
                data-testid="direct-open-profile-submit-btn"
                type="submit"
                variant="primary"
                disabled={isBusy}
                className="w-full mt-2"
            >
                {isBusy ? (
                    <>
                        <RefreshCw size={14} className="animate-spin mr-2" />
                        {t('profile.creatingProgress')}
                    </>
                ) : (
                    <>
                        <CheckCircle2 size={16} className="mr-2" />
                        {t('backup.directOpenProfileSubmit')}
                    </>
                )}
            </Button>
        </form>
    );

    return (
        <AuthLayout maxWidth="max-w-3xl">
            {/* Header */}
            <div className="flex flex-wrap items-center justify-between gap-3 pb-2 border-b border-theme-subtle">
                <div data-testid="restore-back-btn">
                    <BackButton onClick={handleHeaderBack} label={t('common.back')} />
                </div>
                <div className="text-right">
                    <span className="text-[10px] font-black uppercase tracking-widest text-theme-primary">
                        {t('backup.openRestoreBadge')}
                    </span>
                </div>
            </div>

            {/* Title */}
            <div className="text-center space-y-1">
                <h1 className="text-2xl font-black text-theme-secondary tracking-tight">
                    {t('profile.backupRestoreTitle')}
                </h1>
                <p className="text-xs text-theme-light font-medium">
                    {t('profile.backupRestoreSubtitle')}
                </p>
            </div>

            {/* Feedback Banner */}
            {feedback && (
                <AlertBanner variant={feedback.type} onDismiss={() => setFeedback(null)}>
                    {feedback.message}
                </AlertBanner>
            )}

            {/* TOP LEVEL: 3 Equal Cards */}
            {method === 'choose' && (
                <div className="space-y-5 animate-in fade-in duration-300">
                    {activeSocialSession && (
                        <AlertBanner
                            variant="info"
                            data-testid="resume-social-session-banner"
                            action={
                                <Button
                                    type="button"
                                    variant="primary"
                                    size="xs"
                                    data-testid="resume-social-session-btn"
                                    onClick={() => setMethod('social')}
                                >
                                    {t('backup.resumeSessionBtn')}
                                </Button>
                            }
                        >
                            {t('backup.resumeSessionDesc', {
                                code: activeSocialSession.sasCode,
                                collected: activeSocialSession.collectedCount,
                                required: activeSocialSession.thresholdM ?? '?',
                            })}
                        </AlertBanner>
                    )}

                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                        {/* Card 1: Trusted Contacts */}
                        <button
                            type="button"
                            onClick={() => setMethod('social')}
                            data-testid="choose-social-recovery-btn"
                            className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between cursor-pointer"
                        >
                            <div className="space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-105 transition-transform">
                                    <Users size={24} />
                                </div>
                                <h3 className="font-black text-base text-theme-secondary tracking-tight">
                                    {t('backup.restorePrimaryContacts')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('backup.restorePrimaryContactsDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1.5 text-xs font-bold text-theme-primary pt-4">
                                <span>{t('common.select')}</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>

                        {/* Card 2: Recovery Words / Emergency Sheet / Subaccount */}
                        <button
                            type="button"
                            onClick={() => {
                                setWordsPhase('input');
                                setMethod('words_or_sheet');
                            }}
                            data-testid="choose-words-or-sheet-btn"
                            className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between cursor-pointer"
                        >
                            <div className="space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-amber-500/10 text-amber-600 flex items-center justify-center group-hover:scale-105 transition-transform">
                                    <KeyRound size={24} />
                                </div>
                                <h3 className="font-black text-base text-theme-secondary tracking-tight">
                                    {t('backup.restoreCardWordsTitle')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('backup.restoreCardWordsDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1.5 text-xs font-bold text-amber-600 pt-4">
                                <span>{t('common.select')}</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>

                        {/* Card 3: Backup File / Folder */}
                        <button
                            type="button"
                            onClick={() => setMethod('file_or_folder')}
                            data-testid="choose-file-or-folder-btn"
                            className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between cursor-pointer"
                        >
                            <div className="space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-105 transition-transform">
                                    <FileArchive size={24} />
                                </div>
                                <h3 className="font-black text-base text-theme-secondary tracking-tight">
                                    {t('backup.restoreCardFileTitle')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('backup.restoreCardFileDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1.5 text-xs font-bold text-theme-primary pt-4">
                                <span>{t('common.select')}</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>
                    </div>
                </div>
            )}

            {/* BRANCH 1: Social Recovery */}
            {method === 'social' && (
                <SocialRecoveryFlow
                    mode="restore_new_device"
                    initialPayload={initialPayload}
                    existingSession={activeSocialSession}
                    onBack={() => setMethod('choose')}
                    onSuccess={(res) => {
                        if (res.outcome === 'restored_from_sync') {
                            setRestoreOutcome({
                                folderName: res.folderName,
                                profileName: res.profileName,
                                activeVoucherQueriesCount: res.activeVoucherQueriesCount,
                                reconciledSpentCount: res.reconciledSpentCount,
                                restoredStandardsCount: res.restoredStandardsCount,
                                l2ReconciliationStatus: res.l2ReconciliationStatus,
                            });
                            setMethod('completed');
                        } else {
                            onRestoreSuccess({
                                folderName: res.folderName,
                                profileName: res.profileName,
                                activeVoucherQueriesCount: res.activeVoucherQueriesCount,
                                reconciledSpentCount: res.reconciledSpentCount,
                                restoredStandardsCount: res.restoredStandardsCount,
                                l2ReconciliationStatus: res.l2ReconciliationStatus,
                            });
                        }
                    }}
                />
            )}

            {/* BRANCH 2: Words / Emergency Sheet / No Backup / Subaccount */}
            {method === 'words_or_sheet' && (
                <div className="space-y-5 animate-in fade-in duration-300">
                    <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/60">
                        <div className="flex items-center gap-2">
                            <KeyRound className="text-amber-600" size={20} />
                            <h3 className="font-black text-sm uppercase tracking-wider text-theme-secondary">
                                {t('backup.restoreCardWordsTitle')}
                            </h3>
                        </div>
                        <button
                            type="button"
                            onClick={handleHeaderBack}
                            className="text-xs font-bold text-theme-light hover:text-theme-secondary flex items-center gap-1 cursor-pointer"
                        >
                            <ArrowLeft size={14} />
                            <span>{t('common.back')}</span>
                        </button>
                    </div>

                    {wordsPhase === 'input' && (
                        <form onSubmit={handleWordsContinue} className="space-y-5">
                            <SeedPhraseInput
                                allowKitUri
                                showUserPrefix
                                initialPayload={initialPayload}
                                focusPrefixTrigger={focusPrefixTrigger}
                                onChange={handleSeedStateChange}
                                idPrefix="restore-words"
                            />

                            {/* Optional Sync Address */}
                            <div className="p-4 bg-white border border-theme-subtle rounded-2xl space-y-2 shadow-sm">
                                <label
                                    htmlFor="restore-sync-input"
                                    className="text-xs font-bold text-theme-secondary flex items-center gap-1.5"
                                >
                                    <Server size={14} className="text-theme-primary" />
                                    <span>{t('backup.optionalSyncAddressLabel')}</span>
                                </label>
                                <Input
                                    id="restore-sync-input"
                                    data-testid="restore-sync-input"
                                    type="text"
                                    value={syncInput}
                                    onChange={(e) => {
                                        setSyncInput(e.target.value);
                                        setSyncEdited(true);
                                    }}
                                    placeholder={t('profile.backupRestoreSyncAddressPlaceholder')}
                                    disabled={isBusy}
                                />
                                <p className="text-[11px] text-theme-light leading-relaxed">
                                    {t('backup.optionalSyncAddressHint')}
                                </p>
                            </div>

                            <Button
                                type="submit"
                                variant="primary"
                                data-testid="words-continue-btn"
                                disabled={!seedState.isValid || isBusy}
                                className="w-full py-3.5"
                            >
                                {isBusy ? (
                                    <>
                                        <RefreshCw size={15} className="animate-spin mr-2" />
                                        {t('profile.backupRestoreFetchingSync')}
                                    </>
                                ) : (
                                    <>
                                        <span>{t('profile.nextStep')}</span>
                                        <ArrowRight size={16} className="ml-2" />
                                    </>
                                )}
                            </Button>
                        </form>
                    )}

                    {/* Phase: Direct Open without Backup */}
                    {wordsPhase === 'direct_setup' && (
                        <div className="space-y-5">
                            {activeCollision
                                ? renderCollisionWithoutBackupCard(activeCollision)
                                : renderDirectProfileSetupForm()}
                        </div>
                    )}

                    {/* Phase: Sync Reachable, Slot Empty -> Calm Info + Open Profile + Subaccount/Passphrase Check */}
                    {wordsPhase === 'sync_not_found' && (
                        <div className="space-y-5">
                            <AlertBanner
                                variant="info"
                                data-testid="sync-not-found-banner"
                                action={
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="xs"
                                        data-testid="check-passphrase-prefix-btn"
                                        onClick={() => setWordsPhase('input')}
                                    >
                                        {t('backup.checkPassphraseOrPrefixBtn')}
                                    </Button>
                                }
                            >
                                {t('backup.syncNotFoundDesc')}
                            </AlertBanner>

                            {activeCollision
                                ? renderCollisionWithoutBackupCard(activeCollision)
                                : renderDirectProfileSetupForm()}
                        </div>
                    )}

                    {/* Phase: Sync Unreachable -> Hard Stop with Retry & Explicit Secondary Bypass */}
                    {wordsPhase === 'sync_unreachable' && (
                        <div
                            data-testid="sync-unreachable-card"
                            className="p-6 bg-amber-50 border border-amber-200 rounded-3xl space-y-4"
                        >
                            <div className="flex items-start gap-3">
                                <AlertTriangle size={20} className="text-amber-600 shrink-0 mt-0.5" />
                                <div className="space-y-1">
                                    <h4 className="text-sm font-black text-amber-900">
                                        {t('backup.syncUnreachableTitle')}
                                    </h4>
                                    <p className="text-xs text-amber-800 leading-relaxed">
                                        {syncProbe?.error || t('backup.syncUnreachableDesc')}
                                    </p>
                                </div>
                            </div>

                            <div className="flex flex-wrap items-center gap-2 pt-1">
                                <Button
                                    type="button"
                                    variant="primary"
                                    size="sm"
                                    data-testid="retry-sync-probe-btn"
                                    disabled={isBusy}
                                    onClick={() => void runSyncProbe(lastSyncInputs)}
                                >
                                    <RefreshCw size={14} className={`mr-1.5 ${isBusy ? 'animate-spin' : ''}`} />
                                    {t('common.retry')}
                                </Button>
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    data-testid="edit-sync-address-btn"
                                    onClick={() => setWordsPhase('input')}
                                >
                                    {t('backup.editSyncAddressBtn')}
                                </Button>
                                <button
                                    type="button"
                                    data-testid="proceed-without-backup-btn"
                                    onClick={() => {
                                        setResolvedSyncTargets([]);
                                        setWordsPhase('direct_setup');
                                    }}
                                    className="text-xs font-semibold text-theme-light hover:text-theme-secondary underline ml-auto cursor-pointer"
                                >
                                    {t('backup.proceedWithoutBackupLink')}
                                </button>
                            </div>
                        </div>
                    )}

                    {/* Phase: Sync Backup Found */}
                    {wordsPhase === 'sync_found' && inspection && (
                        <div className="space-y-5">
                            {matchedSyncKeyKind && (
                                <div className="flex items-center justify-between px-4 py-2.5 bg-slate-50 border border-slate-200 rounded-2xl text-xs">
                                    <span className="font-semibold text-theme-secondary flex items-center gap-2">
                                        <Server size={15} className="text-theme-primary" />
                                        {syncProbe?.sourceUrl}
                                    </span>
                                    <span className="px-2 py-0.5 rounded-full bg-theme-primary/10 text-theme-primary font-bold text-[10px] uppercase">
                                        {matchedSyncKeyKind}
                                    </span>
                                </div>
                            )}

                            {renderInspectedManifestCard(inspection)}
                            {activeCollision && renderCollisionWithBackupCard(activeCollision, inspection)}
                            {renderDevicePasswordRestoreForm(activeCollision)}
                        </div>
                    )}
                </div>
            )}

            {/* BRANCH 3: File / Folder Restore */}
            {method === 'file_or_folder' && (
                <div className="space-y-5 animate-in fade-in duration-300">
                    <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/60">
                        <div className="flex items-center gap-2">
                            <FileArchive className="text-theme-primary" size={20} />
                            <h3 className="font-black text-sm uppercase tracking-wider text-theme-secondary">
                                {t('backup.restoreCardFileTitle')}
                            </h3>
                        </div>
                        <button
                            type="button"
                            onClick={handleHeaderBack}
                            className="text-xs font-bold text-theme-light hover:text-theme-secondary flex items-center gap-1 cursor-pointer"
                        >
                            <ArrowLeft size={14} />
                            <span>{t('common.back')}</span>
                        </button>
                    </div>

                    {!probe && (
                        <div className="space-y-4">
                            <div
                                data-testid="backup-file-dropzone"
                                onDragEnter={(e) => {
                                    e.preventDefault();
                                    setIsDraggingBackup(true);
                                }}
                                onDragOver={(e) => e.preventDefault()}
                                onDragLeave={(e) => {
                                    e.preventDefault();
                                    setIsDraggingBackup(false);
                                }}
                                onDrop={(e) => void handleInlineDrop(e)}
                                className={`p-8 border-2 border-dashed rounded-3xl text-center space-y-3 transition-all ${
                                    isDraggingBackup
                                        ? 'border-theme-primary bg-theme-primary/10'
                                        : 'border-theme-subtle bg-slate-50/70'
                                }`}
                            >
                                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center mx-auto">
                                    <UploadCloud size={24} />
                                </div>
                                <div className="space-y-1">
                                    <p className="text-sm font-bold text-theme-secondary">
                                        {t('backup.dropBackupHereTitle')}
                                    </p>
                                    <p className="text-xs text-theme-light">
                                        {t('backup.dropBackupHereDesc')}
                                    </p>
                                </div>
                            </div>

                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                <button
                                    type="button"
                                    data-testid="select-backup-file-btn"
                                    onClick={handleSelectFile}
                                    className="p-5 bg-white border-2 border-theme-subtle hover:border-theme-primary/50 hover:bg-theme-primary/5 rounded-3xl text-left transition-all flex items-center gap-4 cursor-pointer shadow-sm"
                                >
                                    <div className="w-11 h-11 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center shrink-0">
                                        <FileArchive size={22} />
                                    </div>
                                    <div className="space-y-0.5 min-w-0">
                                        <h4 className="text-sm font-black text-theme-secondary">
                                            {t('profile.backupRestoreSelectFile')}
                                        </h4>
                                        <p className="text-xs text-theme-light">
                                            {t('profile.backupRestoreSelectFileDesc')}
                                        </p>
                                    </div>
                                </button>

                                <button
                                    type="button"
                                    data-testid="select-backup-folder-btn"
                                    onClick={handleSelectFolder}
                                    className="p-5 bg-white border-2 border-theme-subtle hover:border-theme-primary/50 hover:bg-theme-primary/5 rounded-3xl text-left transition-all flex items-center gap-4 cursor-pointer shadow-sm"
                                >
                                    <div className="w-11 h-11 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center shrink-0">
                                        <FolderSync size={22} />
                                    </div>
                                    <div className="space-y-0.5 min-w-0">
                                        <h4 className="text-sm font-black text-theme-secondary">
                                            {t('profile.backupRestoreSelectFolder')}
                                        </h4>
                                        <p className="text-xs text-theme-light">
                                            {t('profile.backupRestoreSelectFolderDesc')}
                                        </p>
                                    </div>
                                </button>
                            </div>
                        </div>
                    )}

                    {probe && (
                        <div className="space-y-5">
                            <div className="p-4 bg-slate-50 border border-slate-200 rounded-2xl flex items-center justify-between gap-4">
                                <div className="flex items-center gap-3 min-w-0">
                                    <Folder size={18} className="text-theme-primary shrink-0" />
                                    <div className="min-w-0">
                                        <p className="text-[10px] font-black uppercase tracking-wider text-theme-light">
                                            {probe.sourceKind === 'folder'
                                                ? t('backup.sourceKindFolder')
                                                : t('backup.sourceKindArchive')}
                                        </p>
                                        <p className="text-xs font-mono text-theme-secondary truncate max-w-md">
                                            {sourcePath}
                                        </p>
                                    </div>
                                </div>
                                <Button
                                    variant="secondary"
                                    size="xs"
                                    onClick={() => {
                                        setProbe(null);
                                        setInspection(null);
                                        setActiveCollision(null);
                                        setSourcePath('');
                                    }}
                                >
                                    {t('common.change')}
                                </Button>
                            </div>

                            {/* Unlock credentials */}
                            {!inspection && (
                                <div className="space-y-4">
                                    <div className="flex items-center justify-between flex-wrap gap-2">
                                        <h4 className="text-sm font-black uppercase tracking-widest text-theme-secondary flex items-center gap-2">
                                            <KeyRound size={16} className="text-theme-primary" />
                                            {t('profile.backupRestoreUnlockTitle')}
                                        </h4>

                                        {probe.supportsPasswordRestore && (
                                            <div className="flex bg-slate-100 p-1 rounded-xl">
                                                <button
                                                    type="button"
                                                    onClick={() => setCredentialMode('password')}
                                                    className={`px-3 py-1 text-xs font-bold rounded-lg transition-all cursor-pointer ${
                                                        credentialMode === 'password'
                                                            ? 'bg-white text-theme-primary shadow-sm'
                                                            : 'text-theme-light hover:text-theme-secondary'
                                                    }`}
                                                >
                                                    {t('profile.backupRestorePasswordTab')}
                                                </button>
                                                <button
                                                    type="button"
                                                    onClick={() => setCredentialMode('mnemonic')}
                                                    className={`px-3 py-1 text-xs font-bold rounded-lg transition-all cursor-pointer ${
                                                        credentialMode === 'mnemonic'
                                                            ? 'bg-white text-theme-primary shadow-sm'
                                                            : 'text-theme-light hover:text-theme-secondary'
                                                    }`}
                                                >
                                                    {t('profile.backupRestoreMnemonicTab')}
                                                </button>
                                            </div>
                                        )}
                                    </div>

                                    {credentialMode === 'password' ? (
                                        <div className="space-y-2 p-4 bg-white border border-theme-subtle rounded-2xl">
                                            <label className="block text-xs font-bold text-theme-secondary">
                                                {t('settings.backupPasswordLabel')}
                                            </label>
                                            <Input
                                                data-testid="backup-password-input"
                                                type="password"
                                                showToggle
                                                value={backupPassword}
                                                onChange={(e) => setBackupPassword(e.target.value)}
                                                placeholder={t('settings.backupPasswordPlaceholder')}
                                                className="font-mono"
                                            />
                                        </div>
                                    ) : (
                                        <SeedPhraseInput
                                            allowKitUri={false}
                                            showUserPrefix
                                            idPrefix="restore-file"
                                            onChange={setFileSeedState}
                                        />
                                    )}

                                    <Button
                                        variant="primary"
                                        data-testid="inspect-file-backup-btn"
                                        onClick={handleInspectFileBackup}
                                        disabled={isBusy}
                                        className="w-full"
                                    >
                                        {isBusy ? (
                                            <>
                                                <RefreshCw size={14} className="animate-spin mr-2" />
                                                {t('profile.backupRestoreVerifying')}
                                            </>
                                        ) : (
                                            <>
                                                <ShieldCheck size={16} className="mr-2" />
                                                {t('profile.backupRestoreVerifyButton')}
                                            </>
                                        )}
                                    </Button>
                                </div>
                            )}

                            {inspection && (
                                <div className="space-y-5">
                                    {renderInspectedManifestCard(inspection)}
                                    {activeCollision &&
                                        renderCollisionWithBackupCard(activeCollision, inspection)}
                                    {renderDevicePasswordRestoreForm(activeCollision)}
                                </div>
                            )}
                        </div>
                    )}
                </div>
            )}

            {/* UNIFIED COMPLETION STATE */}
            {method === 'completed' && restoreOutcome && (
                <div className="space-y-6 text-center animate-in zoom-in-95 duration-300">
                    <div className="w-16 h-16 rounded-full bg-emerald-100 text-emerald-600 flex items-center justify-center mx-auto shadow-inner">
                        <CheckCircle2 size={36} />
                    </div>

                    <div className="space-y-1">
                        <h2 className="text-2xl font-black text-theme-secondary">
                            {t('profile.backupRestoreSuccessTitle')}
                        </h2>
                        <p className="text-xs text-theme-light max-w-md mx-auto">
                            {t('profile.backupRestoreSuccessDesc')}
                        </p>
                    </div>

                    <div className="grid grid-cols-3 gap-3 p-4 bg-slate-50 border border-slate-200 rounded-2xl max-w-md mx-auto">
                        <div>
                            <p className="text-lg font-black text-theme-primary">
                                {restoreOutcome.activeVoucherQueriesCount}
                            </p>
                            <p className="text-[10px] uppercase font-bold text-theme-light">
                                {t('backup.active')}
                            </p>
                        </div>
                        <div>
                            <p className="text-lg font-black text-slate-700">
                                {restoreOutcome.reconciledSpentCount}
                            </p>
                            <p className="text-[10px] uppercase font-bold text-theme-light">
                                {t('backup.spent')}
                            </p>
                        </div>
                        <div>
                            <p className="text-lg font-black text-slate-700">
                                {restoreOutcome.restoredStandardsCount}
                            </p>
                            <p className="text-[10px] uppercase font-bold text-theme-light">
                                {t('backup.standards')}
                            </p>
                        </div>
                    </div>

                    <div className="p-3.5 rounded-xl bg-slate-100 text-xs text-theme-secondary max-w-md mx-auto leading-relaxed">
                        {restoreOutcome.l2ReconciliationStatus === 'no_servers'
                            ? t('profile.backupRestoreOfflineNotice')
                            : t('profile.backupRestoreReconciledNotice', {
                                  active: restoreOutcome.activeVoucherQueriesCount,
                                  spent: restoreOutcome.reconciledSpentCount,
                              })}
                    </div>

                    {restoreOutcome.l2ReconciliationStatus !== 'reconciled' &&
                        restoreOutcome.activeVoucherQueriesCount > 0 && (
                            <div className="p-4 rounded-2xl bg-amber-50 border border-amber-200 text-amber-900 text-xs text-left space-y-2 max-w-md mx-auto">
                                <div className="flex items-center gap-2 font-bold text-amber-800">
                                    <AlertTriangle size={16} className="shrink-0" />
                                    <span>{t('profile.backupRestoreL2WarningTitle')}</span>
                                </div>
                                <p className="leading-relaxed">
                                    {t('profile.backupRestoreL2WarningDesc')}
                                </p>
                                <Button
                                    data-testid="restore-reconcile-l2-btn"
                                    variant="secondary"
                                    size="sm"
                                    disabled={isReconcilingL2}
                                    onClick={handleReconcileL2}
                                    className="w-full mt-1"
                                >
                                    {isReconcilingL2 ? (
                                        <>
                                            <RefreshCw size={14} className="animate-spin mr-2" />
                                            {t('profile.backupRestoreReconcilingL2')}
                                        </>
                                    ) : (
                                        <>
                                            <RefreshCw size={14} className="mr-2" />
                                            {t('profile.backupRestoreReconcileL2Btn')}
                                        </>
                                    )}
                                </Button>
                            </div>
                        )}

                    <Button
                        variant="primary"
                        onClick={() => onRestoreSuccess(restoreOutcome)}
                        className="w-full max-w-xs mx-auto py-3 text-sm shadow-md"
                    >
                        {t('profile.backupRestoreOpenWallet')}
                    </Button>
                </div>
            )}
        </AuthLayout>
    );
};
