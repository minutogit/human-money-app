import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, AlertCircle, Globe } from 'lucide-react';
import { UserAvatar } from '../ui/UserAvatar';
import { ModalShell } from '../ui/ModalShell';
import { Button } from '../ui/Button';
import { CopyButton } from '../ui/CopyButton';
import { QrCodeDisplay } from '../ui/QrCodeDisplay';
import { SegmentedTabs, SegmentedTabItem } from '../ui/SegmentedTabs';
import { TechId } from '../ui/TechId';
import { ShareCardPanel } from './ShareCardPanel';
import { FrontendQuickContact } from '../../types';
import { FrontendMyMarketProfile } from '../../types/market';
import { profileService } from '../../services/profileService';
import { marketService } from '../../services/marketService';
import { useOptionalSession } from '../../context/SessionContext';
import { buildContactUri } from '../../utils/humocoUri';
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
    const [isPublishingCard, setIsPublishingCard] = useState(false);
    const [publishError, setPublishError] = useState<string | null>(null);

    useEffect(() => {
        let active = true;
        if (!isOpen) return;
        setActiveTab('qr');
        setIsLoading(true);
        setError(null);
        setPublishError(null);

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

    const handlePublishCard = async () => {
        if (typeof marketService?.setCardPublished !== 'function') return;
        setIsPublishingCard(true);
        setPublishError(null);
        try {
            const updated = session
                ? await session.protectAction(async (pwd) => marketService.setCardPublished(true, false, pwd || undefined))
                : await marketService.setCardPublished(true, false);
            if (updated) {
                setMarketProfile(updated);
            }
        } catch (err) {
            logger.error(`Failed to publish market card from share dialog: ${stringifyError(err)}`);
            setPublishError(
                stringifyError(err) || t('contacts.publishCardFailed', 'Veröffentlichen der Visitenkarte fehlgeschlagen.')
            );
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

    const quickContactCopyText = quickContact
        ? buildContactUri({
              did: quickContact.did,
              name: quickContact.name || undefined,
              syncUrls: quickContact.syncUrls.length > 0 ? quickContact.syncUrls : undefined,
          })
        : '';

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
                                <div className="flex flex-col items-center justify-center p-6 bg-white border border-theme-subtle rounded-3xl shadow-xs">
                                    <QrCodeDisplay
                                        svg={quickContact.qrSvg}
                                        value={quickContact.uri}
                                        size={224}
                                        includeMargin={false}
                                        renderAs="inline"
                                        qrTestId="quick-contact-qr-svg"
                                        ariaLabel={t('contacts.shareProfileModalTitle', 'Mein Profil teilen')}
                                        caption={t('contacts.quickContactScanHint', 'Mit der Human Money App scannen, um den Kontakt direkt ins Adressbuch aufzunehmen.')}
                                    />
                                </div>

                                <CopyButton
                                    value={quickContactCopyText}
                                    variant="primary"
                                    size="md"
                                    label={t('contacts.quickContactCopyBtn', 'Kontaktlink kopieren')}
                                    copiedLabel={t('common.copied', 'Kopiert!')}
                                    className="w-full py-2.5"
                                    data-testid="quick-contact-copy-button"
                                />
                            </div>
                        ) : !marketProfile?.published ? (
                            <div className="space-y-4" data-testid="share-card-panel">
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
                                    {publishError && (
                                        <div className="text-xs p-2.5 rounded-xl border bg-rose-50 text-rose-800 border-rose-200 text-left">
                                            {publishError}
                                        </div>
                                    )}
                                </div>
                            </div>
                        ) : (
                            <ShareCardPanel
                                data-testid="share-card-panel"
                                fileWire={marketProfile.fileWire}
                                shareUri={marketProfile.shareUri}
                                qrSvg={marketProfile.qrSvg}
                                did={quickContact.did}
                                qrIsPartial={marketProfile.qrIsPartial}
                                qrSize={208}
                                resetKey={`${isOpen}-${activeTab}`}
                            />
                        )}
                    </div>
                ) : null}
            </div>
        </ModalShell>
    );
};
