import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    CheckCircle2,
    Clock,
    Copy,
    MessageSquareShare,
    QrCode,
    RefreshCw,
    Server,
    ShieldCheck,
    X,
} from 'lucide-react';
import type { ActiveRecoverySessionDto } from '../../../types';
import { AlertBanner } from '../../ui/AlertBanner';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { QrCodeDisplay } from '../../ui/QrCodeDisplay';

interface RecoveryRequestAndSharesStepProps {
    session: ActiveRecoverySessionDto;
    copiedChatMsg: boolean;
    copiedRequest: boolean;
    showRequestQr: boolean;
    requestQrSvg: string | null;
    confirmCancel: boolean;
    replyInput: string;
    isSubmitting: boolean;
    onBack?: () => void;
    onCopyChatMessage: () => void;
    onCopyRequestLink: () => void;
    onToggleRequestQr: () => void;
    onCopySasCode: () => void;
    onSetConfirmCancel: (confirm: boolean) => void;
    onCancelSession: () => void;
    onReplyInputChange: (value: string) => void;
    onOpenReplyQr: () => void;
    onImportReply: () => void;
}

export const RecoveryRequestAndSharesStep: React.FC<RecoveryRequestAndSharesStepProps> = ({
    session,
    copiedChatMsg,
    copiedRequest,
    showRequestQr,
    requestQrSvg,
    confirmCancel,
    replyInput,
    isSubmitting,
    onBack,
    onCopyChatMessage,
    onCopyRequestLink,
    onToggleRequestQr,
    onCopySasCode,
    onSetConfirmCancel,
    onCancelSession,
    onReplyInputChange,
    onOpenReplyQr,
    onImportReply,
}) => {
    const { t } = useTranslation();

    return (
        <>
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
                        onClick={onCopyChatMessage}
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
                        onClick={onCopyRequestLink}
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
                        onClick={onToggleRequestQr}
                        className="flex items-center gap-1.5 text-xs"
                    >
                        <QrCode size={14} />
                        {showRequestQr ? t('common.hideQrCode') : t('common.showQrCode')}
                    </Button>
                </div>
                {showRequestQr && requestQrSvg && (
                    <div className="pt-2 flex flex-col items-center gap-2">
                        <QrCodeDisplay
                            svg={requestQrSvg}
                            value={session.requestUri}
                            size={192}
                            ariaLabel={t('backup.socialStep1Title')}
                            qrTestId="social-request-qr"
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
                        <div className="flex items-center gap-2">
                            <span
                                data-testid="social-sas-code"
                                className="text-2xl font-black tracking-widest text-theme-primary font-mono"
                            >
                                {session.sasCode}
                            </span>
                            <button
                                type="button"
                                onClick={onCopySasCode}
                                className="p-1.5 rounded-lg border border-theme-subtle bg-white text-theme-light hover:text-theme-primary cursor-pointer"
                                title={t('common.copy', 'Kopieren')}
                            >
                                <Copy size={14} />
                            </button>
                        </div>
                    </div>
                    <p className="text-xs text-theme-light leading-relaxed max-w-md">
                        {t('backup.socialSasExplanation')}
                    </p>
                </div>

                {session.createdAt > 0 && Math.floor(Date.now() / 1000) - session.createdAt > 48 * 3600 && (
                    <AlertBanner variant="warning" data-testid="social-stale-48h-warning">
                        {t(
                            'backup.socialSessionStale48h',
                            'Diese Wiederherstellungs-Anfrage ist älter als 48 Stunden. Wenn ein Kontakt eine Warnung sieht oder du einen frischen Link senden möchtest, kannst du den Ablauf abbrechen und neu starten.',
                        )}
                    </AlertBanner>
                )}

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
                                onClick={() => onSetConfirmCancel(true)}
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
                                    onClick={() => onSetConfirmCancel(false)}
                                >
                                    {t('common.abort')}
                                </Button>
                                <Button
                                    type="button"
                                    variant="danger"
                                    size="xs"
                                    data-testid="confirm-cancel-session-btn"
                                    onClick={onCancelSession}
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
                                onChange={(e) => onReplyInputChange(e.target.value)}
                                placeholder={t('backup.pasteReplyLinkPlaceholder')}
                                className="text-xs font-mono"
                            />
                            <Button
                                type="button"
                                variant="outline"
                                size="sm"
                                data-testid="social-scan-reply-qr-btn"
                                onClick={onOpenReplyQr}
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
                                onClick={onImportReply}
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
        </>
    );
};
