// src/components/SettingsView.tsx
import { useState, useEffect, FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { settingsService } from '../services/settingsService';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Card } from './ui/Card';
import { Input } from './ui/Input';
import { AppSettings, PrivacyDefault } from '../types';
import { useSession } from '../context/SessionContext';
import { translateError } from '../utils/errorHelper';

import { ProfileSettings } from './ProfileSettings';
import { PageLayout } from './ui/PageLayout';
import { authService } from '../services/authService';
import { LanguageSelector } from './ui/LanguageSelector';
import { 
    User, 
    Settings as SettingsIcon, 
    Shield, 
    Eye, 
    EyeOff, 
    Clock, 
    Database,
    Save,
    CheckCircle2,
    Fingerprint,
    Globe
} from 'lucide-react';

interface SettingsViewProps {
    onBack: () => void;
}

export function SettingsView({ onBack }: SettingsViewProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [localInstanceId, setLocalInstanceId] = useState('');
    const [isLoading, setIsLoading] = useState(true);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');
    const [activeTab, setActiveTab] = useState<'app' | 'profile'>('profile');

    useEffect(() => {
        async function fetchSettings() {
            try {
                logger.info("SettingsView: Fetching current settings.");
                const currentSettings = await settingsService.getSettings();
                setSettings(currentSettings);
                
                const deviceId = await authService.getLocalInstanceId();
                setLocalInstanceId(deviceId);
            } catch (e) {
                const msg = t('settings.loadFailed', { error: translateError(e, t) });
                logger.error(`Failed to load settings: ${e}`);
                setError(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchSettings();
    }, [t]);

    const handleSave = async (event: FormEvent) => {
        event.preventDefault();
        if (!settings) {
            setError(t('settings.fillAllFields'));
            return;
        }

        setIsSaving(true);
        setError('');
        setSuccess('');

        try {
            logger.info("Attempting to save settings...");
            await protectAction(async (password) => { await settingsService.saveSettings(settings, password || undefined); });
            setSuccess(t('settings.saveSuccess'));
            logger.info("Settings saved successfully.");
            setTimeout(() => setSuccess(''), 3000);
        } catch (e) {
            const msg = t('settings.saveFailed', { error: translateError(e, t) });
            logger.error(`Failed to save settings: ${e}`);
            setError(msg);
        } finally {
            setIsSaving(false);
        }
    };

    if (isLoading) {
        return <div className="py-20 text-center animate-pulse text-theme-light font-black uppercase tracking-[0.2em]">{t('settings.configuringEnvironment')}</div>;
    }

    return (
        <PageLayout 
            title={t('settings.systemPreferencesTitle')} 
            description={t('settings.systemPreferencesDescription')}
            onBack={onBack}
        >
            <div className="max-w-4xl mx-auto">
                {/* Tab Navigation */}
                <div className="flex bg-white/30 backdrop-blur-md p-1.5 rounded-2xl border border-theme-subtle mb-10 shadow-inner-soft">
                    <button 
                        onClick={() => setActiveTab('profile')}
                        className={`flex-1 flex items-center justify-center gap-2 py-3 rounded-xl text-xs font-black uppercase tracking-widest transition-all ${activeTab === 'profile' ? 'bg-white text-theme-primary shadow-premium' : 'text-theme-light hover:text-theme-secondary hover:bg-white/40'}`}
                    >
                        <User size={14} />
                        {t('settings.tabIdentityProfile')}
                    </button>
                    <button 
                        onClick={() => setActiveTab('app')}
                        className={`flex-1 flex items-center justify-center gap-2 py-3 rounded-xl text-xs font-black uppercase tracking-widest transition-all ${activeTab === 'app' ? 'bg-white text-theme-primary shadow-premium' : 'text-theme-light hover:text-theme-secondary hover:bg-white/40'}`}
                    >
                        <SettingsIcon size={14} />
                        {t('settings.tabSecurityOps')}
                    </button>
                </div>

                {activeTab === 'profile' ? (
                    <ProfileSettings />
                ) : (
                    <form onSubmit={handleSave} className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
                            {/* Storage & Data Retention */}
                            <Card header={
                                <div className="flex items-center gap-2">
                                    <Database size={18} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.dataStewardshipTitle')}</span>
                                </div>
                            }>
                                <div className="space-y-6">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('settings.historyStorageLabel')}</label>
                                        <Input
                                            type="number"
                                            value={settings?.bundleRetentionDays ?? 30}
                                            onChange={(e) => setSettings(s => s ? { ...s, bundleRetentionDays: parseInt(e.target.value, 10) } : null)}
                                            min="1"
                                            className="font-bold"
                                        />
                                        <p className="text-[10px] text-theme-light/60 font-medium leading-relaxed">
                                            {t('settings.historyStorageDescription')}
                                        </p>
                                    </div>
                                </div>
                            </Card>

                            {/* Session Control */}
                            <Card header={
                                <div className="flex items-center gap-2">
                                    <Clock size={18} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.activeSessionTitle')}</span>
                                </div>
                            }>
                                <div className="space-y-6">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('settings.timeoutIntervalLabel')}</label>
                                        <Input
                                            type="number"
                                            value={settings ? Math.floor((settings.sessionTimeoutSeconds || 0) / 60) : 10}
                                            onChange={(e) => {
                                                const minutes = parseInt(e.target.value, 10);
                                                setSettings(s => s ? { ...s, sessionTimeoutSeconds: isNaN(minutes) ? 0 : minutes * 60 } : null);
                                            }}
                                            min="0"
                                            className="font-bold"
                                        />
                                        <p className="text-[10px] text-theme-light/60 font-medium leading-relaxed">
                                            {t('settings.timeoutIntervalDescription')}
                                        </p>
                                    </div>
                                </div>
                            </Card>
                        </div>

                        {/* Privacy Protocols */}
                        <Card header={
                            <div className="flex items-center gap-2">
                                <Shield size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest">{t('settings.privacyProtocolsTitle')}</span>
                            </div>
                        }>
                            <div className="space-y-4">
                                <p className="text-xs text-theme-light font-medium mb-4">{t('settings.privacyProtocolsDescription')}</p>
                                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                                    {[
                                        { id: 'ask', label: t('settings.privacyModeInteractive'), icon: Fingerprint, desc: t('settings.privacyModeInteractiveDesc') },
                                        { id: 'stealth', label: t('settings.privacyModeStealth'), icon: EyeOff, desc: t('settings.privacyModeStealthDesc') },
                                        { id: 'public', label: t('settings.privacyModePublic'), icon: Eye, desc: t('settings.privacyModePublicDesc') }
                                    ].map((mode) => (
                                        <label 
                                            key={mode.id}
                                            className={`relative flex flex-col p-4 rounded-2xl border-2 transition-all cursor-pointer group ${settings?.privacyDefault === mode.id ? 'bg-theme-primary/5 border-theme-primary shadow-sm' : 'bg-white border-theme-subtle hover:border-theme-primary/30'}`}
                                        >
                                            <input
                                                type="radio"
                                                name="privacyDefault"
                                                value={mode.id}
                                                checked={settings?.privacyDefault === mode.id}
                                                onChange={(e) => setSettings(s => s ? { ...s, privacyDefault: e.target.value as PrivacyDefault } : null)}
                                                className="absolute opacity-0"
                                            />
                                            <mode.icon className={`mb-3 transition-colors ${settings?.privacyDefault === mode.id ? 'text-theme-primary' : 'text-theme-light group-hover:text-theme-secondary'}`} size={20} />
                                            <span className={`text-sm font-black tracking-tight ${settings?.privacyDefault === mode.id ? 'text-theme-primary' : 'text-theme-secondary'}`}>{mode.label}</span>
                                            <span className="text-[10px] font-medium text-theme-light mt-1">{mode.desc}</span>
                                            {settings?.privacyDefault === mode.id && (
                                                <div className="absolute top-3 right-3 text-theme-primary">
                                                    <CheckCircle2 size={16} />
                                                </div>
                                            )}
                                        </label>
                                    ))}
                                </div>
                            </div>
                        </Card>

                        {/* Language Selector */}
                        <Card 
                            className="!overflow-visible"
                            header={
                                <div className="flex items-center gap-2">
                                    <Globe size={18} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.languageTitle')}</span>
                                </div>
                            }
                        >
                            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                                <p className="text-xs text-theme-light font-medium">{t('settings.languageDescription')}</p>
                                <LanguageSelector variant="compact" />
                            </div>
                        </Card>

                        {/* Device Cryptographic Binding */}
                        <Card header={
                            <div className="flex items-center gap-2">
                                <Fingerprint size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest">{t('settings.deviceBindingTitle')}</span>
                            </div>
                        }>
                            <div className="space-y-4">
                                <div className="space-y-2">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">{t('settings.deviceBindingLabel')}</label>
                                    <div className="flex gap-2">
                                        <input
                                            type="text"
                                            readOnly
                                            value={localInstanceId || t('settings.retrievingId')}
                                            className="w-full bg-slate-50 border border-theme-subtle rounded-2xl px-5 py-4 text-xs font-mono text-theme-secondary select-all"
                                        />
                                        <Button 
                                            type="button" 
                                            onClick={() => {
                                                navigator.clipboard.writeText(localInstanceId);
                                            }}
                                            variant="outline"
                                            className="px-4 text-xs font-black uppercase tracking-widest"
                                        >
                                            {t('settings.copyButton')}
                                        </Button>
                                    </div>
                                    <p className="text-[10px] text-theme-light/60 font-medium leading-relaxed">
                                        {t('settings.deviceBindingDescription')}
                                    </p>
                                </div>
                            </div>
                        </Card>


                        <div className="pt-6 flex flex-col items-center gap-4">
                            <Button type="submit" disabled={isSaving} className="min-w-[240px] gap-2 rounded-2xl py-4 shadow-lg shadow-theme-primary/20">
                                {isSaving ? <Clock className="animate-spin" size={18} /> : <Save size={18} />}
                                {isSaving ? t('settings.saving') : t('settings.saveButton')}
                            </Button>
                            {error && <p className="text-sm font-bold text-rose-500 animate-bounce">{error}</p>}
                            {success && <p className="text-sm font-bold text-emerald-500 flex items-center gap-2"><CheckCircle2 size={16}/> {success}</p>}
                        </div>
                    </form>
                )}
            </div>
        </PageLayout>
    );
}
