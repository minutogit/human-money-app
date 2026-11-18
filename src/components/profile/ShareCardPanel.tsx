// src/components/profile/ShareCardPanel.tsx
import React, { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertCircle, Check, Copy, Download, Info, QrCode } from 'lucide-react';
import { Button } from '../ui/Button';
import { QrCodeDisplay } from '../ui/QrCodeDisplay';
import { useCopyFeedback } from '../../hooks/useCopyFeedback';
import { saveBytesToFile } from '../../utils/fileIo';
import { stringifyError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import { truncateUserId } from '../../utils/userIdHelper';

export interface ShareCardPanelProps {
    fileWire?: string | null;
    shareUri?: string | null;
    qrSvg?: string | null;
    did?: string;
    qrIsPartial?: boolean;
    /** Pixel size of the rendered QR code container. Defaults to 224. */
    qrSize?: number;
    /** Changing value resets transient copy and file-save feedback state. */
    resetKey?: unknown;
    className?: string;
    'data-testid'?: string;
}

/**
 * Shared business card export panel supporting .hmcard file download/save,
 * humoco://card link copy, QR rendering, and partial-QR / forwarding hints.
 */
export const ShareCardPanel: React.FC<ShareCardPanelProps> = ({
    fileWire,
    shareUri,
    qrSvg,
    did,
    qrIsPartial = false,
    qrSize = 224,
    resetKey,
    className = 'space-y-4',
    'data-testid': testId,
}) => {
    const { t } = useTranslation();
    const { copied: linkCopied, copyText, reset: resetCopyFeedback } = useCopyFeedback(2000);
    const [isSavingFile, setIsSavingFile] = useState(false);
    const [saveStatus, setSaveStatus] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    useEffect(() => {
        setSaveStatus(null);
        resetCopyFeedback();
    }, [resetKey, resetCopyFeedback]);

    const handleCopyLink = () => {
        if (!shareUri) return;
        void copyText(shareUri);
    };

    const handleSaveFile = async () => {
        if (!fileWire) return;
        setIsSavingFile(true);
        setSaveStatus(null);
        try {
            const shortDid = did ? truncateUserId(did).replace(/[^a-zA-Z0-9_-]/g, '_') : 'contact';
            const defaultFilename = `humoco-${shortDid}.hmcard`;
            const bytes = new TextEncoder().encode(fileWire);
            const result = await saveBytesToFile(bytes, {
                defaultFilename,
                filters: [
                    { name: t('marketplace.cardFilterName', 'HuMoCo Visitenkarte (.hmcard)'), extensions: ['hmcard'] },
                ],
                mimeType: 'text/plain;charset=utf-8',
            });

            if (result) {
                setSaveStatus({
                    type: 'success',
                    message: result.downloaded
                        ? t('marketplace.cardFileDownloaded', 'Datei heruntergeladen.')
                        : t('marketplace.cardFileSaved', 'Datei erfolgreich gespeichert.'),
                });
            }
        } catch (err) {
            logger.error(`Failed to save contact card file: ${stringifyError(err)}`);
            setSaveStatus({
                type: 'error',
                message: t('marketplace.cardFileSaveFailed', 'Speichern der Datei fehlgeschlagen.'),
            });
        } finally {
            setIsSavingFile(false);
        }
    };

    return (
        <div className={className} data-testid={testId}>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <Button
                    type="button"
                    variant="primary"
                    onClick={handleSaveFile}
                    disabled={isSavingFile || !fileWire}
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

                <Button
                    type="button"
                    variant="outline"
                    onClick={handleCopyLink}
                    disabled={!shareUri}
                    className="h-auto py-3 px-3.5 flex items-start gap-2.5 text-left border-theme-subtle hover:border-theme-primary/30"
                    data-testid="share-card-copy-link-button"
                >
                    {linkCopied ? (
                        <Check size={18} className="text-theme-success shrink-0 mt-0.5" />
                    ) : (
                        <Copy size={18} className="text-theme-primary shrink-0 mt-0.5" />
                    )}
                    <div className="min-w-0">
                        <div className="text-xs font-bold text-theme-secondary">
                            {linkCopied ? t('marketplace.linkCopied', 'Link kopiert!') : t('marketplace.copyCardLinkBtn', 'Link kopieren')}
                        </div>
                        <div className="text-[10px] text-theme-light truncate">
                            {t('marketplace.copyCardLinkDesc', 'Kopiert humoco://card Link')}
                        </div>
                    </div>
                </Button>
            </div>

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

            <div className="flex flex-col items-center justify-center p-4 bg-bg-app rounded-2xl border border-theme-subtle">
                {qrSvg ? (
                    <div className="space-y-3 text-center w-full">
                        <QrCodeDisplay
                            svg={qrSvg}
                            size={qrSize}
                            renderAs="img"
                            includeMargin={true}
                            ariaLabel="HuMoCo Contact Card QR Code"
                            qrTestId="share-card-qr-img"
                            className="w-full"
                            caption={
                                <div className="space-y-1.5 pt-1">
                                    <p className="text-[11px] text-theme-light flex items-center justify-center gap-1.5">
                                        <QrCode size={13} />
                                        <span>{t('marketplace.qrScanHint', 'Mit der Human Money Kamera scannen')}</span>
                                    </p>
                                    <p className="text-[10px] text-theme-light/80 italic text-center max-w-xs mx-auto">
                                        {t('marketplace.qrLargeRecommendation', 'Tipp: Für Mobilgeräte und umfangreiche Inserate wird die Übertragung als .hmcard-Datei empfohlen.')}
                                    </p>
                                </div>
                            }
                        />
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
    );
};
