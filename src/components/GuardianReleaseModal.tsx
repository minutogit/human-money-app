// src/components/GuardianReleaseModal.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { ModalShell } from './ui/ModalShell';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { AlertBanner } from './ui/AlertBanner';
import { recoveryService } from '../services/recoveryService';
import { postService } from '../services/postService';
import { GuardianRecoveryRequestInspectionDto, ApproveGuardianRecoveryResultDto } from '../types';
import { copyToClipboard } from '../utils/clipboard';
import { translateError } from '../utils/errorHelper';
import {
    CheckCircle2,
    Copy,
    AlertTriangle,
    KeyRound,
    UserCheck,
    Clock,
    RefreshCw,
} from 'lucide-react';

interface GuardianReleaseModalProps {
    isOpen: boolean;
    onClose: () => void;
    initialRequestUri?: string;
    initialOwnerDid?: string;
}

export const GuardianReleaseModal: React.FC<GuardianReleaseModalProps> = ({
    isOpen,
    onClose,
    initialRequestUri,
    initialOwnerDid,
}) => {
    const { t } = useTranslation();
    const [requestUri, setRequestUri] = useState<string>(initialRequestUri || '');
    const [password, setPassword] = useState<string>('');
    const [selectedOwnerDid, setSelectedOwnerDid] = useState<string>(initialOwnerDid || '');
    const [inspection, setInspection] = useState<GuardianRecoveryRequestInspectionDto | null>(null);
    const [approvalResult, setApprovalResult] = useState<ApproveGuardianRecoveryResultDto | null>(null);

    const [isInspecting, setIsInspecting] = useState<boolean>(false);
    const [isSyncingInbox, setIsSyncingInbox] = useState<boolean>(false);
    const [isApproving, setIsApproving] = useState<boolean>(false);
    const [errorMsg, setErrorMsg] = useState<string>('');
    const [copied, setCopied] = useState<boolean>(false);
    const [copiedReplyChat, setCopiedReplyChat] = useState<boolean>(false);

    const inspectUri = useCallback(async (uriToInspect: string) => {
        if (!uriToInspect.trim()) return;
        setIsInspecting(true);
        setErrorMsg('');

        try {
            const dto = await recoveryService.inspectGuardianRecoveryRequest(uriToInspect);
            setInspection(dto);
            const wards = dto.wards || [];
            if (wards.length === 1) {
                setSelectedOwnerDid(wards[0].ownerDid);
            } else if (wards.length > 1) {
                if (initialOwnerDid && wards.some((w) => w.ownerDid === initialOwnerDid)) {
                    setSelectedOwnerDid(initialOwnerDid);
                } else if (dto.nameHint && dto.nameHint.trim().length >= 2) {
                    const hint = dto.nameHint.trim().toLowerCase();
                    const matches = wards.filter((w) =>
                        w.ownerDisplayName.toLowerCase().includes(hint)
                    );
                    setSelectedOwnerDid(matches.length === 1 ? matches[0].ownerDid : '');
                } else {
                    setSelectedOwnerDid('');
                }
            } else {
                setSelectedOwnerDid('');
            }
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsInspecting(false);
        }
    }, [initialOwnerDid, t]);

    useEffect(() => {
        if (!isOpen) {
            setRequestUri('');
            setPassword('');
            setSelectedOwnerDid('');
            setInspection(null);
            setApprovalResult(null);
            setErrorMsg('');
            setCopied(false);
            setCopiedReplyChat(false);
            return;
        }

        const uri = initialRequestUri || '';
        setRequestUri(uri);
        setPassword('');
        setSelectedOwnerDid(initialOwnerDid || '');
        setInspection(null);
        setApprovalResult(null);
        setErrorMsg('');
        setCopied(false);
        setCopiedReplyChat(false);

        if (uri.includes('recovery-req?')) {
            inspectUri(uri);
        }
    }, [isOpen, initialRequestUri, initialOwnerDid, inspectUri]);

    const handleApprove = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!requestUri || !selectedOwnerDid || !password || isApproving) return;

        setIsApproving(true);
        setErrorMsg('');

        try {
            const res = await recoveryService.approveGuardianRecoveryRequest(
                requestUri,
                selectedOwnerDid,
                password
            );
            setApprovalResult(res);
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsApproving(false);
        }
    };

    const handleCopy = async () => {
        if (!approvalResult?.replyUri) return;
        await copyToClipboard(approvalResult.replyUri);
        setCopied(true);
        setTimeout(() => setCopied(false), 3000);
    };

    const handleCopyReplyChatMessage = async () => {
        if (!approvalResult?.replyUri) return;
        const msg = t('backup.replyChatTemplate', {
            replyUri: approvalResult.replyUri,
        });
        await copyToClipboard(msg);
        setCopiedReplyChat(true);
        setTimeout(() => setCopiedReplyChat(false), 3000);
    };

    const handleSyncInboxForWards = async () => {
        setIsSyncingInbox(true);
        setErrorMsg('');
        try {
            await postService.syncPostChannels();
            if (requestUri.trim()) {
                await inspectUri(requestUri);
            }
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSyncingInbox(false);
        }
    };

    if (!isOpen) return null;

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            title={t('backup.releaseModalTitle')}
        >
            <div className="space-y-6">
                {errorMsg && (
                    <AlertBanner
                        variant="error"
                        onDismiss={() => setErrorMsg('')}
                    >
                        {errorMsg}
                    </AlertBanner>
                )}

                {/* State A: Result Screen */}
                {approvalResult ? (
                    <div className="space-y-5 animate-in fade-in duration-300">
                        <div className="p-4 bg-emerald-500/10 border border-emerald-500/20 rounded-2xl flex items-center gap-3">
                            <CheckCircle2 size={24} className="text-emerald-500 shrink-0" />
                            <div>
                                <h4 className="text-xs font-bold text-emerald-800 dark:text-emerald-300">
                                    {t('backup.releaseSuccessTitle')}
                                </h4>
                                <p className="text-xs text-theme-secondary mt-0.5 font-medium">
                                    {t('backup.releaseSuccessDesc')}
                                </p>
                            </div>
                        </div>

                        {approvalResult.replyQrSvg && (
                            <div className="flex justify-center p-3 bg-white rounded-2xl border border-theme-subtle shadow-sm">
                                <div
                                    className="w-48 h-48"
                                    dangerouslySetInnerHTML={{ __html: approvalResult.replyQrSvg }}
                                />
                            </div>
                        )}

                        <div className="space-y-2">
                            <Input
                                readOnly
                                value={approvalResult.replyUri}
                                className="text-xs font-mono select-all"
                            />
                            <div className="flex flex-col sm:flex-row gap-2">
                                <Button
                                    type="button"
                                    variant="primary"
                                    data-testid="copy-reply-chat-msg-btn"
                                    onClick={handleCopyReplyChatMessage}
                                    className="flex-1 flex items-center justify-center gap-2"
                                >
                                    <Copy size={16} />
                                    {copiedReplyChat
                                        ? t('backup.replyChatMessageCopied')
                                        : t('backup.copyReplyChatMessage')}
                                </Button>
                                <Button
                                    type="button"
                                    variant="secondary"
                                    onClick={handleCopy}
                                    className="flex-1 flex items-center justify-center gap-2"
                                >
                                    <Copy size={16} />
                                    {copied ? t('backup.replyLinkCopied') : t('backup.copyReplyLink')}
                                </Button>
                            </div>
                        </div>

                        <div className="flex justify-end pt-2">
                            <Button type="button" variant="outline" onClick={onClose}>
                                {t('common.close')}
                            </Button>
                        </div>
                    </div>
                ) : (
                    /* State B: Input & Confirmation Form */
                    <form onSubmit={handleApprove} className="space-y-5">
                        {/* Request URI input if not already loaded */}
                        {!inspection && (
                            <div className="space-y-2">
                                <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                    {t('backup.pasteRequestLinkLabel')}
                                </label>
                                <div className="flex gap-2">
                                    <Input
                                        type="text"
                                        data-testid="guardian-request-uri-input"
                                        value={requestUri}
                                        onChange={(e) => setRequestUri(e.target.value)}
                                        placeholder="humoco://recovery-req?v=1&..."
                                        className="text-xs font-mono"
                                    />
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        data-testid="guardian-inspect-request-btn"
                                        disabled={!requestUri.trim() || isInspecting}
                                        onClick={() => inspectUri(requestUri)}
                                    >
                                        {isInspecting ? t('common.loading') : t('settings.syncProbeNow')}
                                    </Button>
                                </div>
                                <p className="text-[11px] text-theme-light">
                                    {t('backup.pasteRequestChatHint')}
                                </p>
                            </div>
                        )}

                        {inspection && (
                            <div className="space-y-4">
                                {/* SAS Check Code display */}
                                <div className="p-4 bg-theme-primary/10 border border-theme-primary/20 rounded-2xl text-center space-y-1">
                                    <span className="text-xs font-bold text-theme-light uppercase tracking-wider block">
                                        {t('backup.releaseSasCodeLabel')}
                                    </span>
                                    <p className="text-3xl font-black tracking-widest text-theme-primary font-mono">
                                        {inspection.sasCode}
                                    </p>
                                </div>

                                {/* Warning */}
                                <div className="p-3 bg-rose-500/10 border border-rose-500/20 rounded-xl flex items-start gap-2.5">
                                    <AlertTriangle className="text-rose-500 shrink-0 mt-0.5" size={16} />
                                    <p className="text-xs text-theme-secondary font-medium">
                                        {t('backup.releaseSasWarning')}
                                    </p>
                                </div>

                                {/* Stale 48h warning (only if actually > 48h old) */}
                                {inspection.isStale48h && (
                                    <div className="p-3 bg-amber-500/10 border border-amber-500/20 rounded-xl flex items-start gap-2.5">
                                        <Clock className="text-amber-500 shrink-0 mt-0.5" size={16} />
                                        <p className="text-xs text-theme-secondary font-medium">
                                            {t('backup.releaseStale48hWarning')}
                                        </p>
                                    </div>
                                )}

                                {/* Empty vault fallback */}
                                {(!inspection.wards || inspection.wards.length === 0) && (
                                    <AlertBanner
                                        variant="warning"
                                        data-testid="guardian-no-wards-banner"
                                        action={
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="xs"
                                                data-testid="sync-inbox-for-ward-btn"
                                                disabled={isSyncingInbox || isInspecting}
                                                onClick={() => void handleSyncInboxForWards()}
                                            >
                                                <RefreshCw
                                                    size={12}
                                                    className={`mr-1 ${isSyncingInbox ? 'animate-spin' : ''}`}
                                                />
                                                {t('backup.syncInboxNowButton')}
                                            </Button>
                                        }
                                    >
                                        {t('backup.noWardsInVaultHint')}
                                    </AlertBanner>
                                )}

                                {/* Ward Selector if multiple */}
                                {inspection.wards && inspection.wards.length > 1 && (
                                    <div className="space-y-1.5">
                                        <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                            {t('backup.selectWardLabel')}
                                        </label>
                                        <select
                                            data-testid="guardian-ward-select"
                                            value={selectedOwnerDid}
                                            onChange={(e) => setSelectedOwnerDid(e.target.value)}
                                            className="w-full text-xs font-bold bg-bg-surface border border-theme-subtle rounded-lg p-2.5 text-theme-secondary"
                                        >
                                            <option value="">
                                                {t('backup.selectWardPlaceholder')}
                                            </option>
                                            {inspection.wards.map((ward) => (
                                                <option key={ward.ownerDid} value={ward.ownerDid}>
                                                    {ward.ownerDisplayName} ({ward.thresholdM}/{ward.totalN})
                                                </option>
                                            ))}
                                        </select>
                                    </div>
                                )}

                                {inspection.wards && inspection.wards.length === 1 && (
                                    <div className="p-3 bg-bg-app rounded-xl border border-theme-subtle flex items-center justify-between text-xs">
                                        <span className="font-bold text-theme-secondary">
                                            {inspection.wards[0].ownerDisplayName}
                                        </span>
                                        <span className="text-theme-light">
                                            {t('backup.wardThresholdInfo', {
                                                threshold: inspection.wards[0].thresholdM,
                                                total: inspection.wards[0].totalN,
                                            })}
                                        </span>
                                    </div>
                                )}

                                {/* Password Input */}
                                <div className="space-y-1.5">
                                    <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                                        <KeyRound size={15} className="text-theme-primary" />
                                        {t('auth.password')}
                                    </label>
                                    <Input
                                        type="password"
                                        showToggle
                                        value={password}
                                        onChange={(e) => setPassword(e.target.value)}
                                        placeholder={t('auth.passwordPlaceholder')}
                                        className="text-xs"
                                    />
                                </div>

                                <div className="flex justify-end gap-3 pt-3">
                                    <Button
                                        type="button"
                                        variant="outline"
                                        onClick={onClose}
                                        disabled={isApproving}
                                    >
                                        {t('common.cancel')}
                                    </Button>
                                    <Button
                                        type="submit"
                                        variant="primary"
                                        data-testid="guardian-release-approve-btn"
                                        disabled={!password || !selectedOwnerDid || isApproving}
                                        className="flex items-center gap-2"
                                    >
                                        <UserCheck size={16} />
                                        {isApproving
                                            ? t('common.loading')
                                            : t('backup.releaseApproveButton')}
                                    </Button>
                                </div>
                            </div>
                        )}
                    </form>
                )}
            </div>
        </ModalShell>
    );
};
