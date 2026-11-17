import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { QrCode, Copy, Check, Loader2, AlertCircle, Download, Info, Globe } from 'lucide-react';
import { UserAvatar } from '../ui/UserAvatar';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { SegmentedTabs, SegmentedTabItem } from '../ui/SegmentedTabs';
import { TechId } from '../ui/TechId';
import { FrontendQuickContact } from '../../types';
import { FrontendMyMarketProfile } from '../../types/market';
import { profileService } from '../../services/profileService';
import { marketService } from '../../services/marketService';
import { useOptionalSession } from '../../context/SessionContext';
import { copyToClipboard } from '../../utils/clipboard';
import { buildContactCardText } from '../../utils/humocoUri';
import { truncateUserId } from '../../utils/userIdHelper';
import { logger } from '../../utils/log';
import { stringifyError } from '../../utils/errorHelper';

export interface QuickContactModalProps {
    isOpen: boolean;
    onClose: () => void;
}

type ShareTab = 'qr' | 'card';

export const QuickContactModal: React.FC<QuickContactModalProps> = ({
    isOpen,
    onClose,
}) => {
    const { t } = useTranslation();
    const session = useOptionalSession();

    const [activeTab, setActiveTab] = useState<ShareTab>('qr');
    const [quickContact, setQuickContact] = useState<FrontendQuickContact | null>(null);
    const [marketProfile, setMarketProfile] = useState<FrontendMyMarketProfile | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [copied, setCopied] = useState(false);
    const [cardLinkCopied, setCardLinkCopied] = useState(false);
    const [isSavingFile, setIsSavingFile] = useState(false);
    const [isPublishingCard, setIsPublishingCard] = useState(false);
    const [saveStatus, setSaveStatus] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    useEffect(() => {
        let active = true;
        if (!isOpen) return;
        setActiveTab('qr');
        setIsLoading(true);
        setError(null);
        setCopied(false);
        setCardLinkCopied(false);
        setSaveStatus(null);

        const loadCard = typeof marketService?.getMyMarketProfile === 'function'
            ? marketService.getMyMarketProfile().catch(() => null)
            : Promise.resolve(null);

        Promise.all([profileService.getMyQuickContact(), loadCard])
            .then(([quickData, cardData]) => {
                if (active) {
                    setQuickContact(quickData);
                    setMarketProfile(cardData);
                }
            })
            .catch(err => {
                if (active) {
                    setError(typeof err === 'string' ? err : err?.message || t('contacts.quickContactLoadError', 'Fehler beim Laden des Schnellkontakts'));
                }
            })
            .finally(() => {
                if (active) setIsLoading(false);
            });

        return () => {
            active = false;
        };
    }, [isOpen, t]);

    const handleCopyQuickContact = async () => {
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

    const handleCopyCardLink = async () => {
        if (!marketProfile?.shareUri) return;
        const ok = await copyToClipboard(marketProfile.shareUri);
        if (ok) {
            setCardLinkCopied(true);
            setTimeout(() => setCardLinkCopied(false), 2000);
        }
    };

    const handleSaveCardFile = async () => {
        const fileWire = marketProfile?.fileWire;
        if (!fileWire) return;
        setIsSavingFile(true);
        setSaveStatus(null);
        try {
            const shortDid = quickContact?.did
                ? truncateUserId(quickContact.did).replace(/[^a-zA-Z0-9_-]/g, '_')
                : 'contact';
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
            setIsSavingFile(false);
        }
    };

    const handlePublishCard = async () => {
        if (typeof marketService?.setCardPublished !== 'function') return;
        setIsPublishingCard(true);
        setSaveStatus(null);
        try {
            const updated = session
                ? await session.protectAction(async (pwd) => marketService.setCardPublished(true, false, pwd || undefined))
                : await marketService.setCardPublished(true, false);
            if (updated) {
                setMarketProfile(updated);
            }
        } catch (err) {
            logger.error(`Failed to publish market card from share dialog: ${stringifyError(err)}`);
            setSaveStatus({
                type: 'error',
                message: stringifyError(err) || t('contacts.publishCardFailed', 'Veröffentlichen der Visitenkarte fehlgeschlagen.'),
            });
        } finally {
            setIsPublishingCard(false);
        }
    };

    const shareTabs: SegmentedTabItem<ShareTab>[] = [
        {
            id: 'qr',
            label: t('contacts.shareTabQr', 'Nur Name & Kontakt'),
            testId: 'share-tab-qr',
        },
        {
            id: 'card',
            label: t('contacts.shareTabCard', 'Mit meiner Visitenkarte'),
            testId: 'share-tab-card',
        },
    ];

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onClose}
            maxWidth="md"
            title={t('contacts.shareProfileModalTitle', 'Mein Profil teilen')}
            description={t('contacts.shareProfileModalDesc', 'Teile deinen Kontakt-Code vor Ort per QR oder gib deine Visitenkarte weiter.')}
        >
            <div className="space-y-5 py-2">
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
                    <div className="space-y-4">
                        {/* Person header */}
                        <div className="flex items-center gap-3 p-3 bg-slate-50 border border-theme-subtle/80 rounded-2xl">
                            <UserAvatar
                                size={44}
                                pictureUrl={marketProfile?.profile?.pictureUrl || session?.userAvatarUrl}
                                name={quickContact.name || quickContact.did}
                            />
                            <div className="min-w-0 flex-1 space-y-0.5">
                                <h4 className="text-sm font-extrabold text-theme-secondary truncate">
                                    {quickContact.name || t('contacts.unnamedContact', 'Unbenannter Kontakt')}
                                </h4>
                                <TechId value={quickContact.did} kind="did" />
                            </div>
                            {quickContact.syncUrls.length > 0 && (
                                <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800 shrink-0">
                                    {t('contacts.syncAvailable', 'Sync aktiv')}
                                </span>
                            )}
                        </div>

                        <SegmentedTabs<ShareTab>
                            tabs={shareTabs}
                            activeTab={activeTab}
                            onChange={setActiveTab}
                            fullWidth
                        />

                        {activeTab === 'qr' ? (
                            <div className="space-y-4" data-testid="share-qr-panel">
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
                                    onClick={handleCopyQuickContact}
                                    className="w-full gap-2 py-2.5"
                                    data-testid="quick-contact-copy-button"
                                >
                                    {copied ? <Check size={16} /> : <Copy size={16} />}
                                    <span>{copied ? t('common.copied', 'Kopiert!') : t('contacts.quickContactCopyBtn', 'Kontaktlink kopieren')}</span>
                                </Button>
                            </div>
                        ) : (
                            <div className="space-y-4" data-testid="share-card-panel">
                                {!marketProfile?.published ? (
                                    <div className="p-5 bg-amber-50/70 border border-amber-200 rounded-2xl space-y-3 text-center">
                                        <div className="w-10 h-10 rounded-full bg-amber-100 text-amber-700 flex items-center justify-center mx-auto">
                                            <Globe size={20} />
                                        </div>
                                        <div className="space-y-1">
                                            <p className="text-sm font-bold text-amber-900">
                                                {t('contacts.shareCardNotPublishedTitle', 'Deine Visitenkarte ist noch nicht veröffentlicht')}
                                            </p>
                                            <p className="text-xs text-amber-800/90 leading-relaxed">
                                                {t('contacts.shareCardNotPublishedHint', 'Erst wenn du deine Visitenkarte veröffentlichst, wird sie als signierte Datei (.hmcard) und Link zum Teilen vorbereitet.')}
                                            </p>
                                        </div>
                                        <Button
                                            type="button"
                                            variant="primary"
                                            size="sm"
                                            onClick={handlePublishCard}
                                            disabled={isPublishingCard || typeof marketService?.setCardPublished !== 'function'}
                                            className="gap-1.5"
                                            data-testid="publish-card-in-share-btn"
                                        >
                                            {isPublishingCard ? <Loader2 size={14} className="animate-spin" /> : <Globe size={14} />}
                                            <span>{t('contacts.publishCardNowBtn', 'Visitenkarte jetzt veröffentlichen')}</span>
                                        </Button>
                                        {saveStatus && saveStatus.type === 'error' && (
                                            <div className="text-xs p-2.5 rounded-xl border bg-rose-50 text-rose-800 border-rose-200 text-left">
                                                {saveStatus.message}
                                            </div>
                                        )}
                                    </div>
                                ) : (
                                    <>
                                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                                            <Button
                                                variant="primary"
                                                onClick={handleSaveCardFile}
                                                disabled={isSavingFile || !marketProfile.fileWire}
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
                                                variant="outline"
                                                onClick={handleCopyCardLink}
                                                disabled={!marketProfile.shareUri}
                                                className="h-auto py-3 px-3.5 flex items-start gap-2.5 text-left border-theme-subtle hover:border-theme-primary/30"
                                                data-testid="share-card-copy-link-button"
                                            >
                                                {cardLinkCopied ? (
                                                    <Check size={18} className="text-theme-success shrink-0 mt-0.5" />
                                                ) : (
                                                    <Copy size={18} className="text-theme-primary shrink-0 mt-0.5" />
                                                )}
                                                <div className="min-w-0">
                                                    <div className="text-xs font-bold text-theme-secondary">
                                                        {cardLinkCopied ? t('marketplace.linkCopied', 'Link kopiert!') : t('marketplace.copyCardLinkBtn', 'Link kopieren')}
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
                                            {marketProfile.qrSvg ? (
                                                <div className="space-y-3 text-center w-full">
                                                    <img
                                                        src={`data:image/svg+xml;utf8,${encodeURIComponent(marketProfile.qrSvg)}`}
                                                        alt="HuMoCo Visitenkarte QR Code"
                                                        className="w-52 h-52 mx-auto rounded-xl p-2 bg-white shadow-sm border border-theme-subtle object-contain"
                                                        data-testid="share-card-qr-img"
                                                    />
                                                    <p className="text-[11px] text-theme-light flex items-center justify-center gap-1.5 pt-1">
                                                        <QrCode size={13} />
                                                        <span>{t('marketplace.qrScanHint', 'Mit der Human Money Kamera scannen')}</span>
                                                    </p>
                                                    <p className="text-[10px] text-theme-light/80 italic text-center max-w-xs mx-auto">
                                                        {t('marketplace.qrLargeRecommendation', 'Tipp: Für Mobilgeräte und umfangreiche Inserate wird die Übertragung als .hmcard-Datei empfohlen.')}
                                                    </p>
                                                    {marketProfile.qrIsPartial && (
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
                                                    <p className="text-xs font-bold text-theme-secondary">
                                                        {t('marketplace.qrTooLargeTitle', 'Visitenkarte zu umfangreich für einen QR-Code')}
                                                    </p>
                                                    <p className="text-xs text-theme-light leading-relaxed max-w-xs">
                                                        {t('marketplace.qrTooLargeDesc', 'Bitte speichere sie oben als .hmcard-Datei oder kopiere den Link.')}
                                                    </p>
                                                </div>
                                            )}
                                        </div>
                                    </>
                                )}
                            </div>
                        )}
                    </div>
                ) : null}
            </div>
        </ModalShell>
    );
};
