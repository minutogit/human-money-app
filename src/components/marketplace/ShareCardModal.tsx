// src/components/marketplace/ShareCardModal.tsx
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { QrCode, Download, Copy, Check, AlertCircle, Share2, Info } from 'lucide-react';
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
    fileWire?: string | null;
    shareUri?: string | null;
    qrSvg?: string | null;
    did?: string;
    qrIsPartial?: boolean;
}

export function ShareCardModal({
    isOpen,
    onClose,
    title,
    subtitle,
    fileWire,
    shareUri,
    qrSvg,
    did,
    qrIsPartial = false,
}: ShareCardModalProps) {
    const { t } = useTranslation();
    const [copied, setCopied] = useState(false);
    const [isSaving, setIsSaving] = useState(false);
    const [saveStatus, setSaveStatus] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    const handleCopy = async () => {
        if (!shareUri) return;
        const ok = await copyToClipboard(shareUri);
        if (ok) {
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        }
    };

    const handleSaveFile = async () => {
        if (!fileWire) return;
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
                        { name: t('marketplace.cardFilterName', 'HuMoCo Visitenkarte (.hmcard)'), extensions: ['hmcard'] },
                    ],
                });
            } catch (err) {
                logger.info(`Tauri save dialog not available, using browser download: ${stringifyError(err)}`);
            }

            if (filePath) {
                const bytes = new TextEncoder().encode(fileWire);
                await writeFile(filePath, bytes);
                setSaveStatus({ type: 'success', message: t('marketplace.cardFileSaved', 'Datei erfolgreich gespeichert.') });
            } else if (filePath === null && typeof window !== 'undefined' && window.document) {
                const blob = new Blob([fileWire], { type: 'text/plain;charset=utf-8' });
                const url = URL.createObjectURL(blob);
                const a = document.createElement('a');
                a.href = url;
                a.download = defaultFilename;
                a.click();
                URL.revokeObjectURL(url);
                setSaveStatus({ type: 'success', message: t('marketplace.cardFileDownloaded', 'Datei heruntergeladen.') });
            }
        } catch (err) {
            logger.error(`Failed to save contact card file: ${stringifyError(err)}`);
            setSaveStatus({ type: 'error', message: t('marketplace.cardFileSaveFailed', 'Speichern der Datei fehlgeschlagen.') });
        } finally {
            setIsSaving(false);
        }
    };

    const modalTitle = title || t('marketplace.shareCardTitle', 'Visitenkarte teilen');
    const modalSubtitle = subtitle || t('marketplace.shareCardSubtitle', 'Teilen per QR-Code, Datei oder Link');

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
                        {t('common.close', 'Schließen')}
                    </Button>
                </div>
            }
        >
            <div className="space-y-5 pt-2">
                {/* Actions: File and Link at top */}
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                    {/* Save .hmcard file */}
                    <Button
                        variant="primary"
                        onClick={handleSaveFile}
                        disabled={isSaving || !fileWire}
                        className="h-auto py-3 px-3.5 flex items-start gap-2.5 text-left shadow-sm"
                        data-testid="share-card-save-file-button"
                    >
                        <Download size={18} className="shrink-0 mt-0.5" />
                        <div className="min-w-0">
                            <div className="text-xs font-bold">
                                {t('marketplace.saveCardFileBtn', 'Als Datei speichern / teilen (.hmcard)')}
                            </div>
                            <div className="text-[10px] opacity-90 truncate">
                                {t('marketplace.saveCardFileDesc', 'Empfohlen für Bluetooth, AirDrop & Chat')}
                            </div>
                        </div>
                    </Button>

                    {/* Copy Link */}
                    <Button
                        variant="outline"
                        onClick={handleCopy}
                        disabled={!shareUri}
                        className="h-auto py-3 px-3.5 flex items-start gap-2.5 text-left border-theme-subtle hover:border-theme-primary/30"
                        data-testid="share-card-copy-link-button"
                    >
                        {copied ? (
                            <Check size={18} className="text-theme-success shrink-0 mt-0.5" />
                        ) : (
                            <Copy size={18} className="text-theme-primary shrink-0 mt-0.5" />
                        )}
                        <div className="min-w-0">
                            <div className="text-xs font-bold text-theme-secondary">
                                {copied ? t('marketplace.linkCopied', 'Link kopiert!') : t('marketplace.copyCardLinkBtn', 'Link kopieren')}
                            </div>
                            <div className="text-[10px] text-theme-light truncate">
                                {t('marketplace.copyCardLinkDesc', 'Kopiert humoco://card Link')}
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

                {/* QR Code Section below */}
                <div className="flex flex-col items-center justify-center p-4 bg-bg-app rounded-2xl border border-theme-subtle">
                    {qrSvg ? (
                        <div className="space-y-3 text-center w-full">
                            <img
                                src={`data:image/svg+xml;utf8,${encodeURIComponent(qrSvg)}`}
                                alt="HuMoCo Contact Card QR Code"
                                className="w-56 h-56 mx-auto rounded-xl p-2 bg-white shadow-sm border border-theme-subtle object-contain"
                            />
                            <p className="text-[11px] text-theme-light flex items-center justify-center gap-1.5 pt-1">
                                <QrCode size={13} />
                                <span>{t('marketplace.qrScanHint', 'Mit der Human Money Kamera scannen')}</span>
                            </p>
                            <p className="text-[10px] text-theme-light/80 italic text-center max-w-xs mx-auto">
                                {t('marketplace.qrLargeRecommendation', 'Tipp: Für Mobilgeräte und umfangreiche Inserate wird die Übertragung als .hmcard-Datei empfohlen.')}
                            </p>
                            {qrIsPartial && (
                                <div className="text-xs p-3 rounded-xl bg-amber-50 border border-amber-200 text-amber-800 flex items-start gap-2 text-left mt-2">
                                    <Info size={16} className="text-amber-600 shrink-0 mt-0.5" />
                                    <p className="leading-relaxed">
                                        {t('marketplace.qrPartialNotice', 'Der QR-Code enthält die wichtigsten Angaben. Den Rest bekommt dein Gegenüber über den Sync-Dienst oder als Datei.')}
                                    </p>
                                </div>
                            )}
                        </div>
                    ) : (
                        <div className="p-4 text-center space-y-2">
                            <div className="w-10 h-10 rounded-full bg-amber-50 text-amber-600 flex items-center justify-center mx-auto border border-amber-200">
                                <AlertCircle size={20} />
                            </div>
                            <h4 className="text-xs font-bold text-theme-secondary">
                                {t('marketplace.qrTooLargeTitle', 'Zu groß für QR-Code')}
                            </h4>
                            <p className="text-[11px] text-theme-light max-w-xs mx-auto leading-relaxed">
                                {t('marketplace.qrTooLargeDesc', 'Aufgrund umfangreicher Angaben ist der Datenumfang zu groß für einen QR-Code. Bitte teile die Visitenkarte als Datei oder Direktlink.')}
                            </p>
                        </div>
                    )}
                </div>
            </div>
        </ModalShell>
    );
}
