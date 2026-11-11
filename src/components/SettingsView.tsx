// src/components/SettingsView.tsx
import { useState, useEffect, FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { settingsService } from '../services/settingsService';
import { authService } from '../services/authService';
import { profileService } from '../services/profileService';
import { useSession } from '../context/SessionContext';
import { useOptionalNavigation } from '../context/NavigationContext';
import { translateError, stringifyError } from '../utils/errorHelper';
import { logger } from '../utils/log';
import { tourBridge, SettingsTabId } from '../utils/tourBridge';

import { PageLayout } from './ui/PageLayout';
import { SegmentedTabs } from './ui/SegmentedTabs';
import { ProfileTab } from './settings/ProfileTab';
import { MarketplaceSettingsTab } from './settings/MarketplaceSettingsTab';
import { SecurityBackupTab } from './settings/SecurityBackupTab';
import { ConnectionTab } from './settings/ConnectionTab';
import { AdvancedTab } from './settings/AdvancedTab';
import { AppSettings } from '../types';

import { User, Store, Shield, Cloud, Settings as SettingsIcon } from 'lucide-react';

export type SettingsActiveTab = 'profile' | 'marketplace' | 'security' | 'connection' | 'advanced';

interface SettingsViewProps {
    onBack: () => void;
}

export function SettingsView({ onBack }: SettingsViewProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const nav = useOptionalNavigation();
    const appVersion = nav?.appVersion || '0.3.0';

    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [localInstanceId, setLocalInstanceId] = useState('');
    const [userId, setUserId] = useState('');
    const [isLoading, setIsLoading] = useState(true);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');

    const [activeTab, setActiveTab] = useState<SettingsActiveTab>('profile');
    const [initialOpenSection, setInitialOpenSection] = useState<'network' | 'standards' | undefined>(undefined);
    const [devClickCount, setDevClickCount] = useState(0);

    // Deep link section mapping
    useEffect(() => {
        if (nav?.appState.view === 'settings') {
            const state = nav.appState as {
                view: 'settings';
                initialSection?: 'sync' | 'backup' | 'network' | 'profile' | 'marketplace' | 'standards' | 'security' | 'connection' | 'advanced';
            };
            if (state.initialSection) {
                const sec = state.initialSection;
                if (sec === 'profile') {
                    setActiveTab('profile');
                } else if (sec === 'marketplace') {
                    setActiveTab('marketplace');
                } else if (sec === 'security' || sec === 'backup') {
                    setActiveTab('security');
                } else if (sec === 'connection' || sec === 'sync') {
                    setActiveTab('connection');
                } else if (sec === 'advanced' || sec === 'network' || sec === 'standards') {
                    setActiveTab('advanced');
                    if (sec === 'network') {
                        setInitialOpenSection('network');
                    } else if (sec === 'standards') {
                        setInitialOpenSection('standards');
                    }
                }
                setTimeout(() => {
                    const el = document.getElementById(`settings-${sec}-section`);
                    if (el && typeof el.scrollIntoView === 'function') {
                        el.scrollIntoView({ behavior: 'smooth', block: 'center' });
                    }
                }, 100);
            }
        }
    }, [nav?.appState]);

    // Tour bridge tab registration
    useEffect(() => {
        tourBridge.registerSettingsTabSetter((tab: SettingsTabId) => {
            if (tab === 'profile') {
                setActiveTab('profile');
            } else if (tab === 'marketplace') {
                setActiveTab('marketplace');
            } else if (tab === 'security') {
                setActiveTab('security');
            } else if (tab === 'connection') {
                setActiveTab('connection');
            } else if (tab === 'advanced') {
                setActiveTab('advanced');
            }
        });
        return () => {
            tourBridge.registerSettingsTabSetter(null);
        };
    }, []);

    useEffect(() => {
        async function fetchSettings() {
            try {
                logger.info("SettingsView: Fetching current settings.");
                const currentSettings = await settingsService.getSettings();
                setSettings(currentSettings);
                
                const deviceId = await authService.getLocalInstanceId().catch(() => '');
                setLocalInstanceId(deviceId);

                const currentUserId = await profileService.getUserId().catch(() => '');
                setUserId(currentUserId);
            } catch (e) {
                const msg = t('settings.loadFailed', { error: translateError(e, t) });
                logger.error(`Failed to load settings: ${stringifyError(e)}`);
                setError(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchSettings();
    }, [t]);

    const handleDevModeClick = () => {
        if (settings?.devModeUnlocked) return;
        const next = devClickCount + 1;
        setDevClickCount(next);
        if (next >= 7) {
            setSettings(s => s ? { ...s, devModeUnlocked: true } : s);
            logger.info("SettingsView: Dev mode unlocked via 7 clicks.");
            setSuccess(t('settings.devModeUnlockedToast'));
            setTimeout(() => setSuccess(''), 3000);
        }
    };

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
            await protectAction(async (password) => {
                await settingsService.saveSettings(settings, password || undefined);
            });
            setSuccess(t('settings.saveSuccess'));
            logger.info("Settings saved successfully.");
            setTimeout(() => setSuccess(''), 3000);
        } catch (e) {
            const msg = t('settings.saveFailed', { error: translateError(e, t) });
            logger.error(`Failed to save settings: ${stringifyError(e)}`);
            setError(msg);
        } finally {
            setIsSaving(false);
        }
    };

    const isDevUnlocked = !!settings?.devModeUnlocked;

    if (isLoading) {
        return (
            <div className="py-20 text-center animate-pulse text-theme-light font-black uppercase tracking-[0.2em]">
                {t('settings.configuringEnvironment')}
            </div>
        );
    }

    return (
        <PageLayout 
            title={
                <span 
                    onClick={handleDevModeClick} 
                    className="select-none cursor-pointer"
                    title={isDevUnlocked ? t('settings.devModeUnlockedToast') : `Klick ${7 - devClickCount} bis Entwicklermodus`}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); handleDevModeClick(); }}}
                >
                    {t('settings.systemPreferencesTitle')}
                </span>
            } 
            description={t('settings.systemPreferencesDescription')}
            onBack={onBack}
        >
            <div className="max-w-4xl mx-auto">
                {/* Tab Navigation */}
                <SegmentedTabs<SettingsActiveTab>
                    fullWidth
                    className="mb-10"
                    activeTab={activeTab}
                    onChange={setActiveTab}
                    tabs={[
                        { id: 'profile', label: t('settings.tabProfile'), icon: <User size={14} /> },
                        { id: 'marketplace', label: t('settings.tabMarketplace'), icon: <Store size={14} /> },
                        { id: 'security', label: t('settings.tabSecurityBackup'), icon: <Shield size={14} /> },
                        { id: 'connection', label: t('settings.tabConnection'), icon: <Cloud size={14} /> },
                        { id: 'advanced', label: t('settings.tabAdvanced'), icon: <SettingsIcon size={14} /> }
                    ]}
                />

                {activeTab === 'profile' && (
                    <ProfileTab 
                        userId={userId} 
                        settings={settings}
                        onSettingsUpdated={setSettings}
                    />
                )}

                {activeTab === 'marketplace' && (
                    <MarketplaceSettingsTab
                        settings={settings}
                        onSettingsUpdated={setSettings}
                    />
                )}

                {activeTab === 'security' && (
                    <SecurityBackupTab
                        settings={settings}
                        onSettingsUpdated={setSettings}
                        onSave={handleSave}
                        isSaving={isSaving}
                        saveError={error}
                        saveSuccess={success}
                    />
                )}

                {activeTab === 'connection' && (
                    <ConnectionTab
                        settings={settings}
                        onSettingsUpdated={setSettings}
                    />
                )}

                {activeTab === 'advanced' && (
                    <AdvancedTab
                        settings={settings}
                        onSettingsUpdated={setSettings}
                        onSave={handleSave}
                        isSaving={isSaving}
                        saveError={error}
                        saveSuccess={success}
                        localInstanceId={localInstanceId}
                        appVersion={appVersion}
                        isDevUnlocked={isDevUnlocked}
                        onDevClick={handleDevModeClick}
                        devClickCount={devClickCount}
                        initialOpenSection={initialOpenSection}
                    />
                )}
            </div>
        </PageLayout>
    );
}

export default SettingsView;
