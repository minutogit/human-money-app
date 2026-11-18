// src/components/marketplace/MarketplaceOptionsPanel.tsx
import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Store, CloudOff, Settings } from 'lucide-react';
import { PageOptionsPanel } from '../ui/PageOptionsPanel';
import { Advanced } from '../ui/Advanced';
import { Button } from '../ui/Button';
import { ReachPresetSelector } from '../settings/ReachPresetSelector';
import { AppSettings, FrontendMyMarketProfile } from '../../types';
import { settingsService } from '../../services/settingsService';
import { useSession } from '../../context/SessionContext';
import { logger } from '../../utils/log';
import { stringifyError, translateError } from '../../utils/errorHelper';

export interface MarketplaceOptionsPanelProps {
    myProfile?: FrontendMyMarketProfile;
    onUpdateMyProfile?: (patch: { maxHops?: number; validityYears?: number; allowInquiries?: boolean }) => Promise<void>;
    storedCardCount?: number;
    onSettingsChanged?: () => void;
    onClose?: () => void;
    hasSyncService?: boolean;
    onNavigateToSyncSettings?: () => void;
    className?: string;
}

export function MarketplaceOptionsPanel({
    myProfile,
    onUpdateMyProfile,
    storedCardCount,
    onSettingsChanged,
    onClose,
    hasSyncService,
    onNavigateToSyncSettings,
    className = '',
}: MarketplaceOptionsPanelProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [marketShowUnknown, setMarketShowUnknown] = useState<boolean>(true);
    const [maxStoredCards, setMaxStoredCards] = useState<number>(2000);
    const [isSaving, setIsSaving] = useState(false);
    const [isUpdatingProfile, setIsUpdatingProfile] = useState(false);
    const [errorMsg, setErrorMsg] = useState<string | null>(null);

    useEffect(() => {
        let isMounted = true;
        settingsService
            .getSettings()
            .then((s) => {
                if (isMounted && s) {
                    setMarketShowUnknown(s.marketShowUnknown ?? true);
                    setMaxStoredCards(s.maxStoredCards ?? 2000);
                }
            })
            .catch((err) => {
                logger.error('Failed to load settings in MarketplaceOptionsPanel: ' + stringifyError(err));
            });
        return () => {
            isMounted = false;
        };
    }, []);

    const handleUpdateProfile = async (patch: { maxHops?: number; validityYears?: number; allowInquiries?: boolean }) => {
        if (!onUpdateMyProfile) return;
        setIsUpdatingProfile(true);
        setErrorMsg(null);
        try {
            await onUpdateMyProfile(patch);
        } catch (err) {
            logger.error('Failed to update profile options in MarketplaceOptionsPanel: ' + stringifyError(err));
            setErrorMsg(translateError(err, t));
        } finally {
            setIsUpdatingProfile(false);
        }
    };

    const handleToggleShowUnknown = async (value: boolean) => {
        setMarketShowUnknown(value);
        try {
            setIsSaving(true);
            setErrorMsg(null);
            const current = await settingsService.getSettings();
            const updated: AppSettings = { ...current, marketShowUnknown: value };
            await protectAction(async (pwd) => {
                await settingsService.saveSettings(updated, pwd || undefined);
            });
            onSettingsChanged?.();
        } catch (err) {
            logger.error('Failed to save marketShowUnknown: ' + stringifyError(err));
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSaving(false);
        }
    };

    const handleSelectMaxStoredCards = async (count: number) => {
        setMaxStoredCards(count);
        try {
            setIsSaving(true);
            setErrorMsg(null);
            const current = await settingsService.getSettings();
            const updated: AppSettings = { ...current, maxStoredCards: count };
            await protectAction(async (pwd) => {
                await settingsService.saveSettings(updated, pwd || undefined);
            });
            onSettingsChanged?.();
        } catch (err) {
            logger.error('Failed to save maxStoredCards: ' + stringifyError(err));
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSaving(false);
        }
    };

    return (
        <PageOptionsPanel
            title={t('marketplace.optionsTitle', 'Marktplatz-Optionen & Reichweite')}
            icon={<Store size={18} />}
            onClose={onClose}
            className={className}
            data-testid="marketplace-options-panel"
        >
            <div className="space-y-4">
                {errorMsg && (
                    <div className="p-3 bg-rose-50 border border-rose-200 text-rose-700 text-xs rounded-xl flex items-center justify-between">
                        <span>{errorMsg}</span>
                        <button
                            type="button"
                            onClick={() => setErrorMsg(null)}
                            className="text-rose-500 hover:text-rose-700 font-bold ml-2 cursor-pointer"
                        >
                            ×
                        </button>
                    </div>
                )}

                {/* Section 1: Own Business Card & Reachability (Outbound) */}
                {myProfile && onUpdateMyProfile && (
                    <div className="space-y-3 pb-3 border-b border-theme-subtle/40" data-testid="marketplace-options-outbound-section">
                        <div className="flex items-center gap-2">
                            <h5 className="text-xs font-black uppercase tracking-wider text-theme-secondary">
                                {t('marketplace.optionsOutboundSection', 'Meine Visitenkarte & Reichweite')}
                            </h5>
                        </div>

                        {/* Unified 3-tier Reach Selector */}
                        <ReachPresetSelector
                            initialCardMaxHops={myProfile.maxHops || 2}
                            disabled={isSaving || isUpdatingProfile}
                            onCardMaxHopsChange={async (hops) => {
                                await handleUpdateProfile({ maxHops: hops });
                            }}
                            onChanged={() => onSettingsChanged?.()}
                        />

                        {/* Validity of own business card (Expert only) */}
                        <Advanced>
                            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pt-2 border-t border-theme-subtle/30">
                                <div className="flex-1 min-w-0 pr-2">
                                    <label htmlFor="validityYearsSelect" className="text-xs font-bold text-theme-secondary block">
                                        {t('marketplace.validityYearsLabel', 'Gültigkeit der Visitenkarte')}
                                    </label>
                                    <p className="text-xs text-theme-light leading-relaxed">
                                        {t('marketplace.validityYearsDesc', 'Gibt an, nach wie vielen Jahren dein Profil erneuert werden muss.')}
                                    </p>
                                </div>
                                <select
                                    id="validityYearsSelect"
                                    data-testid="validity-years-select"
                                    disabled={isSaving || isUpdatingProfile}
                                    value={myProfile.validityYears ?? 2}
                                    onChange={(e) => handleUpdateProfile({ validityYears: Number(e.target.value) })}
                                    className="px-3 py-1.5 rounded-xl text-xs font-medium bg-white border border-theme-subtle text-theme-secondary shrink-0 focus:outline-none focus:ring-2 focus:ring-theme-primary/30 cursor-pointer"
                                >
                                    <option value={1}>{t('marketplace.validityYears1', '1 Jahr')}</option>
                                    <option value={2}>{t('marketplace.validityYears2', '2 Jahre (Standard)')}</option>
                                    <option value={3}>{t('marketplace.validityYears3', '3 Jahre')}</option>
                                </select>
                            </div>
                        </Advanced>

                        {/* Allow general inquiries toggle */}
                        <div className="flex items-start gap-3 p-3 bg-white/70 border border-theme-subtle rounded-xl hover:border-theme-primary/30 transition-colors">
                            <input
                                type="checkbox"
                                id="panelAllowInquiries"
                                checked={myProfile.allowInquiries}
                                disabled={isSaving || isUpdatingProfile}
                                onChange={(e) => handleUpdateProfile({ allowInquiries: e.target.checked })}
                                className="mt-0.5 accent-theme-primary cursor-pointer"
                            />
                            <div className="flex-1">
                                <label
                                    htmlFor="panelAllowInquiries"
                                    className="text-xs font-bold text-theme-secondary leading-relaxed cursor-pointer"
                                >
                                    {t('marketplace.allowInquiriesGeneralLabel', 'Anfragen grundsätzlich erlauben')}
                                </label>
                                <label
                                    htmlFor="panelAllowInquiries"
                                    className="block text-xs text-theme-light font-normal cursor-pointer mt-0.5"
                                >
                                    {t('marketplace.allowInquiriesGeneralDesc', 'Ermöglicht anderen Kontakten, dir direkte Nachrichten oder Anfragen zu deinen Fähigkeiten zu senden.')}
                                </label>
                            </div>
                        </div>
                    </div>
                )}

                {/* Section 2: Marketplace Storage & Cleanup (Inbound, Expert only) */}
                <Advanced>
                    <div className="space-y-3" data-testid="marketplace-options-inbound-section">
                        <div className="flex items-center justify-between">
                            <h5 className="text-xs font-black uppercase tracking-wider text-theme-secondary">
                                {t('marketplace.optionsInboundSection', 'Kartenspeicher & Bereinigung')}
                            </h5>
                            {storedCardCount !== undefined && (
                                <span
                                    className="px-2 py-0.5 rounded-full text-xs font-semibold bg-theme-primary/10 text-theme-primary border border-theme-primary/20"
                                    data-testid="stored-cards-count-badge"
                                >
                                    {`${storedCardCount} / ${maxStoredCards} ${t('settings.storedCardsCountUnit', 'Karten belegt')}`}
                                </span>
                            )}
                        </div>

                        {/* Max Stored Cards Select */}
                        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                            <div className="flex-1 min-w-0 pr-2">
                                <label htmlFor="maxStoredCardsSelect" className="text-xs font-bold text-theme-secondary block">
                                    {t('settings.maxStoredCards', 'Lokaler Kartenspeicher')}
                                </label>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('settings.maxStoredCardsDesc', 'Abgelaufene Profile werden automatisch entfernt. Bei vollem Speicherplatz weichen zuerst weit entfernte, rein lokale Profile aus dem erweiterten Netzwerk (direkte Kontakte bleiben immer geschützt).')}
                                </p>
                            </div>
                            <select
                                id="maxStoredCardsSelect"
                                data-testid="max-stored-cards-select"
                                disabled={isSaving || isUpdatingProfile}
                                value={maxStoredCards}
                                onChange={(e) => handleSelectMaxStoredCards(Number(e.target.value))}
                                className="px-3 py-1.5 rounded-xl text-xs font-medium bg-white border border-theme-subtle text-theme-secondary shrink-0 focus:outline-none focus:ring-2 focus:ring-theme-primary/30 cursor-pointer"
                            >
                                <option value={500}>{t('settings.maxStoredCards500', 'Sparsam (max. 500 Karten)')}</option>
                                <option value={2000}>{t('settings.maxStoredCards2000', 'Standard (max. 2.000 Karten)')}</option>
                                <option value={5000}>{t('settings.maxStoredCards5000', 'Groß (max. 5.000 Karten)')}</option>
                            </select>
                        </div>

                        {/* Marketplace Show Unknown Creators Toggle */}
                        <div className="flex items-start gap-3 p-3 bg-white/70 border border-theme-subtle rounded-xl hover:border-theme-primary/30 transition-colors">
                            <input
                                type="checkbox"
                                id="panelMarketShowUnknown"
                                checked={marketShowUnknown}
                                disabled={isSaving || isUpdatingProfile}
                                onChange={(e) => handleToggleShowUnknown(e.target.checked)}
                                className="mt-0.5 accent-theme-primary cursor-pointer"
                            />
                            <div className="flex-1">
                                <label
                                    htmlFor="panelMarketShowUnknown"
                                    className="text-xs font-bold text-theme-secondary leading-relaxed cursor-pointer"
                                >
                                    {t('settings.marketShowUnknown')}
                                </label>
                                <label
                                    htmlFor="panelMarketShowUnknown"
                                    className="block text-xs text-theme-light font-normal cursor-pointer mt-0.5"
                                >
                                    {t('settings.marketShowUnknownDesc')}
                                </label>
                            </div>
                        </div>
                    </div>
                </Advanced>

                {/* Sync Service recommendation if not connected */}
                {hasSyncService === false && (
                    <div
                        data-testid="marketplace-sync-recommendation"
                        className="p-3 rounded-xl bg-amber-500/10 border border-amber-500/20 flex flex-col sm:flex-row sm:items-center justify-between gap-3 text-amber-950"
                    >
                        <div className="flex items-start sm:items-center gap-2.5">
                            <CloudOff size={16} className="text-amber-600 shrink-0 mt-0.5 sm:mt-0" />
                            <p className="text-xs font-medium text-amber-900 leading-relaxed">
                                <span className="font-bold text-amber-950">
                                    {t('contacts.syncRecommendationTitle', 'Empfohlen')}:
                                </span>{' '}
                                {t(
                                    'contacts.syncRecommendationDesc',
                                    'Verbinde einen Sync-Dienst, damit Zahlungen, Bürgschaften und Vertrauenseinstufungen automatisch im Hintergrund fließen und du Dateien nicht manuell austauschen musst.'
                                )}
                            </p>
                        </div>
                        {onNavigateToSyncSettings && (
                            <Button
                                type="button"
                                size="xs"
                                variant="secondary"
                                onClick={onNavigateToSyncSettings}
                                className="self-start sm:self-auto shrink-0 bg-white hover:bg-amber-50 border-amber-200 text-amber-900 font-bold text-xs gap-1.5 shadow-xs"
                            >
                                <Settings size={13} />
                                <span>{t('contacts.connectSyncServiceBtn', 'Sync-Dienst verbinden')}</span>
                            </Button>
                        )}
                    </div>
                )}
            </div>
        </PageOptionsPanel>
    );
}

export default MarketplaceOptionsPanel;
