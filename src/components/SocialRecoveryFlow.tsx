// src/components/SocialRecoveryFlow.tsx
import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { AlertBanner } from './ui/AlertBanner';
import { QrScannerModal } from './ui/QrScannerModal';
import { recoveryService } from '../services/recoveryService';
import { authService } from '../services/authService';
import { profileService } from '../services/profileService';
import { ActiveRecoverySessionDto, RecoveryCompletionResultDto } from '../types';
import { copyToClipboard } from '../utils/clipboard';
import { translateError } from '../utils/errorHelper';
import {
    Users,
    Copy,
    CheckCircle2,
    RefreshCw,
    X,
    ArrowLeft,
    AlertTriangle,
    KeyRound,
    MessageSquareShare,
    Clock,
    ShieldCheck,
    Server,
    QrCode,
} from 'lucide-react';

export interface SocialRecoveryFlowProps {
    mode: 'restore_new_device' | 'reset_local_password';
    targetFolderName?: string;
    initialPayload?: string;
    existingSession?: ActiveRecoverySessionDto | null;
    onSuccess: (result: RecoveryCompletionResultDto) => void;
    onBack?: () => void;
}

const QUESTION_CATALOG: Record<number, string> = {
    1: 'backup.recoveryQuestion1',
    2: 'backup.recoveryQuestion2',
    3: 'backup.recoveryQuestion3',
    4: 'backup.recoveryQuestion4',
    5: 'backup.recoveryQuestion5',
    6: 'backup.recoveryQuestion6',
};

export const SocialRecoveryFlow: React.FC<SocialRecoveryFlowProps> = ({
    mode,
    targetFolderName,
    initialPayload,
    existingSession,
    onSuccess,
    onBack,
}) => {
    const { t } = useTranslation();

    const [session, setSession] = useState<ActiveRecoverySessionDto | null>(
        existingSession ?? null
    );
    const [conflictingSession, setConflictingSession] =
        useState<ActiveRecoverySessionDto | null>(null);
    const [replyInput, setReplyInput] = useState<string>('');
    const [answers, setAnswers] = useState<[string, string, string]>(['', '', '']);
    const [bip39Passphrase, setBip39Passphrase] = useState<string>('');
    const [showPassphraseInput, setShowPassphraseInput] = useState<boolean>(false);

    const [profileName, setProfileName] = useState<string>(
        existingSession?.ownerDisplayName ?? ''
    );
    const [newPassword, setNewPassword] = useState<string>('');
    const [confirmNewPassword, setConfirmNewPassword] = useState<string>('');

    const [collisionResult, setCollisionResult] =
        useState<RecoveryCompletionResultDto | null>(null);
    const [syncEmptyResult, setSyncEmptyResult] =
        useState<RecoveryCompletionResultDto | null>(null);
    const [unreachableSyncError, setUnreachableSyncError] = useState<boolean>(false);

    const [isLoadingSession, setIsLoadingSession] = useState<boolean>(false);
    const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
    const [errorMsg, setErrorMsg] = useState<string>('');
    const [successMsg, setSuccessMsg] = useState<string>('');
    const [infoMsg, setInfoMsg] = useState<string>('');
    const [copiedRequest, setCopiedRequest] = useState<boolean>(false);
    const [copiedChatMsg, setCopiedChatMsg] = useState<boolean>(false);
    const [confirmCancel, setConfirmCancel] = useState<boolean>(false);
    const [requestQrSvg, setRequestQrSvg] = useState<string | null>(null);
    const [showRequestQr, setShowRequestQr] = useState<boolean>(false);
    const [isReplyQrOpen, setIsReplyQrOpen] = useState<boolean>(false);
    const autoImportedRef = useRef<string | null>(null);

    useEffect(() => {
        if (existingSession) {
            setSession(existingSession);
            if (existingSession.ownerDisplayName && !profileName) {
                setProfileName(existingSession.ownerDisplayName);
            }
            return;
        }

        let active = true;
        setIsLoadingSession(true);
        recoveryService
            .getActiveRecoverySession()
            .then((found) => {
                if (!active) return;
                if (found && (!found.sessionMode || found.sessionMode === mode)) {
                    setSession(found);
                    setConflictingSession(null);
                    if (found.ownerDisplayName) {
                        setProfileName((prev) => prev || found.ownerDisplayName || '');
                    }
                } else if (found && found.sessionMode && found.sessionMode !== mode) {
                    setConflictingSession(found);
                }
            })
            .catch(() => {
                // Ignore initial session load failure
            })
            .finally(() => {
                if (active) setIsLoadingSession(false);
            });

        return () => {
            active = false;
        };
    }, [existingSession, mode, profileName]);

    const handleImportReply = useCallback(
        async (uriToImport?: string) => {
            const uri = (uriToImport ?? replyInput).trim();
            if (!uri || isSubmitting) return;

            setIsSubmitting(true);
            setErrorMsg('');
            setSuccessMsg('');
            setInfoMsg('');

            try {
                const updated = await recoveryService.importGuardianRecoveryReply(uri);
                setSession(updated);
                setReplyInput('');
                if (updated.ownerDisplayName) {
                    setProfileName((prev) => prev || updated.ownerDisplayName || '');
                }
                if (updated.lastImportStatus === 'duplicate') {
                    setInfoMsg(t('backup.importReplyDuplicate'));
                } else {
                    setSuccessMsg(
                        t('backup.importReplySuccess', {
                            name: updated.ownerDisplayName || t('common.unknown'),
                        })
                    );
                }
            } catch (err) {
                setErrorMsg(translateError(err, t));
            } finally {
                setIsSubmitting(false);
            }
        },
        [replyInput, isSubmitting, t]
    );

    useEffect(() => {
        if (
            initialPayload?.includes('humoco://recovery-reply') &&
            session &&
            !session.hasEnoughShares &&
            autoImportedRef.current !== initialPayload
        ) {
            autoImportedRef.current = initialPayload;
            void handleImportReply(initialPayload);
        }
    }, [initialPayload, session, handleImportReply]);

    const handleStartSession = async () => {
        setIsSubmitting(true);
        setErrorMsg('');
        setInfoMsg('');
        setCollisionResult(null);
        setSyncEmptyResult(null);
        setUnreachableSyncError(false);

        try {
            const newSession = await recoveryService.startSocialRecoverySession(
                undefined,
                mode,
                targetFolderName
            );
            setSession(newSession);
            setConflictingSession(null);
            setConfirmCancel(false);
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSubmitting(false);
        }
    };

    const handleCancelSession = async () => {
        setIsSubmitting(true);
        setErrorMsg('');
        setInfoMsg('');
        setSuccessMsg('');
        try {
            await recoveryService.cancelSocialRecoverySession();
            setSession(null);
            setConfirmCancel(false);
            setAnswers(['', '', '']);
            setReplyInput('');
            setCollisionResult(null);
            setSyncEmptyResult(null);
            setUnreachableSyncError(false);
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSubmitting(false);
        }
    };

    const handleCopyRequestLink = async () => {
        if (!session?.requestUri) return;
        await copyToClipboard(session.requestUri);
        setCopiedRequest(true);
        setTimeout(() => setCopiedRequest(false), 3000);
    };

    const handleCopyRequestChatMessage = async () => {
        if (!session?.requestUri) return;
        const msg = t('backup.requestChatTemplate', {
            requestUri: session.requestUri,
            sasCode: session.sasCode,
        });
        await copyToClipboard(msg);
        setCopiedChatMsg(true);
        setTimeout(() => setCopiedChatMsg(false), 3000);
    };

    const submitRecoveryCompletion = async (options?: {
        overrideMode?: 'restore_new_device' | 'reset_local_password';
        overrideTargetFolder?: string;
        allowEmptyFallback?: boolean;
        replaceFolderName?: string;
    }) => {
        if (
            !session?.ownerDid ||
            session.rotationEpoch == null ||
            newPassword.length < 8 ||
            newPassword !== confirmNewPassword ||
            isSubmitting
        ) {
            return;
        }

        const effectiveMode = options?.overrideMode ?? mode;
        const effectiveTargetFolder = options?.overrideTargetFolder ?? targetFolderName;

        setIsSubmitting(true);
        setErrorMsg('');
        setUnreachableSyncError(false);

        try {
            const localInstanceId = await authService.getLocalInstanceId();
            const result = await recoveryService.completeSocialRecovery({
                ownerDid: session.ownerDid,
                rotationEpoch: session.rotationEpoch,
                answers: [answers[0].trim(), answers[1].trim(), answers[2].trim()],
                bip39Passphrase: bip39Passphrase.trim() || undefined,
                mode: effectiveMode,
                newPassword,
                profileName:
                    effectiveMode === 'restore_new_device'
                        ? profileName.trim() || undefined
                        : undefined,
                targetFolderName: effectiveTargetFolder,
                localInstanceId,
                allowEmptyFallback: options?.allowEmptyFallback,
                replaceFolderName: options?.replaceFolderName,
            });

            if (result.outcome === 'collision_requires_choice' && result.localCollision) {
                setCollisionResult(result);
                return;
            }
            if (result.outcome === 'sync_empty_requires_confirm') {
                setSyncEmptyResult(result);
                return;
            }

            onSuccess(result);
        } catch (err) {
            const msg = String(err);
            if (msg.includes('BIP39_PASSPHRASE_REQUIRED') || msg.includes('Passphrase erforderlich')) {
                setShowPassphraseInput(true);
                setErrorMsg(t('backup.bip39PassphraseRequired'));
            } else if (
                msg.includes('Antworten stimmen noch nicht') ||
                msg.includes('Decryption failed')
            ) {
                setErrorMsg(t('backup.wrongAnswersRetry'));
            } else if (msg.includes('Sync-Dienst nicht erreichbar')) {
                setUnreachableSyncError(true);
                setErrorMsg(translateError(err, t));
            } else {
                setErrorMsg(translateError(err, t));
            }
        } finally {
            setIsSubmitting(false);
        }
    };

    const handleFormSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        setCollisionResult(null);
        setSyncEmptyResult(null);
        void submitRecoveryCompletion();
    };

    const isPasswordValid = newPassword.length >= 8 && newPassword === confirmNewPassword;
    const isProfileNameValid =
        mode === 'reset_local_password' || profileName.trim().length > 0;

    return (
        <div className="space-y-5 animate-in fade-in duration-300">
            <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/60">
                <div className="flex items-center gap-2">
                    <Users className="text-theme-primary" size={20} />
                    <h3 className="font-black text-sm uppercase tracking-wider text-theme-secondary">
                        {t('backup.restorePrimaryContacts')}
                    </h3>
                </div>
                {onBack && (
                    <button
                        type="button"
                        onClick={onBack}
                        data-testid="social-flow-back-btn"
                        className="text-xs font-bold text-theme-light hover:text-theme-secondary flex items-center gap-1 cursor-pointer"
                    >
                        <ArrowLeft size={14} />
                        <span>{t('common.back')}</span>
                    </button>
                )}
            </div>

            {errorMsg && (
                <AlertBanner variant="error" onDismiss={() => setErrorMsg('')}>
                    {errorMsg}
                </AlertBanner>
            )}
            {successMsg && (
                <AlertBanner variant="success" onDismiss={() => setSuccessMsg('')}>
                    {successMsg}
                </AlertBanner>
            )}
            {infoMsg && (
                <AlertBanner variant="info" onDismiss={() => setInfoMsg('')}>
                    {infoMsg}
                </AlertBanner>
            )}

            {/* Start Session */}
            {!session && (
                <div className="space-y-4">
                    {conflictingSession && (
                        <AlertBanner variant="warning" data-testid="social-conflicting-session-warning">
                            {t('backup.conflictingSessionWarning')}
                        </AlertBanner>
                    )}
                    <p className="text-xs text-theme-light leading-relaxed">
                        {t('backup.socialRecoveryDesc')}
                    </p>
                    <div className="p-4 bg-bg-surface border border-theme-subtle rounded-2xl space-y-2 text-xs text-theme-secondary">
                        <p className="font-bold text-theme-secondary">{t('backup.socialOverviewTitle')}</p>
                        <ul className="space-y-1.5 text-theme-light leading-relaxed">
                            <li>{t('backup.socialOverviewStep1')}</li>
                            <li>{t('backup.socialOverviewStep2')}</li>
                            <li>{t('backup.socialOverviewStep3')}</li>
                        </ul>
                    </div>
                    <Button
                        type="button"
                        variant="primary"
                        data-testid="start-social-recovery-btn"
                        onClick={handleStartSession}
                        disabled={isSubmitting || isLoadingSession}
                        className="flex items-center gap-2"
                    >
                        <Users size={16} />
                        {isSubmitting ? t('common.loading') : t('backup.startRecoverySession')}
                    </Button>
                </div>
            )}

            {/* Active Session */}
            {session && (
                <div className="space-y-5">
                    {/* Step 1: Send Request Link */}
                    <div className="p-4 bg-bg-surface border border-theme-subtle rounded-2xl space-y-3">
                        <div className="space-y-1">
                            <h4 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                                <MessageSquareShare size={15} className="text-theme-primary" />
                                {t('backup.socialStep1Title')}
                            </h4>
                            <p className="text-xs text-theme-light leading-relaxed">
                                {t('backup.socialStep1Desc')}
                            </p>
                        </div>
                        <Input
                            readOnly
                            value={session.requestUri}
                            className="text-[11px] font-mono select-all bg-bg-app"
                        />
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                variant="primary"
                                size="sm"
                                data-testid="copy-social-chat-msg-btn"
                                onClick={handleCopyRequestChatMessage}
                                className="flex items-center gap-1.5 text-xs"
                            >
                                <Copy size={14} />
                                {copiedChatMsg
                                    ? t('backup.requestChatMessageCopied')
                                    : t('backup.copyRequestChatMessage')}
                            </Button>
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={handleCopyRequestLink}
                                className="flex items-center gap-1.5 text-xs"
                            >
                                <Copy size={14} />
                                {copiedRequest
                                    ? t('backup.requestLinkCopied')
                                    : t('backup.copyRequestLink')}
                            </Button>
                            <Button
                                type="button"
                                variant="outline"
                                size="sm"
                                data-testid="social-toggle-request-qr-btn"
                                onClick={async () => {
                                    if (showRequestQr) {
                                        setShowRequestQr(false);
                                        return;
                                    }
                                    if (!requestQrSvg && session.requestUri) {
                                        try {
                                            const svg = await profileService.encodeTextQrSvg(session.requestUri);
                                            setRequestQrSvg(svg);
                                        } catch {
                                            return;
                                        }
                                    }
                                    setShowRequestQr(true);
                                }}
                                className="flex items-center gap-1.5 text-xs"
                            >
                                <QrCode size={14} />
                                {showRequestQr ? t('common.hideQrCode') : t('common.showQrCode')}
                            </Button>
                        </div>
                        {showRequestQr && requestQrSvg && (
                            <div className="pt-2 flex flex-col items-center gap-2">
                                <div
                                    className="w-48 h-48 p-2 bg-white rounded-xl border border-theme-subtle shadow-xs [&>svg]:w-full [&>svg]:h-full"
                                    dangerouslySetInnerHTML={{ __html: requestQrSvg }}
                                />
                            </div>
                        )}
                    </div>

                    {/* Step 2: Check Code & Multi-Day Pause */}
                    <div className="p-4 bg-theme-primary/5 border border-theme-primary/20 rounded-2xl space-y-3">
                        <h4 className="text-xs font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                            <ShieldCheck size={15} className="text-theme-primary" />
                            {t('backup.socialVerifyTitle')}
                        </h4>
                        <div className="p-3.5 bg-white/80 dark:bg-slate-900/60 border border-theme-primary/20 rounded-xl flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                            <div>
                                <span className="text-[10px] font-bold uppercase tracking-wider text-theme-light block">
                                    {t('backup.releaseSasCodeLabel')}
                                </span>
                                <span
                                    data-testid="social-sas-code"
                                    className="text-2xl font-black tracking-widest text-theme-primary font-mono"
                                >
                                    {session.sasCode}
                                </span>
                            </div>
                            <p className="text-xs text-theme-light leading-relaxed max-w-md">
                                {t('backup.socialSasExplanation')}
                            </p>
                        </div>

                        <div className="p-3 bg-blue-50/70 dark:bg-blue-950/30 border border-blue-200/70 dark:border-blue-800/40 rounded-xl space-y-2.5">
                            <div className="flex items-start gap-2 text-xs text-blue-900 dark:text-blue-200 leading-relaxed">
                                <Clock size={15} className="text-blue-600 dark:text-blue-400 shrink-0 mt-0.5" />
                                <span>{t('backup.socialPauseHint')}</span>
                            </div>
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                {onBack && (
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        data-testid="pause-social-session-btn"
                                        onClick={onBack}
                                        className="text-xs"
                                    >
                                        {t('backup.pauseSessionButton')}
                                    </Button>
                                )}
                                {!confirmCancel ? (
                                    <button
                                        type="button"
                                        data-testid="request-cancel-session-btn"
                                        onClick={() => setConfirmCancel(true)}
                                        className="text-xs font-semibold text-rose-500 hover:text-rose-600 underline cursor-pointer ml-auto"
                                    >
                                        {t('backup.cancelSessionButton')}
                                    </button>
                                ) : (
                                    <div className="flex flex-wrap items-center gap-2 ml-auto bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 px-3 py-1.5 rounded-xl">
                                        <span className="text-[11px] font-medium text-rose-800 dark:text-rose-200">
                                            {t('backup.cancelSessionConfirmWarning')}
                                        </span>
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="xs"
                                            onClick={() => setConfirmCancel(false)}
                                        >
                                            {t('common.abort')}
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="danger"
                                            size="xs"
                                            data-testid="confirm-cancel-session-btn"
                                            onClick={handleCancelSession}
                                        >
                                            <X size={12} className="mr-1" />
                                            {t('backup.confirmCancelSessionButton')}
                                        </Button>
                                    </div>
                                )}
                            </div>
                        </div>
                    </div>

                    {/* Step 3: Approval Progress & Reply Paste */}
                    <div className="p-4 bg-bg-surface border border-theme-subtle rounded-2xl space-y-3">
                        <div className="flex items-center justify-between gap-2 flex-wrap">
                            <div className="space-y-0.5">
                                <h4 className="text-xs font-black uppercase tracking-wider text-theme-secondary">
                                    {t('backup.socialStep2Title')}
                                </h4>
                                <span className="text-xs font-bold text-theme-secondary block">
                                    {session.thresholdM
                                        ? t('backup.socialSharesProgressDetailed', {
                                              collected: session.collectedCount,
                                              required: session.thresholdM,
                                              total: session.totalN ?? session.thresholdM,
                                          })
                                        : t('backup.socialSharesProgressInitial', {
                                              collected: session.collectedCount,
                                          })}
                                </span>
                            </div>
                            {session.hasEnoughShares ? (
                                <span className="px-2.5 py-1 rounded-full bg-emerald-500/10 text-emerald-600 font-bold text-xs flex items-center gap-1">
                                    <CheckCircle2 size={14} />
                                    {t('backup.socialReadyBadge')}
                                </span>
                            ) : (
                                <span className="px-2.5 py-1 rounded-full bg-amber-500/10 text-amber-600 font-bold text-xs flex items-center gap-1">
                                    <RefreshCw size={14} className="animate-spin" />
                                    {t('backup.socialPendingBadge')}
                                </span>
                            )}
                        </div>

                        {session.collectedShareIndices && session.collectedShareIndices.length > 0 && (
                            <div className="flex flex-wrap gap-1.5">
                                {session.collectedShareIndices.map((idx) => (
                                    <span
                                        key={idx}
                                        data-testid={`collected-share-badge-${idx}`}
                                        className="px-2 py-0.5 rounded-full bg-emerald-500/10 border border-emerald-500/20 text-emerald-700 dark:text-emerald-300 text-[11px] font-bold flex items-center gap-1"
                                    >
                                        <CheckCircle2 size={12} />
                                        {t('backup.collectedShareBadge', { index: idx })}
                                    </span>
                                ))}
                            </div>
                        )}

                        {session.discoveredSyncUrls && session.discoveredSyncUrls.length > 0 && (
                            <div
                                data-testid="social-discovered-sync-hint"
                                className="flex items-center gap-2 text-[11px] text-emerald-700 dark:text-emerald-300 bg-emerald-500/5 border border-emerald-500/20 rounded-xl px-3 py-2"
                            >
                                <Server size={13} className="shrink-0" />
                                <span>{t('backup.socialSyncAutoDetectedHint')}</span>
                            </div>
                        )}

                        {!session.hasEnoughShares && (
                            <div className="space-y-1.5 pt-1">
                                <div className="flex gap-2">
                                    <Input
                                        type="text"
                                        data-testid="social-reply-input"
                                        value={replyInput}
                                        onChange={(e) => setReplyInput(e.target.value)}
                                        placeholder={t('backup.pasteReplyLinkPlaceholder')}
                                        className="text-xs font-mono"
                                    />
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        data-testid="social-scan-reply-qr-btn"
                                        onClick={() => setIsReplyQrOpen(true)}
                                        title={t('common.scanQrCode')}
                                        className="shrink-0 text-xs gap-1.5"
                                    >
                                        <QrCode size={14} />
                                        <span className="hidden sm:inline">{t('common.scanQrCode')}</span>
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        data-testid="social-add-reply-btn"
                                        onClick={() => void handleImportReply()}
                                        disabled={!replyInput.trim() || isSubmitting}
                                        className="shrink-0 text-xs"
                                    >
                                        {t('backup.addReplyLink')}
                                    </Button>
                                </div>
                                <p className="text-[11px] text-theme-light">
                                    {t('backup.pasteReplyChatHint')}
                                </p>
                            </div>
                        )}
                    </div>

                    {/* Unreachable Sync Offer Secondary Empty Start */}
                    {unreachableSyncError && (
                        <AlertBanner
                            variant="warning"
                            data-testid="social-unreachable-warning"
                            action={
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="xs"
                                    data-testid="social-unreachable-empty-btn"
                                    disabled={isSubmitting}
                                    onClick={() =>
                                        void submitRecoveryCompletion({ allowEmptyFallback: true })
                                    }
                                >
                                    {t('backup.proceedWithoutBackupLink')}
                                </Button>
                            }
                        >
                            {t('backup.socialSyncUnreachableHint')}
                        </AlertBanner>
                    )}

                    {/* Calm Confirm when Sync Service had no backup */}
                    {syncEmptyResult && (
                        <div
                            data-testid="social-sync-empty-confirm"
                            className="p-4 bg-blue-50 border border-blue-200 rounded-2xl space-y-3"
                        >
                            <p className="text-xs font-semibold text-blue-900 leading-relaxed">
                                {t('backup.socialSyncEmptyNotice')}
                            </p>
                            <div className="flex justify-end gap-2">
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    onClick={() => setSyncEmptyResult(null)}
                                >
                                    {t('common.back')}
                                </Button>
                                <Button
                                    type="button"
                                    variant="primary"
                                    size="sm"
                                    data-testid="social-confirm-empty-btn"
                                    disabled={isSubmitting}
                                    onClick={() =>
                                        void submitRecoveryCompletion({ allowEmptyFallback: true })
                                    }
                                >
                                    {t('backup.openWithoutBackupAction')}
                                </Button>
                            </div>
                        </div>
                    )}

                    {/* Local Collision Choice Card */}
                    {collisionResult?.localCollision && (() => {
                        const hasOnlineBackup =
                            collisionResult.backupEpoch !== null &&
                            collisionResult.backupEpoch !== undefined;
                        return (
                            <div
                                data-testid="social-collision-card"
                                className="p-5 bg-amber-50 border border-amber-200 rounded-2xl space-y-4"
                            >
                                <div className="flex items-start gap-2.5">
                                    <AlertTriangle size={18} className="text-amber-600 shrink-0 mt-0.5" />
                                    <div className="space-y-1">
                                        <h4 className="text-sm font-black text-amber-900">
                                            {t('backup.collisionDetectedTitle', {
                                                name: collisionResult.localCollision.profileName,
                                            })}
                                        </h4>
                                        <p className="text-xs text-amber-800 leading-relaxed">
                                            {hasOnlineBackup
                                                ? t('backup.collisionComparisonDesc', {
                                                      name: collisionResult.localCollision.profileName,
                                                      localEpoch:
                                                          collisionResult.localCollision.localEpoch ?? 0,
                                                      localNonce:
                                                          collisionResult.localCollision.localTxNonce ?? 0,
                                                      backupEpoch: collisionResult.backupEpoch ?? 0,
                                                      backupNonce: collisionResult.backupTxNonce ?? 0,
                                                  })
                                                : t('backup.collisionLocalOnlyDesc', {
                                                      name: collisionResult.localCollision.profileName,
                                                  })}
                                        </p>
                                    </div>
                                </div>

                                <div className="flex flex-col sm:flex-row gap-2 justify-end">
                                    <Button
                                        type="button"
                                        variant="primary"
                                        size="sm"
                                        data-testid="social-collision-reset-pw-btn"
                                        disabled={isSubmitting}
                                        onClick={() =>
                                            void submitRecoveryCompletion({
                                                overrideMode: 'reset_local_password',
                                                overrideTargetFolder:
                                                    collisionResult.localCollision?.folderName,
                                            })
                                        }
                                    >
                                        <KeyRound size={14} className="mr-1.5" />
                                        {t('backup.collisionResetPasswordBtn', {
                                            name: collisionResult.localCollision.profileName,
                                        })}
                                    </Button>
                                    {hasOnlineBackup && (
                                        <Button
                                            type="button"
                                            variant="secondary"
                                            size="sm"
                                            data-testid="social-collision-replace-btn"
                                            disabled={isSubmitting}
                                            onClick={() =>
                                                void submitRecoveryCompletion({
                                                    replaceFolderName:
                                                        collisionResult.localCollision?.folderName,
                                                })
                                            }
                                        >
                                            {t('backup.collisionReplaceProfileBtn', {
                                                name: collisionResult.localCollision.profileName,
                                            })}
                                        </Button>
                                    )}
                                </div>
                            </div>
                        );
                    })()}

                    {/* Step 3 Form */}
                    {session.hasEnoughShares && session.questionIds && (
                        <form
                            onSubmit={handleFormSubmit}
                            className="space-y-4 pt-2 border-t border-theme-subtle/60"
                        >
                            <h4 className="text-xs font-bold uppercase tracking-wider text-theme-secondary">
                                {t('backup.socialStep3Title')}
                            </h4>

                            {session.questionIds.map((qId, idx) => (
                                <div key={idx} className="space-y-1.5">
                                    <label className="text-xs font-bold text-theme-secondary block">
                                        {idx + 1}. {t(QUESTION_CATALOG[qId] || 'backup.recoveryQuestion1')}
                                    </label>
                                    <Input
                                        type="text"
                                        data-testid={`social-answer-${idx}`}
                                        value={answers[idx]}
                                        onChange={(e) => {
                                            const next = [...answers] as [string, string, string];
                                            next[idx] = e.target.value;
                                            setAnswers(next);
                                        }}
                                        className="text-xs"
                                    />
                                </div>
                            ))}

                            {showPassphraseInput && (
                                <div className="space-y-1.5 p-3 bg-amber-500/10 border border-amber-500/20 rounded-xl">
                                    <label className="text-xs font-bold text-theme-secondary block">
                                        {t('backup.bip39PassphraseRequired')}
                                    </label>
                                    <Input
                                        type="password"
                                        showToggle
                                        data-testid="social-passphrase-input"
                                        value={bip39Passphrase}
                                        onChange={(e) => setBip39Passphrase(e.target.value)}
                                        className="text-xs"
                                    />
                                </div>
                            )}

                            {mode === 'restore_new_device' && (
                                <div className="space-y-1.5 pt-2">
                                    <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                        {t('backup.profileNameLabel')}
                                    </label>
                                    <Input
                                        type="text"
                                        data-testid="social-profile-name-input"
                                        value={profileName}
                                        onChange={(e) => setProfileName(e.target.value)}
                                        placeholder={t('profile.profileNamePlaceholder')}
                                        className="text-xs"
                                    />
                                    <p className="text-[11px] text-theme-light">
                                        {t('backup.localProfileNameHint')}
                                    </p>
                                </div>
                            )}

                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                                <div className="space-y-1.5">
                                    <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                        {t('backup.newPasswordLabel')}
                                    </label>
                                    <Input
                                        type="password"
                                        showToggle
                                        data-testid="social-new-password-input"
                                        value={newPassword}
                                        onChange={(e) => setNewPassword(e.target.value)}
                                        placeholder={t('auth.passwordMinChars')}
                                        className="text-xs"
                                    />
                                </div>
                                <div className="space-y-1.5">
                                    <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                        {t('backup.confirmNewPasswordLabel')}
                                    </label>
                                    <Input
                                        type="password"
                                        showToggle
                                        data-testid="social-confirm-password-input"
                                        value={confirmNewPassword}
                                        onChange={(e) => setConfirmNewPassword(e.target.value)}
                                        placeholder={t('auth.repeatPassword')}
                                        className="text-xs"
                                    />
                                </div>
                            </div>

                            <div className="flex justify-end pt-3">
                                <Button
                                    type="submit"
                                    variant="primary"
                                    data-testid="social-complete-submit-btn"
                                    disabled={
                                        !isPasswordValid ||
                                        !isProfileNameValid ||
                                        !answers[0] ||
                                        !answers[1] ||
                                        !answers[2] ||
                                        isSubmitting
                                    }
                                    className="flex items-center gap-2"
                                >
                                    <CheckCircle2 size={16} />
                                    {isSubmitting
                                        ? t('common.loading')
                                        : t('backup.restoreActionComplete')}
                                </Button>
                            </div>
                        </form>
                    )}
                </div>
            )}

            <QrScannerModal
                isOpen={isReplyQrOpen}
                onClose={() => setIsReplyQrOpen(false)}
                onScan={(decoded) => {
                    setReplyInput(decoded);
                    void handleImportReply(decoded);
                }}
            />
        </div>
    );
};
