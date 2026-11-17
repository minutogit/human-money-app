// src/components/settings/ProfileTab.tsx
import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Fingerprint, Globe, User, SlidersHorizontal, Radio } from 'lucide-react';
import { Card } from '../ui/Card';
import { LanguageSelector } from '../ui/LanguageSelector';
import { UiModeToggle } from '../ui/UiModeToggle';
import { InfoHint } from '../ui/InfoHint';
import { TechId } from '../ui/TechId';
import { ReachPresetSelector } from '../ReachPresetSelector';
import { MyProfileCard } from '../profile/MyProfileCard';
import { useSession } from '../../context/SessionContext';
import { AppSettings } from '../../types';
import { settingsService } from '../../services/settingsService';
import { logger } from '../../utils/log';
import { stringifyError, translateError } from '../../utils/errorHelper';

export interface ProfileTabProps {
    userId: string;
    settings?: AppSettings | null;
    onSettingsUpdated?: (newSettings: AppSettings) => void;
}

export const ProfileTab: React.FC<ProfileTabProps> = ({ 
    userId,
    settings,
    onSettingsUpdated,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [marketSendCardOnPayment, setMarketSendCardOnPayment] = useState<boolean>(settings?.marketSendCardOnPayment ?? false);
    const [errorMsg, setErrorMsg] = useState<string | null>(null);

    useEffect(() => {
        if (settings) {
            setMarketSendCardOnPayment(settings.marketSendCardOnPayment ?? false);
        }
    }, [settings]);

    const handleSaveSendCardToggle = async (val: boolean) => {
        const prev = marketSendCardOnPayment;
        setMarketSendCardOnPayment(val);
        setErrorMsg(null);
        try {
            const current = settings || await settingsService.getSettings();
            const updated: AppSettings = { ...current, marketSendCardOnPayment: val };
            await protectAction(async (pwd) => {
                await settingsService.saveSettings(updated, pwd || undefined);
            });
            if (onSettingsUpdated) onSettingsUpdated(updated);
        } catch (err) {
            logger.error(`Failed to save marketSendCardOnPayment: ${stringifyError(err)}`);
            setMarketSendCardOnPayment(prev);
            setErrorMsg(translateError(err, t));
        }
    };

    return (
        <div id="settings-profile-section" className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Benutzer-ID */}
            <Card header={
                <div className="flex items-center justify-between w-full">
                    <div className="flex items-center gap-2">
                        <Fingerprint size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">{t('profile.userIdTitle')}</span>
                    </div>
                </div>
            }>
                <div className="space-y-3">
                    <InfoHint topic="benutzerId" />
                    <TechId value={userId} kind="did" className="py-1.5 px-3" />
                </div>
            </Card>

            {/* Sprache */}
            <Card header={
                <div className="flex items-center gap-2">
                    <Globe size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.languageTitle')}</span>
                </div>
            }>
                <LanguageSelector />
            </Card>

            {/* Ansicht (Einfach / Erweitert) */}
            <Card header={
                <div className="flex items-center gap-2">
                    <SlidersHorizontal size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.viewModeTitle')}</span>
                </div>
            }>
                <div className="space-y-3">
                    <p className="text-xs text-theme-light leading-relaxed">
                        {t('settings.viewModeSubtitle')}
                    </p>
                    <UiModeToggle variant="full" />
                </div>
            </Card>

            {/* Profil & Visitenkarte */}
            <Card header={
                <div className="flex items-center gap-2">
                    <User size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.profileAndCardTitle')}</span>
                </div>
            }>
                <div className="space-y-4">
                    <MyProfileCard compact showSkills={false} />

                    {/* Marketplace Send Card On Payment Toggle */}
                    {errorMsg && (
                        <div className="p-3 bg-rose-50 border border-rose-200 text-rose-700 text-xs rounded-xl flex items-center justify-between">
                            <span>{errorMsg}</span>
                            <button type="button" onClick={() => setErrorMsg(null)} className="text-rose-500 hover:text-rose-700 font-bold ml-2">×</button>
                        </div>
                    )}
                    <div className="flex items-start gap-3 p-3 bg-white dark:bg-slate-900 border border-theme-subtle rounded-xl hover:border-theme-primary/40 transition-colors pt-3">
                        <input
                            type="checkbox"
                            id="profileSendCardOnPayment"
                            checked={marketSendCardOnPayment}
                            onChange={(e) => handleSaveSendCardToggle(e.target.checked)}
                            className="mt-0.5 accent-theme-primary cursor-pointer"
                        />
                        <div className="flex-1">
                            <label htmlFor="profileSendCardOnPayment" className="text-xs font-medium text-theme-secondary leading-relaxed cursor-pointer">
                                {t('settings.marketSendCardOnPayment')}
                            </label>
                            <label htmlFor="profileSendCardOnPayment" className="block text-[10px] text-theme-light/60 font-normal cursor-pointer">
                                {t('settings.marketSendCardOnPaymentDesc')}
                            </label>
                            <p className="mt-1 text-[10px] text-amber-700 bg-amber-50/70 p-2 rounded-lg border border-amber-200/60 leading-relaxed">
                                {t('settings.marketSendCardOnPaymentPrivacyNote')}
                            </p>
                        </div>
                    </div>
                </div>
            </Card>

            {/* Sichtbarkeit & Reichweite */}
            <div id="settings-reach-section">
                <Card
                    header={
                        <div className="flex items-center justify-between w-full">
                            <div className="flex items-center gap-2">
                                <Radio size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest">
                                    {t('settings.reachTitle')}
                                </span>
                            </div>
                            <InfoHint topic="visibilityReach" />
                        </div>
                    }
                >
                    <p className="text-xs text-theme-light leading-relaxed mb-4">
                        {t('settings.reachSubtitle')}
                    </p>
                    <ReachPresetSelector protectAction={protectAction} showHeader={false} />
                </Card>
            </div>
        </div>
    );
};

export default ProfileTab;
