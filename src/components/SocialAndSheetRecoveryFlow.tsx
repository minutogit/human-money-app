// src/components/SocialAndSheetRecoveryFlow.tsx
import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { AlertBanner } from './ui/AlertBanner';
import { recoveryService } from '../services/recoveryService';
import { authService } from '../services/authService';
import { ActiveRecoverySessionDto } from '../types';
import { copyToClipboard } from '../utils/clipboard';
import { translateError } from '../utils/errorHelper';
import {
    Users,
    FileText,
    Copy,
    CheckCircle2,
    RefreshCw,
    X,
    ArrowRight,
    ArrowLeft,
} from 'lucide-react';

export interface SocialAndSheetRecoveryFlowProps {
    mode: 'restore_new_device' | 'reset_local_password';
    targetFolderName?: string;
    initialPayload?: string;
    onSuccess: () => void;
    onCancel?: () => void;
}

const QUESTION_CATALOG: Record<number, string> = {
    1: 'backup.recoveryQuestion1',
    2: 'backup.recoveryQuestion2',
    3: 'backup.recoveryQuestion3',
    4: 'backup.recoveryQuestion4',
    5: 'backup.recoveryQuestion5',
    6: 'backup.recoveryQuestion6',
};

export const SocialAndSheetRecoveryFlow: React.FC<SocialAndSheetRecoveryFlowProps> = ({
    mode,
    targetFolderName,
    initialPayload,
    onSuccess,
    onCancel,
}) => {
    const { t } = useTranslation();

    // Active method: 'choose' | 'social' | 'sheet'
    const [activeMethod, setActiveMethod] = useState<'choose' | 'social' | 'sheet'>(() => {
        if (initialPayload?.startsWith('humoco://kit')) return 'sheet';
        if (initialPayload?.startsWith('humoco://recovery-reply')) return 'social';
        return 'choose';
    });

    // --- Social Recovery State ---
    const [session, setSession] = useState<ActiveRecoverySessionDto | null>(null);
    const [nameHint, setNameHint] = useState<string>('');
    const [replyInput, setReplyInput] = useState<string>('');
    const [answers, setAnswers] = useState<[string, string, string]>(['', '', '']);
    const [bip39Passphrase, setBip39Passphrase] = useState<string>('');
    const [showPassphraseInput, setShowPassphraseInput] = useState<boolean>(false);

    // --- Emergency Sheet State ---
    const [sheetInputMode, setSheetInputMode] = useState<'link' | 'words'>('link');
    const [kitUri, setKitUri] = useState<string>(
        initialPayload?.startsWith('humoco://kit') ? initialPayload : ''
    );
    const [manualWords, setManualWords] = useState<string>('');
    const [manualSyncUrl, setManualSyncUrl] = useState<string>('');

    // --- Credentials State ---
    const [profileName, setProfileName] = useState<string>('');
    const [newPassword, setNewPassword] = useState<string>('');
    const [confirmNewPassword, setConfirmNewPassword] = useState<string>('');

    // --- UI Status ---
    const [isLoadingSession, setIsLoadingSession] = useState<boolean>(false);
    const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
    const [errorMsg, setErrorMsg] = useState<string>('');
    const [successMsg, setSuccessMsg] = useState<string>('');
    const [copiedRequest, setCopiedRequest] = useState<boolean>(false);
    const autoImportedRef = useRef<string | null>(null);

    // Load active recovery session if any
    useEffect(() => {
        let active = true;

        const checkSession = async () => {
            setIsLoadingSession(true);
            try {
                const existing = await recoveryService.getActiveRecoverySession();
                if (!active) return;
                if (existing) {
                    setSession(existing);
                    if (existing.ownerDisplayName) {
                        setProfileName(existing.ownerDisplayName);
                    }
                    setActiveMethod((prev) => (prev === 'choose' ? 'social' : prev));
                }
            } catch {
                // Ignore session load error
            } finally {
                if (active) {
                    setIsLoadingSession(false);
                }
            }
        };

        checkSession();

        return () => {
            active = false;
        };
    }, []);

    const handleImportReply = useCallback(
        async (uriToImport?: string) => {
            const uri = uriToImport || replyInput;
            if (!uri.trim() || isSubmitting) return;

            setIsSubmitting(true);
            setErrorMsg('');
            setSuccessMsg('');

            try {
                const updated = await recoveryService.importGuardianRecoveryReply(uri.trim());
                setSession(updated);
                setReplyInput('');
                if (updated.ownerDisplayName) {
                    setProfileName((prev) => (!prev && updated.ownerDisplayName ? updated.ownerDisplayName : prev));
                }
                setSuccessMsg(
                    t('backup.importReplySuccess', {
                        name: updated.ownerDisplayName || t('common.unknown'),
                    })
                );
            } catch (err) {
                setErrorMsg(translateError(err, t));
            } finally {
                setIsSubmitting(false);
            }
        },
        [replyInput, isSubmitting, t]
    );

    // Auto-import initial payload if recovery-reply arrived
    useEffect(() => {
        if (
            initialPayload?.startsWith('humoco://recovery-reply') &&
            session &&
            !session.hasEnoughShares &&
            autoImportedRef.current !== initialPayload
        ) {
            autoImportedRef.current = initialPayload;
            handleImportReply(initialPayload);
        }
    }, [initialPayload, session, handleImportReply]);

    // --- Social Recovery Actions ---
    const handleStartSession = async () => {
        setIsSubmitting(true);
        setErrorMsg('');
        try {
            const newSession = await recoveryService.startSocialRecoverySession(nameHint || undefined);
            setSession(newSession);
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSubmitting(false);
        }
    };

    const handleCancelSession = async () => {
        setIsSubmitting(true);
        setErrorMsg('');
        try {
            await recoveryService.cancelSocialRecoverySession();
            setSession(null);
            setAnswers(['', '', '']);
            setReplyInput('');
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

    const handleCompleteSocialRecovery = async (e: React.FormEvent) => {
        e.preventDefault();
        if (
            !session?.ownerDid ||
            session.rotationEpoch == null ||
            newPassword.length < 8 ||
            newPassword !== confirmNewPassword ||
            isSubmitting
        ) {
            return;
        }

        setIsSubmitting(true);
        setErrorMsg('');

        try {
            const localInstanceId = await authService.getLocalInstanceId();
            await recoveryService.completeSocialRecovery({
                ownerDid: session.ownerDid,
                rotationEpoch: session.rotationEpoch,
                answers: [answers[0].trim(), answers[1].trim(), answers[2].trim()],
                bip39Passphrase: bip39Passphrase.trim() || undefined,
                mode,
                newPassword,
                profileName: mode === 'restore_new_device' ? profileName.trim() || undefined : undefined,
                targetFolderName,
                localInstanceId,
            });
            onSuccess();
        } catch (err) {
            const msg = String(err);
            if (msg.includes('BIP39_PASSPHRASE_REQUIRED') || msg.includes('Passphrase erforderlich')) {
                setShowPassphraseInput(true);
                setErrorMsg(t('backup.bip39PassphraseRequired'));
            } else if (msg.includes('Antworten stimmen noch nicht') || msg.includes('Decryption failed')) {
                setErrorMsg(t('backup.wrongAnswersRetry'));
            } else {
                setErrorMsg(translateError(err, t));
            }
        } finally {
            setIsSubmitting(false);
        }
    };

    // --- Emergency Sheet Actions ---
    const handleRestoreEmergencyKit = async (e: React.FormEvent) => {
        e.preventDefault();
        if (newPassword.length < 8 || newPassword !== confirmNewPassword || isSubmitting) {
            return;
        }

        setIsSubmitting(true);
        setErrorMsg('');

        try {
            const localInstanceId = await authService.getLocalInstanceId();
            const kitInput =
                sheetInputMode === 'link'
                    ? { kitUri: kitUri.trim() }
                    : {
                          manualMnemonic: manualWords.trim(),
                          manualSyncUrls: manualSyncUrl.trim() ? [manualSyncUrl.trim()] : [],
                      };

            await recoveryService.restoreFromEmergencyKitUri({
                kitUriOrManual: kitInput,
                mode,
                newPassword,
                profileName: mode === 'restore_new_device' ? profileName.trim() || undefined : undefined,
                targetFolderName,
                localInstanceId,
            });
            onSuccess();
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSubmitting(false);
        }
    };

    const isPasswordValid = newPassword.length >= 8 && newPassword === confirmNewPassword;
    const isProfileNameValid = mode === 'reset_local_password' || profileName.trim().length > 0;

    return (
        <div className="space-y-6">
            {errorMsg && (
                <AlertBanner
                    variant="error"
                    onDismiss={() => setErrorMsg('')}
                >
                    {errorMsg}
                </AlertBanner>
            )}
            {successMsg && (
                <AlertBanner
                    variant="success"
                    onDismiss={() => setSuccessMsg('')}
                >
                    {successMsg}
                </AlertBanner>
            )}

            {/* Selection Screen (2 Big Primary Action Tiles) */}
            {activeMethod === 'choose' && (
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 animate-in fade-in duration-300">
                    <button
                        type="button"
                        onClick={() => setActiveMethod('social')}
                        data-testid="choose-social-recovery-btn"
                        className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between"
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

                    <button
                        type="button"
                        onClick={() => setActiveMethod('sheet')}
                        data-testid="choose-sheet-recovery-btn"
                        className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between"
                    >
                        <div className="space-y-3">
                            <div className="w-12 h-12 rounded-2xl bg-amber-500/10 text-amber-600 flex items-center justify-center group-hover:scale-105 transition-transform">
                                <FileText size={24} />
                            </div>
                            <h3 className="font-black text-base text-theme-secondary tracking-tight">
                                {t('backup.restorePrimarySheet')}
                            </h3>
                            <p className="text-xs text-theme-light leading-relaxed">
                                {t('backup.restorePrimarySheetDesc')}
                            </p>
                        </div>
                        <div className="flex items-center gap-1.5 text-xs font-bold text-amber-600 pt-4">
                            <span>{t('common.select', 'Auswählen')}</span>
                            <ArrowRight size={14} />
                        </div>
                    </button>
                </div>
            )}

            {activeMethod === 'choose' && onCancel && (
                <div className="flex justify-center pt-2">
                    <Button
                        type="button"
                        variant="outline"
                        onClick={onCancel}
                        className="text-xs"
                    >
                        {t('common.cancel')}
                    </Button>
                </div>
            )}

            {/* Branch A: Social Recovery Flow */}
            {activeMethod === 'social' && (
                <div className="space-y-6 animate-in fade-in duration-300">
                    <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/60">
                        <div className="flex items-center gap-2">
                            <Users className="text-theme-primary" size={20} />
                            <h3 className="font-black text-sm uppercase tracking-wider text-theme-secondary">
                                {t('backup.restorePrimaryContacts')}
                            </h3>
                        </div>
                        <button
                            type="button"
                            onClick={() => setActiveMethod('choose')}
                            className="text-xs font-bold text-theme-light hover:text-theme-secondary flex items-center gap-1"
                        >
                            <ArrowLeft size={14} />
                            <span>{t('common.back')}</span>
                        </button>
                    </div>

                    {/* Step 1: Start Session */}
                    {!session ? (
                        <div className="space-y-4">
                            <p className="text-xs text-theme-light leading-relaxed">
                                {t('backup.socialRecoveryDesc')}
                            </p>
                            {mode === 'restore_new_device' && (
                                <div className="space-y-1.5">
                                    <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                        {t('backup.profileNameLabel')} (optional)
                                    </label>
                                    <Input
                                        type="text"
                                        value={nameHint}
                                        onChange={(e) => setNameHint(e.target.value)}
                                        placeholder="z. B. Alice"
                                        className="text-xs"
                                    />
                                </div>
                            )}
                            <Button
                                type="button"
                                variant="primary"
                                onClick={handleStartSession}
                                disabled={isSubmitting || isLoadingSession}
                                className="flex items-center gap-2"
                            >
                                <Users size={16} />
                                {isSubmitting ? t('common.loading') : t('backup.startRecoverySession')}
                            </Button>
                        </div>
                    ) : (
                        /* Step 2 & 3: Active Session UI */
                        <div className="space-y-6">
                            {/* SAS Code Display */}
                            <div className="p-4 bg-theme-primary/10 border border-theme-primary/20 rounded-2xl flex flex-col sm:flex-row items-center justify-between gap-4">
                                <div className="text-center sm:text-left">
                                    <span className="text-[10px] font-bold uppercase tracking-wider text-theme-light block">
                                        {t('backup.releaseSasCodeLabel')}
                                    </span>
                                    <span className="text-2xl font-black tracking-widest text-theme-primary font-mono">
                                        {session.sasCode}
                                    </span>
                                </div>
                                <div className="flex gap-2">
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        onClick={handleCopyRequestLink}
                                        className="flex items-center gap-1.5 text-xs"
                                    >
                                        <Copy size={14} />
                                        {copiedRequest ? t('backup.requestLinkCopied') : t('backup.copyRequestLink')}
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={handleCancelSession}
                                        className="text-xs text-rose-500 hover:text-rose-600"
                                    >
                                        <X size={14} />
                                        {t('backup.cancelSessionButton')}
                                    </Button>
                                </div>
                            </div>

                            {/* Approval Count & Collector */}
                            <div className="p-4 bg-bg-surface border border-theme-subtle rounded-2xl space-y-3">
                                <div className="flex items-center justify-between">
                                    <span className="text-xs font-bold text-theme-secondary">
                                        {t('backup.socialSharesProgress', {
                                            collected: session.collectedCount,
                                            required: session.thresholdM ?? '?',
                                        })}
                                    </span>
                                    {session.hasEnoughShares ? (
                                        <span className="px-2.5 py-1 rounded-full bg-emerald-500/10 text-emerald-600 font-bold text-xs flex items-center gap-1">
                                            <CheckCircle2 size={14} />
                                            Bereit
                                        </span>
                                    ) : (
                                        <span className="px-2.5 py-1 rounded-full bg-amber-500/10 text-amber-600 font-bold text-xs flex items-center gap-1">
                                            <RefreshCw size={14} className="animate-spin" />
                                            Ausstehend
                                        </span>
                                    )}
                                </div>

                                {!session.hasEnoughShares && (
                                    <div className="flex gap-2 pt-1">
                                        <Input
                                            type="text"
                                            value={replyInput}
                                            onChange={(e) => setReplyInput(e.target.value)}
                                            placeholder={t('backup.pasteReplyLinkPlaceholder')}
                                            className="text-xs font-mono"
                                        />
                                        <Button
                                            type="button"
                                            variant="secondary"
                                            onClick={() => handleImportReply()}
                                            disabled={!replyInput.trim() || isSubmitting}
                                            className="shrink-0 text-xs"
                                        >
                                            {t('backup.addReplyLink')}
                                        </Button>
                                    </div>
                                )}
                            </div>

                            {/* Form when enough shares are collected */}
                            {session.hasEnoughShares && session.questionIds && (
                                <form
                                    onSubmit={handleCompleteSocialRecovery}
                                    className="space-y-4 pt-2 border-t border-theme-subtle/60"
                                >
                                    <h4 className="text-xs font-bold uppercase tracking-wider text-theme-secondary">
                                        {t('backup.socialStep3Title')}
                                    </h4>

                                    {/* 3 Personal Questions */}
                                    {session.questionIds.map((qId, idx) => (
                                        <div key={idx} className="space-y-1.5">
                                            <label className="text-xs font-bold text-theme-secondary block">
                                                {idx + 1}. {t(QUESTION_CATALOG[qId] || 'backup.recoveryQuestion1')}
                                            </label>
                                            <Input
                                                type="text"
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

                                    {/* Passphrase prompt if required */}
                                    {showPassphraseInput && (
                                        <div className="space-y-1.5 p-3 bg-amber-500/10 border border-amber-500/20 rounded-xl">
                                            <label className="text-xs font-bold text-theme-secondary block">
                                                {t('backup.bip39PassphraseRequired')}
                                            </label>
                                            <Input
                                                type="password"
                                                value={bip39Passphrase}
                                                onChange={(e) => setBip39Passphrase(e.target.value)}
                                                className="text-xs"
                                            />
                                        </div>
                                    )}

                                    {/* Profile Name & New Password */}
                                    {mode === 'restore_new_device' && (
                                        <div className="space-y-1.5 pt-2">
                                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                                {t('backup.profileNameLabel')}
                                            </label>
                                            <Input
                                                type="text"
                                                value={profileName}
                                                onChange={(e) => setProfileName(e.target.value)}
                                                placeholder="Mein Profil"
                                                className="text-xs"
                                            />
                                        </div>
                                    )}

                                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                                        <div className="space-y-1.5">
                                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                                {t('backup.newPasswordLabel')}
                                            </label>
                                            <Input
                                                type="password"
                                                value={newPassword}
                                                onChange={(e) => setNewPassword(e.target.value)}
                                                placeholder="Mind. 8 Zeichen"
                                                className="text-xs"
                                            />
                                        </div>
                                        <div className="space-y-1.5">
                                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                                {t('backup.confirmNewPasswordLabel')}
                                            </label>
                                            <Input
                                                type="password"
                                                value={confirmNewPassword}
                                                onChange={(e) => setConfirmNewPassword(e.target.value)}
                                                placeholder="Wiederholen"
                                                className="text-xs"
                                            />
                                        </div>
                                    </div>

                                    <div className="flex justify-end pt-3">
                                        <Button
                                            type="submit"
                                            variant="primary"
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
                </div>
            )}

            {/* Branch B: Emergency Sheet Flow */}
            {activeMethod === 'sheet' && (
                <form onSubmit={handleRestoreEmergencyKit} className="space-y-5 animate-in fade-in duration-300">
                    <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/60">
                        <div className="flex items-center gap-2">
                            <FileText className="text-amber-600" size={20} />
                            <h3 className="font-black text-sm uppercase tracking-wider text-theme-secondary">
                                {t('backup.restorePrimarySheet')}
                            </h3>
                        </div>
                        <button
                            type="button"
                            onClick={() => setActiveMethod('choose')}
                            className="text-xs font-bold text-theme-light hover:text-theme-secondary flex items-center gap-1"
                        >
                            <ArrowLeft size={14} />
                            <span>{t('common.back')}</span>
                        </button>
                    </div>

                    <div className="flex gap-2">
                        <Button
                            type="button"
                            variant={sheetInputMode === 'link' ? 'primary' : 'secondary'}
                            size="sm"
                            onClick={() => setSheetInputMode('link')}
                            className="flex-1 text-xs"
                        >
                            {t('backup.restoreSheetKitUriLabel')}
                        </Button>
                        <Button
                            type="button"
                            variant={sheetInputMode === 'words' ? 'primary' : 'secondary'}
                            size="sm"
                            onClick={() => setSheetInputMode('words')}
                            className="flex-1 text-xs"
                        >
                            {t('backup.restoreSheetManualWordsLabel')}
                        </Button>
                    </div>

                    {sheetInputMode === 'link' ? (
                        <div className="space-y-1.5">
                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                {t('backup.restoreSheetKitUriLabel')}
                            </label>
                            <Input
                                type="text"
                                value={kitUri}
                                onChange={(e) => setKitUri(e.target.value)}
                                placeholder="humoco://kit?v=1&ent=..."
                                data-testid="emergency-sheet-uri-input"
                                className="text-xs font-mono"
                            />
                        </div>
                    ) : (
                        <div className="space-y-3">
                            <div className="space-y-1.5">
                                <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                    {t('backup.restoreSheetManualWordsLabel')}
                                </label>
                                <textarea
                                    rows={3}
                                    value={manualWords}
                                    onChange={(e) => setManualWords(e.target.value)}
                                    placeholder="12 Wörter getrennt durch Leerzeichen..."
                                    className="w-full text-xs font-mono p-3 bg-bg-surface border border-theme-subtle rounded-xl text-theme-secondary focus:ring-1 focus:ring-theme-primary"
                                />
                            </div>
                            <div className="space-y-1.5">
                                <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                    {t('backup.restoreSheetSyncUrlLabel')}
                                </label>
                                <Input
                                    type="text"
                                    value={manualSyncUrl}
                                    onChange={(e) => setManualSyncUrl(e.target.value)}
                                    placeholder="https://sync.humoco.org (optional)"
                                    className="text-xs font-mono"
                                />
                            </div>
                        </div>
                    )}

                    {/* Profile Name & Passwords */}
                    {mode === 'restore_new_device' && (
                        <div className="space-y-1.5 pt-2">
                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                {t('backup.profileNameLabel')}
                            </label>
                            <Input
                                type="text"
                                value={profileName}
                                onChange={(e) => setProfileName(e.target.value)}
                                placeholder="Mein Profil"
                                data-testid="sheet-profile-name-input"
                                className="text-xs"
                            />
                        </div>
                    )}

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-2">
                        <div className="space-y-1.5">
                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                {t('backup.newPasswordLabel')}
                            </label>
                            <Input
                                type="password"
                                value={newPassword}
                                onChange={(e) => setNewPassword(e.target.value)}
                                placeholder="Mind. 8 Zeichen"
                                data-testid="sheet-new-password-input"
                                className="text-xs"
                            />
                        </div>
                        <div className="space-y-1.5">
                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                {t('backup.confirmNewPasswordLabel')}
                            </label>
                            <Input
                                type="password"
                                value={confirmNewPassword}
                                onChange={(e) => setConfirmNewPassword(e.target.value)}
                                placeholder="Wiederholen"
                                data-testid="sheet-confirm-password-input"
                                className="text-xs"
                            />
                        </div>
                    </div>

                    <div className="flex justify-end pt-3">
                        <Button
                            type="submit"
                            variant="primary"
                            disabled={
                                !isPasswordValid ||
                                !isProfileNameValid ||
                                (sheetInputMode === 'link' ? !kitUri.trim() : !manualWords.trim()) ||
                                isSubmitting
                            }
                            data-testid="sheet-restore-submit-btn"
                            className="flex items-center gap-2"
                        >
                            <CheckCircle2 size={16} />
                            {isSubmitting ? t('common.loading') : t('backup.restoreActionComplete')}
                        </Button>
                    </div>
                </form>
            )}
        </div>
    );
};
