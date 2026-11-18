// src/components/SettingsView.tsx
import { useState, useEffect, useRef, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { settingsService } from '../../services/settingsService';
import { authService } from '../../services/authService';
import { useSession } from '../../context/SessionContext';
import { useOptionalNavigation } from '../../context/NavigationContext';
import { translateError, stringifyError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import { tourBridge, SettingsTabId } from '../../utils/tourBridge';
import { SettingsSection } from '../../types';

import { PageLayout } from '../ui/PageLayout';
import { ExpertTabsLayout } from '../ui/ExpertTabsLayout';
import { SegmentedTabs } from '../ui/SegmentedTabs';
import { AlertBanner } from '../ui/AlertBanner';
import { Card } from '../ui/Card';
import { LanguageSelector } from '../ui/LanguageSelector';
import { MyProfileCard } from '../profile/MyProfileCard';
import { InfoHint } from '../ui/InfoHint';
import { BackupSettingsCard } from './BackupSettingsCard';
import { ReachPresetSelector } from './ReachPresetSelector';
import { ProfileTab } from './ProfileTab';
import { SecurityBackupTab } from './SecurityBackupTab';
import { ConnectionTab } from './ConnectionTab';
import { AdvancedTab } from './AdvancedTab';
import { IdentityRecoveryCard } from './IdentityRecoveryCard';
import { AutoSyncSummaryCard } from './AutoSyncSummaryCard';
import { ViewAndLockCard } from './ViewAndLockCard';
import { DangerZoneCard } from './DangerZoneCard';
import { NetworkProtectionCard } from './NetworkProtectionCard';
import { profileService } from '../../services/profileService';
import { AppSettings } from '../../types';

import { User, Shield, Cloud, Settings as SettingsIcon, Radio } from 'lucide-react';

export type SettingsActiveTab = 'profile' | 'security' | 'connection' | 'advanced';

interface SettingsViewProps {
    onBack: () => void;
}

export function SettingsView({ onBack }: SettingsViewProps) {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const nav = useOptionalNavigation();
    const appVersion = nav?.appVersion || '0.3.0';

    const [settings, setSettings] = useState<AppSettings | null>(null);
    const settingsRef = useRef<AppSettings | null>(null);
    const [localInstanceId, setLocalInstanceId] = useState('');
    const [userId, setUserId] = useState('');
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState('');
    const [devFeedback, setDevFeedback] = useState('');

    const [activeTab, setActiveTab] = useState<SettingsActiveTab>('profile');
    const [initialOpenSection, setInitialOpenSection] = useState<'network' | 'standards' | undefined>(
        undefined
    );
    const [devClickCount, setDevClickCount] = useState(0);

    const updateSettingsState = useCallback(
        (updater: AppSettings | ((prev: AppSettings | null) => AppSettings | null)) => {
            const next =
                typeof updater === 'function' ? updater(settingsRef.current) : updater;
            settingsRef.current = next;
            setSettings(next);
            return next;
        },
        []
    );

    const persistSettingsUpdate = useCallback(
        (updater: AppSettings | ((prev: AppSettings | null) => AppSettings | null)) => {
            const next = updateSettingsState(updater);
            if (!next) return;
            setError('');
            void (async () => {
                try {
                    await protectAction(async (password) => {
                        await settingsService.saveSettings(next, password || undefined);
                    });
                } catch (e) {
                    const msg = t('settings.saveFailed', { error: translateError(e, t) });
                    logger.error(`Failed to save settings: ${stringifyError(e)}`);
                    setError(msg);
                }
            })();
        },
        [protectAction, t, updateSettingsState]
    );

    // Deep link section mapping
    useEffect(() => {
        if (nav?.appState.view === 'settings') {
            const state = nav.appState as {
                view: 'settings';
                initialSection?: SettingsSection;
            };
            if (state.initialSection) {
                const sec = state.initialSection;
                if (sec === 'profile') {
                    setActiveTab('profile');
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
                if (!isLoading) {
                    const timer = setTimeout(() => {
                        if (typeof document !== 'undefined') {
                            const el = document.getElementById(`settings-${sec}-section`);
                            if (el && typeof el.scrollIntoView === 'function') {
                                el.scrollIntoView({ behavior: 'smooth', block: 'center' });
                            }
                        }
                    }, 100);
                    return () => clearTimeout(timer);
                }
            }
        }
    }, [nav?.appState, isLoading]);

    // Tour bridge tab registration
    useEffect(() => {
        tourBridge.registerSettingsTabSetter((tab: SettingsTabId) => {
            if (tab === 'profile') {
                setActiveTab('profile');
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

    // Reset feedback when active tab changes
    useEffect(() => {
        setError('');
        setDevFeedback('');
    }, [activeTab]);

    useEffect(() => {
        let active = true;
        async function fetchSettings() {
            try {
                logger.info('SettingsView: Fetching current settings.');
                const currentSettings = await settingsService.getSettings();
                if (!active) return;
                updateSettingsState(currentSettings);

                const deviceId = await authService.getLocalInstanceId().catch(() => '');
                if (!active) return;
                setLocalInstanceId(deviceId);

                const currentUserId = await profileService.getUserId().catch(() => '');
                if (!active) return;
                setUserId(currentUserId);
            } catch (e) {
                if (active) {
                    const msg = t('settings.loadFailed', { error: translateError(e, t) });
                    logger.error(`Failed to load settings: ${stringifyError(e)}`);
                    setError(msg);
                }
            } finally {
                if (active) setIsLoading(false);
            }
        }
        fetchSettings();
        return () => {
            active = false;
        };
    }, [t, updateSettingsState]);

    const handleDevModeClick = () => {
        if (settingsRef.current?.devModeUnlocked) return;
        const nextCount = devClickCount + 1;
        setDevClickCount(nextCount);
        if (nextCount >= 7) {
            persistSettingsUpdate((s) => (s ? { ...s, devModeUnlocked: true } : s));
            logger.info('SettingsView: Dev mode unlocked via 7 clicks.');
            setDevFeedback(t('settings.devModeUnlockedToast'));
            setTimeout(() => setDevFeedback(''), 3000);
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

    const simpleBlocksContent = (
        <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Block 1: Profil & Visitenkarte */}
            <div id="settings-profile-section" className="space-y-6">
                <Card
                    header={
                        <div className="flex items-center gap-2">
                            <User size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest">
                                {t('settings.profileAndCardTitle')}
                            </span>
                        </div>
                    }
                >
                    <div className="space-y-4">
                        <MyProfileCard compact showSkills={false} />
                        <div className="pt-2 border-t border-theme-subtle/50">
                            <h5 className="text-xs font-bold text-theme-secondary mb-2.5">
                                {t('settings.languageTitle')}
                            </h5>
                            <LanguageSelector />
                        </div>
                    </div>
                </Card>
            </div>

            {/* Block 2: Datensicherung & Wiederherstellung */}
            <div id="settings-security-section" className="space-y-6">
                <div id="settings-backup-section">
                    <BackupSettingsCard
                        settings={settings}
                        onSettingsUpdated={updateSettingsState}
                    />
                </div>
                <IdentityRecoveryCard />
            </div>

            {/* Block 3: Sichtbarkeit (3 Stufen) */}
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

            {/* Block 4: Abgleich (SyncSummaryCard) */}
            <div id="settings-sync-section">
                <div id="settings-connection-section">
                    <AutoSyncSummaryCard
                        settings={settings}
                        onPersistSettings={persistSettingsUpdate}
                        onSettingsSynced={updateSettingsState}
                    />
                </div>
            </div>

            {/* Block 5: Ansicht & Automatische Sperre */}
            <div id="settings-view-section">
                <ViewAndLockCard
                    settings={settings}
                    onPersistSettings={persistSettingsUpdate}
                />
            </div>

            {initialOpenSection === 'network' && (
                <NetworkProtectionCard
                    settings={settings}
                    onSettingsUpdated={updateSettingsState}
                    isDevUnlocked={isDevUnlocked}
                    initialOpen={true}
                />
            )}

            {/* Block 6: Gefahrenzone */}
            <div id="settings-danger-section">
                <DangerZoneCard />
            </div>
        </div>
    );

    const expertTabsContent = (
        <div>
            <SegmentedTabs<SettingsActiveTab>
                fullWidth
                className="mb-10"
                activeTab={activeTab}
                onChange={setActiveTab}
                tabs={[
                    { id: 'profile', label: t('settings.tabProfile'), icon: <User size={14} /> },
                    { id: 'security', label: t('settings.tabSecurityBackup'), icon: <Shield size={14} /> },
                    { id: 'connection', label: t('settings.tabConnection'), icon: <Cloud size={14} /> },
                    { id: 'advanced', label: t('settings.tabAdvanced'), icon: <SettingsIcon size={14} /> },
                ]}
            />

            {activeTab === 'profile' && (
                <ProfileTab
                    userId={userId}
                    settings={settings}
                    onSettingsUpdated={updateSettingsState}
                />
            )}

            {activeTab === 'security' && (
                <SecurityBackupTab
                    settings={settings}
                    onPersistSettings={persistSettingsUpdate}
                    onSettingsSynced={updateSettingsState}
                />
            )}

            {activeTab === 'connection' && (
                <ConnectionTab
                    settings={settings}
                    onPersistSettings={persistSettingsUpdate}
                    onSettingsSynced={updateSettingsState}
                />
            )}

            {activeTab === 'advanced' && (
                <AdvancedTab
                    settings={settings}
                    onSettingsUpdated={persistSettingsUpdate}
                    localInstanceId={localInstanceId}
                    appVersion={appVersion}
                    isDevUnlocked={isDevUnlocked}
                    onDevClick={handleDevModeClick}
                    devClickCount={devClickCount}
                    initialOpenSection={initialOpenSection}
                />
            )}
        </div>
    );

    return (
        <PageLayout
            title={
                <span
                    onClick={handleDevModeClick}
                    className="select-none cursor-pointer"
                    title={
                        isDevUnlocked
                            ? t('settings.devModeUnlockedToast')
                            : `Klick ${7 - devClickCount} bis Entwicklermodus`
                    }
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            handleDevModeClick();
                        }
                    }}
                >
                    {t('settings.systemPreferencesTitle')}
                </span>
            }
            description={t('settings.systemPreferencesDescription')}
            onBack={onBack}
        >
            <div className="max-w-4xl mx-auto space-y-6">
                {error && <AlertBanner variant="error">{error}</AlertBanner>}
                {devFeedback && <AlertBanner variant="success">{devFeedback}</AlertBanner>}

                <ExpertTabsLayout
                    simpleContent={simpleBlocksContent}
                    expertContent={expertTabsContent}
                />
            </div>
        </PageLayout>
    );
}
