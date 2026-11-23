import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import type { TFunction } from 'i18next';
import { RECOVERY_QUESTION_MAP } from '../../../constants/recoveryQuestions';
import { authService } from '../../../services/authService';
import { profileService } from '../../../services/profileService';
import { recoveryService } from '../../../services/recoveryService';
import type { ActiveRecoverySessionDto, RecoveryCompletionResultDto } from '../../../types';
import { copyToClipboard } from '../../../utils/clipboard';
import { isBackendError, stringifyError, translateError } from '../../../utils/errorHelper';

export const QUESTION_CATALOG = RECOVERY_QUESTION_MAP;

interface UseSocialRecoveryFlowParams {
    mode: 'restore_new_device' | 'reset_local_password';
    targetFolderName?: string;
    initialPayload?: string;
    existingSession?: ActiveRecoverySessionDto | null;
    onSuccess: (result: RecoveryCompletionResultDto) => void;
    t: TFunction;
}

export function useSocialRecoveryFlow({
    mode,
    targetFolderName,
    initialPayload,
    existingSession,
    onSuccess,
    t,
}: UseSocialRecoveryFlowParams) {
    const [session, setSession] = useState<ActiveRecoverySessionDto | null>(
        existingSession ?? null,
    );
    const [conflictingSession, setConflictingSession] =
        useState<ActiveRecoverySessionDto | null>(null);
    const [replyInput, setReplyInput] = useState<string>('');
    const [answers, setAnswers] = useState<[string, string, string]>(['', '', '']);
    const [bip39Passphrase, setBip39Passphrase] = useState<string>('');
    const [showPassphraseInput, setShowPassphraseInput] = useState<boolean>(false);

    const [profileName, setProfileName] = useState<string>(
        existingSession?.ownerDisplayName ?? '',
    );
    const [newPassword, setNewPassword] = useState<string>('');
    const [confirmNewPassword, setConfirmNewPassword] = useState<string>('');

    const [collisionResult, setCollisionResult] =
        useState<RecoveryCompletionResultDto | null>(null);
    const [syncEmptyResult, setSyncEmptyResult] =
        useState<RecoveryCompletionResultDto | null>(null);
    const [unreachableSyncError, setUnreachableSyncError] = useState<boolean>(false);

    const [isLoadingSession, setIsLoadingSession] = useState<boolean>(() => !existingSession);
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
    const profileNameRef = useRef<string>(profileName);
    profileNameRef.current = profileName;
    const mountedRef = useRef(true);
    const importSeqRef = useRef(0);
    const completeSeqRef = useRef(0);

    useEffect(() => {
        mountedRef.current = true;
        return () => {
            mountedRef.current = false;
        };
    }, []);

    useEffect(() => {
        if (existingSession) {
            setSession(existingSession);
            setIsLoadingSession(false);
            if (existingSession.ownerDisplayName && !profileNameRef.current) {
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
    }, [existingSession, mode]);

    const handleImportReply = useCallback(
        async (uriToImport?: string) => {
            const uri = (uriToImport ?? replyInput).trim();
            if (!uri || isSubmitting) return;

            setIsSubmitting(true);
            setErrorMsg('');
            setSuccessMsg('');
            setInfoMsg('');

            if (
                /humoco:\/\/recovery-req\b/i.test(uri) &&
                !/humoco:\/\/recovery-reply\b/i.test(uri)
            ) {
                setErrorMsg(
                    t(
                        'backup.replyWrongLinkReq',
                        'Das ist dein eigener Anfrage-Link (recovery-req), keine Antwort eines Vertrauenskontakts. Bitte füge den erhaltenen Antwort-Link (recovery-reply) ein.',
                    ),
                );
                setIsSubmitting(false);
                return;
            }
            if (/humoco:\/\/kit\b/i.test(uri) && !/humoco:\/\/recovery-reply\b/i.test(uri)) {
                setErrorMsg(
                    t(
                        'backup.replyWrongLinkKit',
                        'Das ist ein Notfall-Blatt-Link (humoco://kit). Nutze zur Wiederherstellung per Notfall-Blatt die Option „Mit 12/24 Wörtern oder Notfall-Blatt“.',
                    ),
                );
                setIsSubmitting(false);
                return;
            }

            const seq = ++importSeqRef.current;
            try {
                const updated = await recoveryService.importGuardianRecoveryReply(uri);
                if (!mountedRef.current || seq !== importSeqRef.current) return;
                setSession(updated);
                setReplyInput('');
                if (updated.ownerDisplayName) {
                    setProfileName((prev) => prev || updated.ownerDisplayName || '');
                }
                if (updated.lastImportStatus === 'superseded_epoch') {
                    setInfoMsg(
                        t(
                            'backup.importReplySupersededEpoch',
                            'Ein neuerer Vertrauenskontakt-Stand wurde erkannt: Ältere bisher gesammelte Anteile wurden durch diesen aktuellen Anteil ersetzt.',
                        ),
                    );
                } else if (updated.lastImportStatus === 'duplicate') {
                    setInfoMsg(t('backup.importReplyDuplicate'));
                } else {
                    setSuccessMsg(
                        t('backup.importReplySuccess', {
                            name: updated.ownerDisplayName || t('common.unknown'),
                        }),
                    );
                }
            } catch (err) {
                if (!mountedRef.current || seq !== importSeqRef.current) return;
                setErrorMsg(translateError(err, t));
            } finally {
                if (mountedRef.current && seq === importSeqRef.current) {
                    setIsSubmitting(false);
                }
            }
        },
        [replyInput, isSubmitting, t],
    );

    useEffect(() => {
        if (
            !initialPayload?.toLowerCase().includes('humoco://recovery-reply') ||
            isLoadingSession ||
            autoImportedRef.current === initialPayload
        ) {
            return;
        }
        if (!session) {
            setReplyInput((prev) => prev || initialPayload);
            setInfoMsg(
                t(
                    'backup.importReplyNeedsSessionFirst',
                    'Antwort-Link erkannt: Bitte starte zuerst unten einen Wiederherstellungs-Durchgang (oder setze den begonnenen fort), damit der Link zu deinem temporären Schlüssel passt.',
                ),
            );
            return;
        }
        if (session.hasEnoughShares) {
            autoImportedRef.current = initialPayload;
            setInfoMsg(
                t(
                    'backup.importReplyAlreadyEnoughShares',
                    'Du hast bereits genügend Anteile gesammelt. Du kannst den Wiederherstellungs-Vorgang jetzt abschließen.',
                ),
            );
            return;
        }
        autoImportedRef.current = initialPayload;
        void handleImportReply(initialPayload);
    }, [initialPayload, isLoadingSession, session, handleImportReply, t]);

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
                targetFolderName,
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

    const handleToggleRequestQr = async () => {
        if (showRequestQr) {
            setShowRequestQr(false);
            return;
        }
        if (!requestQrSvg && session?.requestUri) {
            try {
                const svg = await profileService.encodeTextQrSvg(session.requestUri);
                setRequestQrSvg(svg);
            } catch {
                return;
            }
        }
        setShowRequestQr(true);
    };

    const handleCopySasCode = () => {
        if (!session?.sasCode) return;
        void copyToClipboard(session.sasCode);
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

        const seq = ++completeSeqRef.current;
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
            if (!mountedRef.current || completeSeqRef.current !== seq) return;

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
            if (!mountedRef.current || completeSeqRef.current !== seq) return;
            const code = isBackendError(err) ? err.code : '';
            const msg = stringifyError(err);
            if (
                code === 'error.recovery.bip39PassphraseRequired' ||
                msg.includes('BIP39_PASSPHRASE_REQUIRED') ||
                msg.includes('Passphrase erforderlich')
            ) {
                setShowPassphraseInput(true);
                setErrorMsg(t('backup.bip39PassphraseRequired'));
            } else if (
                code === 'error.recovery.wrongAnswers' ||
                msg.includes('Antworten stimmen noch nicht') ||
                msg.includes('Decryption failed')
            ) {
                setErrorMsg(t('backup.wrongAnswersRetry'));
            } else if (
                code === 'error.recovery.syncUnreachable' ||
                msg.includes('Sync-Dienst nicht erreichbar')
            ) {
                setUnreachableSyncError(true);
                setErrorMsg(translateError(err, t));
            } else {
                setErrorMsg(translateError(err, t));
            }
        } finally {
            if (mountedRef.current && completeSeqRef.current === seq) {
                setIsSubmitting(false);
            }
        }
    };

    const handleFormSubmit = (e: FormEvent) => {
        e.preventDefault();
        setCollisionResult(null);
        setSyncEmptyResult(null);
        void submitRecoveryCompletion();
    };

    const isPasswordValid = newPassword.length >= 8 && newPassword === confirmNewPassword;
    const isProfileNameValid = mode === 'reset_local_password' || profileName.trim().length > 0;

    return {
        session,
        conflictingSession,
        replyInput,
        setReplyInput,
        answers,
        setAnswers,
        bip39Passphrase,
        setBip39Passphrase,
        showPassphraseInput,
        profileName,
        setProfileName,
        newPassword,
        setNewPassword,
        confirmNewPassword,
        setConfirmNewPassword,
        collisionResult,
        syncEmptyResult,
        setSyncEmptyResult,
        unreachableSyncError,
        isLoadingSession,
        isSubmitting,
        errorMsg,
        setErrorMsg,
        successMsg,
        setSuccessMsg,
        infoMsg,
        setInfoMsg,
        copiedRequest,
        copiedChatMsg,
        confirmCancel,
        setConfirmCancel,
        requestQrSvg,
        showRequestQr,
        isReplyQrOpen,
        setIsReplyQrOpen,
        isPasswordValid,
        isProfileNameValid,
        handleImportReply,
        handleStartSession,
        handleCancelSession,
        handleCopyRequestLink,
        handleCopyRequestChatMessage,
        handleToggleRequestQr,
        handleCopySasCode,
        submitRecoveryCompletion,
        handleFormSubmit,
    };
}
