// src/components/marketplace/MarketplaceOptionsPanel.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Store, CloudOff, Settings } from 'lucide-react';
import { PageOptionsPanel } from '../ui/PageOptionsPanel';
import { Button } from '../ui/Button';
import { AppSettings, FrontendMyMarketProfile } from '../../types';
import { settingsService } from '../../services/settingsService';
import { logger } from '../../utils/log';
import { stringifyError, translateError } from '../../utils/errorHelper';

export interface MarketplaceOptionsPanelProps {
    myProfile?: FrontendMyMarketProfile;
    onUpdateMyProfile?: (patch: { maxHops?: number; allowInquiries?: boolean }) => Promise<void>;
    onSettingsChanged?: () => void;
    onClose?: () => void;
    hasSyncService?: boolean;
    onNavigateToSyncSettings?: () => void;
    className?: string;
}

export function MarketplaceOptionsPanel({
    myProfile,
    onUpdateMyProfile,
    onSettingsChanged,
    onClose,
    hasSyncService,
    onNavigateToSyncSettings,
    className = '',
}: MarketplaceOptionsPanelProps) {
    const { t } = useTranslation();

    const [marketShowUnknown, setMarketShowUnknown] = useState<boolean>(true);
    const [maxStoreHops, setMaxStoreHops] = useState<number>(2);
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
                    setMaxStoreHops(s.maxStoreHops ?? 2);
                }
            })
            .catch((err) => {
                logger.error('Failed to load settings in MarketplaceOptionsPanel: ' + stringifyError(err));
            });
        return () => {
            isMounted = false;
        };
    }, []);

    const handleUpdateProfile = async (patch: { maxHops?: number; allowInquiries?: boolean }) => {
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
            await settingsService.saveSettings(updated);
            onSettingsChanged?.();
        } catch (err) {
            logger.error('Failed to save marketShowUnknown: ' + stringifyError(err));
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSaving(false);
        }
    };

    const handleSelectMaxStoreHops = async (hops: number) => {
        setMaxStoreHops(hops);
        try {
            setIsSaving(true);
            setErrorMsg(null);
            const current = await settingsService.getSettings();
            const updated: AppSettings = { ...current, maxStoreHops: hops };
            await settingsService.saveSettings(updated);
            onSettingsChanged?.();
        } catch (err) {
            logger.error('Failed to save maxStoreHops: ' + stringifyError(err));
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
                                {t('marketplace.optionsOutboundSection', 'Meine Visitenkarte & Erreichbarkeit')}
                            </h5>
                        </div>

                        {/* Reach of own business card */}
                        <div className="space-y-1.5">
                            <label className="text-xs font-bold text-theme-secondary block">
                                {t('marketplace.reachLabel', 'Reichweite deiner Visitenkarte')}
                            </label>
                            <p className="text-[11px] text-theme-light leading-relaxed">
                                {t('marketplace.maxHopsDesc', 'Bestimmt, wie weit Empfänger deine Visitenkarte an eigene Kontakte weiterempfehlen dürfen.')}
                            </p>
                            <div className="grid grid-cols-3 gap-2 pt-1">
                                {[
                                    { hops: 1, labelKey: 'marketplace.maxHops1Short' },
                                    { hops: 2, labelKey: 'marketplace.maxHops2Short' },
                                    { hops: 3, labelKey: 'marketplace.maxHops3Short' },
                                ].map(({ hops, labelKey }) => {
                                    const isSelected = (myProfile.maxHops || 2) === hops;
                                    return (
                                        <button
                                            key={hops}
                                            type="button"
                                            disabled={isSaving || isUpdatingProfile}
                                            onClick={() => handleUpdateProfile({ maxHops: hops })}
                                            className={`px-3 py-2 rounded-xl text-xs font-bold transition-all cursor-pointer disabled:opacity-50 text-center border ${
                                                isSelected
                                                    ? 'bg-theme-primary text-white border-theme-primary shadow-sm'
                                                    : 'bg-white border-theme-subtle text-theme-secondary hover:border-theme-primary/40'
                                            }`}
                                        >
                                            {t(labelKey)}
                                        </button>
                                    );
                                })}
                            </div>
                        </div>

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
                                    className="block text-[11px] text-theme-light font-normal cursor-pointer mt-0.5"
                                >
                                    {t('marketplace.allowInquiriesGeneralDesc', 'Ermöglicht anderen Kontakten, dir direkte Nachrichten oder Anfragen zu deinen Fähigkeiten zu senden.')}
                                </label>
                            </div>
                        </div>
                    </div>
                )}

                {/* Section 2: Marketplace Filter & Storage (Inbound) */}
                <div className="space-y-3" data-testid="marketplace-options-inbound-section">
                    <div className="flex items-center gap-2">
                        <h5 className="text-xs font-black uppercase tracking-wider text-theme-secondary">
                            {t('marketplace.optionsInboundSection', 'Marktplatz-Filter & Speicher')}
                        </h5>
                    </div>

                    {/* Marketplace Max Store Hops Reach */}
                    <div className="space-y-1.5">
                        <label className="text-xs font-bold text-theme-secondary block">
                            {t('settings.maxStoreHops')}
                        </label>
                        <p className="text-[11px] text-theme-light leading-relaxed">
                            {t('settings.maxStoreHopsDesc')}
                        </p>
                        <div className="grid grid-cols-3 gap-2 pt-1">
                            {[
                                { hops: 1, labelKey: 'settings.maxStoreHops1' },
                                { hops: 2, labelKey: 'settings.maxStoreHops2' },
                                { hops: 3, labelKey: 'settings.maxStoreHops3' },
                            ].map(({ hops, labelKey }) => {
                                const isSelected = maxStoreHops === hops;
                                return (
                                    <button
                                        key={hops}
                                        type="button"
                                        disabled={isSaving || isUpdatingProfile}
                                        onClick={() => handleSelectMaxStoreHops(hops)}
                                        className={`px-3 py-2 rounded-xl text-xs font-bold transition-all cursor-pointer disabled:opacity-50 text-center border ${
                                            isSelected
                                                ? 'bg-theme-primary text-white border-theme-primary shadow-sm'
                                                : 'bg-white border-theme-subtle text-theme-secondary hover:border-theme-primary/40'
                                        }`}
                                    >
                                        {t(labelKey)}
                                    </button>
                                );
                            })}
                        </div>
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
                                className="block text-[11px] text-theme-light font-normal cursor-pointer mt-0.5"
                            >
                                {t('settings.marketShowUnknownDesc')}
                            </label>
                        </div>
                    </div>
                </div>

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
