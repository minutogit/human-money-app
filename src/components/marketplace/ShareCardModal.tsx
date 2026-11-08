// src/components/marketplace/ShareCardModal.tsx
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { QrCode, Download, Copy, Check, AlertCircle, Share2 } from 'lucide-react';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { copyToClipboard } from '../../utils/clipboard';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { logger } from '../../utils/log';
import { stringifyError } from '../../utils/errorHelper';
import { truncateUserId } from '../../utils/userIdHelper';

export interface ShareCardModalProps {
    isOpen: boolean;
    onClose: () => void;
    title?: string;
    subtitle?: string;
    envelopeJson: string;
    shareUri: string;
    qrSvg?: string | null;
    did?: string;
}

export function ShareCardModal({
    isOpen,
    onClose,
    title,
    subtitle,
    envelopeJson,
    shareUri,
    qrSvg,
    did,
}: ShareCardModalProps) {
    const { t } = useTranslation();
    const [copied, setCopied] = useState(false);
    const [isSaving, setIsSaving] = useState(false);
    const [saveStatus, setSaveStatus] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    const handleCopy = async () => {
        const ok = await copyToClipboard(shareUri);
        if (ok) {
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        }
    };

    const handleSaveFile = async () => {
        setIsSaving(true);
        setSaveStatus(null);
        try {
            const shortDid = did ? truncateUserId(did).replace(/[^a-zA-Z0-9_-]/g, '_') : 'contact';
            const defaultFilename = `humoco-${shortDid}.hmcard`;

            let filePath: string | null = null;
            try {
                filePath = await save({
                    defaultPath: defaultFilename,
                    filters: [
                        { name: t('marketplace.cardFilterName'), extensions: ['hmcard'] },
                    ],
                });
            } catch (err) {
                logger.info(`Tauri save dialog not available, using browser download: ${stringifyError(err)}`);
            }

            if (filePath) {
                const bytes = new TextEncoder().encode(envelopeJson);
                await writeFile(filePath, bytes);
                setSaveStatus({ type: 'success', message: t('marketplace.cardFileSaved') });
            } else if (filePath === null && typeof window !== 'undefined' && window.document) {
                // Fallback to browser download if not cancelled by user in a pure browser context
                const blob = new Blob([envelopeJson], { type: 'application/json' });
                const url = URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.href = url;
                a.download = defaultFilename;
                a.click();
                URL.revokeObjectURL(url);
                setSaveStatus({ type: 'success', message: t('marketplace.cardFileDownloaded') });
            }
        } catch (err) {
            logger.error(`Failed to save contact card file: ${stringifyError(err)}`);
            setSaveStatus({ type: 'error', message: t('marketplace.cardFileSaveFailed') });
        } finally {
            setIsSaving(false);
        }
    };

    const modalTitle = title || t('marketplace.shareCardTitle');
    const modalSubtitle = subtitle || t('marketplace.shareCardSubtitle');

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="md"
            title={
                <div className="flex items-center gap-2.5">
                    <div className="w-9 h-9 rounded-xl bg-theme-primary/10 text-theme-primary flex items-center justify-center">
                        <Share2 size={18} />
                    </div>
                    <div>
                        <h2 className="text-base font-bold text-theme-secondary">{modalTitle}</h2>
                        <p className="text-xs text-theme-light">{modalSubtitle}</p>
                    </div>
                </div>
            }
            footer={
                <div className="flex justify-end w-full">
                    <Button variant="secondary" onClick={onClose} size="sm">
                        {t('common.close')}
                    </Button>
                </div>
            }
        >
            <div className="space-y-5 pt-2">
                {/* QR Code Section */}
                <div className="flex flex-col items-center justify-center p-4 bg-bg-app rounded-2xl border border-theme-subtle">
                    {qrSvg ? (
                        <div className="space-y-2 text-center">
                            <img
                                src={`data:image/svg+xml;utf8,${encodeURIComponent(qrSvg)}`}
                                alt="HuMoCo Contact Card QR Code"
                                className="w-56 h-56 mx-auto rounded-xl p-2 bg-white shadow-sm border border-theme-subtle object-contain"
                            />
                            <p className="text-[11px] text-theme-light flex items-center justify-center gap-1.5 pt-1">
                                <QrCode size={13} />
                                <span>{t('marketplace.qrScanHint')}</span>
                            </p>
                        </div>
                    ) : (
                        <div className="p-4 text-center space-y-2">
                            <div className="w-10 h-10 rounded-full bg-amber-50 text-amber-600 flex items-center justify-center mx-auto border border-amber-200">
                                <AlertCircle size={20} />
                            </div>
                            <h4 className="text-xs font-bold text-theme-secondary">
                                {t('marketplace.qrTooLargeTitle')}
                            </h4>
                            <p className="text-[11px] text-theme-light max-w-xs mx-auto leading-relaxed">
                                {t('marketplace.qrTooLargeDesc')}
                            </p>
                        </div>
                    )}
                </div>

                {/* Actions: File and Link */}
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    {/* Save .hmcard file */}
                    <Button
                        variant="outline"
                        onClick={handleSaveFile}
                        disabled={isSaving}
                        className="h-auto py-3 px-3.5 flex items-start gap-2.5 text-left border-theme-subtle hover:border-theme-primary/30"
                    >
                        <Download size={18} className="text-theme-primary shrink-0 mt-0.5" />
                        <div className="min-w-0">
                            <div className="text-xs font-bold text-theme-secondary">
                                {t('marketplace.saveCardFileBtn')}
                            </div>
                            <div className="text-[10px] text-theme-light truncate">
                                {t('marketplace.saveCardFileDesc')}
                            </div>
                        </div>
                    </Button>

                    {/* Copy Link */}
                    <Button
                        variant="outline"
                        onClick={handleCopy}
                        className="h-auto py-3 px-3.5 flex items-start gap-2.5 text-left border-theme-subtle hover:border-theme-primary/30"
                    >
                        {copied ? (
                            <Check size={18} className="text-theme-success shrink-0 mt-0.5" />
                        ) : (
                            <Copy size={18} className="text-theme-primary shrink-0 mt-0.5" />
                        )}
                        <div className="min-w-0">
                            <div className="text-xs font-bold text-theme-secondary">
                                {copied ? t('marketplace.linkCopied') : t('marketplace.copyCardLinkBtn')}
                            </div>
                            <div className="text-[10px] text-theme-light truncate">
                                {t('marketplace.copyCardLinkDesc')}
                            </div>
                        </div>
                    </Button>
                </div>

                {/* Feedback status */}
                {saveStatus && (
                    <div
                        className={`text-xs p-2.5 rounded-xl border ${
                            saveStatus.type === 'success'
                                ? 'bg-emerald-50 text-emerald-800 border-emerald-200'
                                : 'bg-rose-50 text-rose-800 border-rose-200'
                        }`}
                    >
                        {saveStatus.message}
                    </div>
                )}
            </div>
        </ModalShell>
    );
}
