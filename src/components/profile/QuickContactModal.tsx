// src/components/profile/QuickContactModal.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { QrCode, Copy, Check, FileText, Loader2, AlertCircle } from 'lucide-react';
import Avatar from 'boring-avatars';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { FrontendQuickContact } from '../../types';
import { profileService } from '../../services/profileService';
import { copyToClipboard } from '../../utils/clipboard';
import { buildContactCardText } from '../../utils/humocoUri';
import { truncateUserId } from '../../utils/userIdHelper';

export interface QuickContactModalProps {
    isOpen: boolean;
    onClose: () => void;
    onOpenFullShare?: () => void;
}

export const QuickContactModal: React.FC<QuickContactModalProps> = ({
    isOpen,
    onClose,
    onOpenFullShare,
}) => {
    const { t } = useTranslation();
    const [quickContact, setQuickContact] = useState<FrontendQuickContact | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [copied, setCopied] = useState(false);

    useEffect(() => {
        if (!isOpen) return;
        setIsLoading(true);
        setError(null);
        setCopied(false);

        profileService.getMyQuickContact()
            .then(data => {
                setQuickContact(data);
            })
            .catch(err => {
                setError(typeof err === 'string' ? err : err?.message || 'Fehler beim Laden des Schnellkontakts');
            })
            .finally(() => {
                setIsLoading(false);
            });
    }, [isOpen]);

    const handleCopy = async () => {
        if (!quickContact) return;
        const textToCopy = buildContactCardText({
            did: quickContact.did,
            name: quickContact.name || undefined,
            syncUrls: quickContact.syncUrls.length > 0 ? quickContact.syncUrls : undefined,
        });
        await copyToClipboard(textToCopy);
        setCopied(true);
        setTimeout(() => setCopied(false), 2500);
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="md"
            title={t('contacts.quickContactModalTitle', 'Mein QR-Code')}
            description={t('contacts.quickContactModalDesc', 'Für schnellen Kontakttausch vor Ort oder per Messenger.')}
        >
            <div className="space-y-6 py-2">
                {isLoading ? (
                    <div className="py-12 flex flex-col items-center justify-center gap-3 text-theme-light">
                        <Loader2 className="animate-spin text-theme-primary" size={32} />
                        <span className="text-xs">{t('common.loading', 'Laden...')}</span>
                    </div>
                ) : error ? (
                    <div className="p-4 bg-rose-50 border border-rose-200 rounded-2xl flex items-center gap-3 text-rose-700 text-xs">
                        <AlertCircle size={18} className="shrink-0" />
                        <span>{error}</span>
                    </div>
                ) : quickContact ? (
                    <div className="space-y-6">
                        {/* Person header */}
                        <div className="flex items-center gap-3 p-3 bg-slate-50 border border-theme-subtle/80 rounded-2xl">
                            <Avatar
                                size={44}
                                name={quickContact.did}
                                variant="beam"
                                colors={['#0f172a', '#2563eb', '#38bdf8', '#f8fafc', '#e2e8f0']}
                            />
                            <div className="min-w-0 flex-1">
                                <h4 className="text-sm font-extrabold text-theme-secondary truncate">
                                    {quickContact.name || t('contacts.unnamedContact', 'Unbenannter Kontakt')}
                                </h4>
                                <span className="text-[11px] font-mono text-theme-light">
                                    {truncateUserId(quickContact.did)}
                                </span>
                            </div>
                            {quickContact.syncUrls.length > 0 && (
                                <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800 shrink-0">
                                    {t('contacts.syncAvailable', 'Sync aktiv')}
                                </span>
                            )}
                        </div>

                        {/* Large crisp QR Code */}
                        <div className="flex flex-col items-center justify-center p-6 bg-white border border-theme-subtle rounded-3xl shadow-xs">
                            {quickContact.qrSvg ? (
                                <div
                                    className="w-56 h-56 max-w-full flex items-center justify-center [&>svg]:w-full [&>svg]:h-full"
                                    dangerouslySetInnerHTML={{ __html: quickContact.qrSvg }}
                                    data-testid="quick-contact-qr-svg"
                                />
                            ) : (
                                <div className="w-56 h-56 flex items-center justify-center bg-slate-100 rounded-2xl text-theme-light">
                                    <QrCode size={48} />
                                </div>
                            )}
                            <p className="text-[11px] text-theme-light mt-4 text-center max-w-xs">
                                {t('contacts.quickContactScanHint', 'Mit der Human Money App scannen, um den Kontakt direkt ins Adressbuch aufzunehmen.')}
                            </p>
                        </div>

                        {/* Copy Link Button */}
                        <Button
                            type="button"
                            variant="primary"
                            onClick={handleCopy}
                            className="w-full gap-2 py-2.5"
                            data-testid="quick-contact-copy-button"
                        >
                            {copied ? <Check size={16} /> : <Copy size={16} />}
                            <span>{copied ? t('common.copied', 'Kopiert!') : t('contacts.quickContactCopyBtn', 'Kontaktlink kopieren')}</span>
                        </Button>

                        {/* Subtle Footer Link to Full Card Export */}
                        {onOpenFullShare && (
                            <div className="pt-2 text-center border-t border-theme-subtle/50">
                                <button
                                    type="button"
                                    onClick={() => {
                                        onClose();
                                        onOpenFullShare();
                                    }}
                                    className="text-xs text-theme-light hover:text-theme-primary font-medium inline-flex items-center gap-1.5 cursor-pointer transition-colors p-1"
                                    data-testid="open-full-share-button"
                                >
                                    <FileText size={14} />
                                    <span>{t('contacts.quickContactFullShareLink', 'Vollständige Visitenkarte teilen oder als .hmcard-Datei exportieren')}</span>
                                </button>
                            </div>
                        )}
                    </div>
                ) : null}
            </div>
        </ModalShell>
    );
};
