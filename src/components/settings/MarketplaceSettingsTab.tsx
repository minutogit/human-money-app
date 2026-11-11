// src/components/settings/MarketplaceSettingsTab.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Store } from 'lucide-react';
import { Card } from '../ui/Card';
import { AppSettings } from '../../types';
import { settingsService } from '../../services/settingsService';
import { logger } from '../../utils/log';
import { stringifyError, translateError } from '../../utils/errorHelper';

export interface MarketplaceSettingsTabProps {
    settings: AppSettings | null;
    onSettingsUpdated?: (newSettings: AppSettings) => void;
}

export const MarketplaceSettingsTab: React.FC<MarketplaceSettingsTabProps> = ({
    settings,
    onSettingsUpdated,
}) => {
    const { t } = useTranslation();

    const [marketShowUnknown, setMarketShowUnknown] = useState<boolean>(settings?.marketShowUnknown ?? true);
    const [maxStoreHops, setMaxStoreHops] = useState<number>(settings?.maxStoreHops ?? 2);
    const [errorMsg, setErrorMsg] = useState<string | null>(null);

    useEffect(() => {
        if (settings) {
            setMarketShowUnknown(settings.marketShowUnknown ?? true);
            setMaxStoreHops(settings.maxStoreHops ?? 2);
        }
    }, [settings]);

    const handleSaveToggle = async (key: keyof AppSettings, value: boolean) => {
        try {
            setErrorMsg(null);
            const current = settings || await settingsService.getSettings();
            const updated: AppSettings = { ...current, [key]: value };
            await settingsService.saveSettings(updated);
            if (onSettingsUpdated) onSettingsUpdated(updated);
        } catch (err) {
            logger.error(`Failed to save marketplace setting ${key}: ${stringifyError(err)}`);
            setErrorMsg(translateError(err, t));
        }
    };

    const handleSaveNumber = async (key: keyof AppSettings, value: number) => {
        try {
            setErrorMsg(null);
            const current = settings || await settingsService.getSettings();
            const updated: AppSettings = { ...current, [key]: value };
            await settingsService.saveSettings(updated);
            if (onSettingsUpdated) onSettingsUpdated(updated);
        } catch (err) {
            logger.error(`Failed to save marketplace setting ${key}: ${stringifyError(err)}`);
            setErrorMsg(translateError(err, t));
        }
    };

    return (
        <div id="settings-marketplace-section" className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
            <Card header={
                <div className="flex items-center gap-2">
                    <Store size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.tabMarketplace')}</span>
                </div>
            }>
                <div className="space-y-5">
                    {errorMsg && (
                        <div className="p-3 bg-rose-50 border border-rose-200 text-rose-700 text-xs rounded-xl flex items-center justify-between">
                            <span>{errorMsg}</span>
                            <button type="button" onClick={() => setErrorMsg(null)} className="text-rose-500 hover:text-rose-700 font-bold ml-2">×</button>
                        </div>
                    )}
                    {/* Marketplace Show Unknown Creators Toggle */}
                    <div className="flex items-start gap-3 p-3 bg-white dark:bg-slate-900 border border-theme-subtle rounded-xl hover:border-theme-primary/40 transition-colors">
                        <input
                            type="checkbox"
                            id="tabMarketShowUnknown"
                            checked={marketShowUnknown}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setMarketShowUnknown(val);
                                handleSaveToggle('marketShowUnknown', val);
                            }}
                            className="mt-0.5 accent-theme-primary cursor-pointer"
                        />
                        <div className="flex-1">
                            <label htmlFor="tabMarketShowUnknown" className="text-xs font-medium text-theme-secondary leading-relaxed cursor-pointer">
                                {t('settings.marketShowUnknown')}
                            </label>
                            <label htmlFor="tabMarketShowUnknown" className="block text-[10px] text-theme-light/60 font-normal cursor-pointer">
                                {t('settings.marketShowUnknownDesc')}
                            </label>
                        </div>
                    </div>

                    {/* Marketplace Max Store Hops (Reichweite für empfangene Visitenkarten) */}
                    <div className="flex flex-col gap-1.5 p-3 bg-white dark:bg-slate-900 border border-theme-subtle rounded-xl hover:border-theme-primary/40 transition-colors">
                        <div className="flex items-center justify-between gap-3">
                            <label htmlFor="tabMaxStoreHops" className="text-xs font-medium text-theme-secondary cursor-pointer">
                                {t('settings.maxStoreHops')}
                            </label>
                            <select
                                id="tabMaxStoreHops"
                                value={maxStoreHops}
                                onChange={(e) => {
                                    const val = Number(e.target.value);
                                    setMaxStoreHops(val);
                                    handleSaveNumber('maxStoreHops', val);
                                }}
                                className="h-8 px-2.5 bg-bg-app border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20 cursor-pointer"
                            >
                                <option value={1}>{t('settings.maxStoreHops1')}</option>
                                <option value={2}>{t('settings.maxStoreHops2')}</option>
                                <option value={3}>{t('settings.maxStoreHops3')}</option>
                            </select>
                        </div>
                        <label htmlFor="tabMaxStoreHops" className="block text-[10px] text-theme-light/60 font-normal cursor-pointer">
                            {t('settings.maxStoreHopsDesc')}
                        </label>
                    </div>
                </div>
            </Card>
        </div>
    );
};

export default MarketplaceSettingsTab;
