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
import { Disclosure } from '../ui/Disclosure';
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
import { QuickPinCard } from './QuickPinCard';
import { AutoSyncSummaryCard } from './AutoSyncSummaryCard';
import { SyncServiceSettingsCard } from './SyncServiceSettingsCard';
import { StandardsManager } from '../standards/StandardsManager';
import { ViewAndLockCard } from './ViewAndLockCard';
import { DangerZoneCard } from './DangerZoneCard';
import { NetworkProtectionCard } from './NetworkProtectionCard';
import { profileService } from '../../services/profileService';
import { AppConfigDto, SecurityPolicyPatch, UiPreferencesPatch } from '../../types';

import { User, Shield, ShieldCheck, Cloud, Settings as SettingsIcon, Radio } from 'lucide-react';

export type SettingsActiveTab = 'profile' | 'security' | 'connection' | 'advanced';

interface SettingsViewProps {
    onBack: () => void;
}

export function SettingsView({ onBack }: SettingsViewProps) {
    const { t } = useTranslation();
    const { protectAction, lockSession, refreshSessionStatus } = useSession();
    const nav = useOptionalNavigation();
    const appVersion = nav?.appVersion || '0.3.0';

    const [config, setConfig] = useState<AppConfigDto | null>(null);
    const configRef = useRef<AppConfigDto | null>(null);
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

    const updateConfigState = useCallback(
        (updater: AppConfigDto | ((prev: AppConfigDto | null) => AppConfigDto | null)) => {
            const next =
                typeof updater === 'function' ? updater(configRef.current) : updater;
            configRef.current = next;
            setConfig(next);
            return next;
        },
        []
    );

    const handlePatchUiPreferences = useCallback(
        async (patch: UiPreferencesPatch) => {
            const prevConfig = configRef.current;
            if (prevConfig) {
                updateConfigState({
                    ...prevConfig,
                    uiPreferences: { ...prevConfig.uiPreferences, ...patch },
                });
            }
            setError('');
            try {
                const updatedUi = await protectAction(async (password) => {
                    return await settingsService.patchUiPreferences(patch, password || undefined);
                });
                if (updatedUi) {
                    updateConfigState((cur) => (cur ? { ...cur, uiPreferences: updatedUi } : cur));
                }
            } catch (e) {
                configRef.current = prevConfig;
                setConfig(prevConfig);
                const msg = t('settings.saveFailed', { error: translateError(e, t) });
                logger.error(`Failed to save settings: ${stringifyError(e)}`);
                setError(msg);
            }
        },
        [protectAction, t, updateConfigState]
    );

    const handlePatchSecurityPolicy = useCallback(
        async (patch: SecurityPolicyPatch): Promise<boolean> => {
            const prevConfig = configRef.current;
            setError('');
            try {
                const updatedPolicy = await protectAction(
                    async (password) => {
                        return await settingsService.patchSecurityPolicy(
                            patch,
                            password || undefined,
                        );
                    },
                    { requireFullAccess: true },
                );
                if (!updatedPolicy) {
                    return false;
                }
                updateConfigState((cur) =>
                    cur
                        ? { ...cur, securityPolicy: updatedPolicy }
                        : cur,
                );
                if (updatedPolicy.sessionTimeoutSeconds === 0) {
                    await lockSession?.();
                }
                if ('sessionTimeoutSeconds' in patch) {
                    await refreshSessionStatus?.();
                }
                return true;
            } catch (e) {
                configRef.current = prevConfig;
                setConfig(prevConfig);
                const msg = t('settings.saveFailed', { error: translateError(e, t) });
                logger.error(`Failed to save settings: ${stringifyError(e)}`);
                setError(msg);
                return false;
            }
        },
        [lockSession, protectAction, refreshSessionStatus, t, updateConfigState]
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
                logger.info('SettingsView: Fetching current config.');
                const currentConfig = await settingsService.getConfig();
                if (!active) return;
                updateConfigState(currentConfig);

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
    }, [t, updateConfigState]);

    const handleDevModeClick = () => {
        if (configRef.current?.securityPolicy.devModeUnlocked) return;
        const nextCount = devClickCount + 1;
        setDevClickCount(nextCount);
        if (nextCount >= 7) {
            void (async () => {
                const ok = await handlePatchSecurityPolicy({ devModeUnlocked: true });
                if (ok) {
                    logger.info('SettingsView: Dev mode unlocked via 7 clicks.');
                    setDevFeedback(t('settings.devModeUnlockedToast'));
                    setTimeout(() => setDevFeedback(''), 3000);
                }
            })();
        }
    };

    const isDevUnlocked = !!config?.securityPolicy.devModeUnlocked;

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
                        config={config}
                        onConfigUpdated={updateConfigState}
                    />
                </div>
                <IdentityRecoveryCard />
                <QuickPinCard />
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
            <div id="settings-sync-section" className="space-y-4">
                <div id="settings-connection-section">
                    <AutoSyncSummaryCard
                        config={config}
                        onPatchUiPreferences={handlePatchUiPreferences}
                        onConfigUpdated={updateConfigState}
                    />
                </div>
                <Disclosure
                    title={t('settings.manageSyncEndpointsToggle')}
                    defaultOpen={false}
                    toggleTestId="simple-manage-syncs-toggle"
                >
                    <SyncServiceSettingsCard
                        config={config}
                        onConfigUpdated={updateConfigState}
                    />
                </Disclosure>
            </div>

            {/* Block 5: Ansicht & Automatische Sperre */}
            <div id="settings-view-section">
                <ViewAndLockCard
                    config={config}
                    onPatchSecurityPolicy={handlePatchSecurityPolicy}
                />
            </div>

            {initialOpenSection === 'network' && (
                <NetworkProtectionCard
                    config={config}
                    onConfigUpdated={updateConfigState}
                    isDevUnlocked={isDevUnlocked}
                    initialOpen={true}
                />
            )}

            {initialOpenSection === 'standards' && (
                <div id="settings-standards-section">
                    <Card
                        header={
                            <div className="flex items-center gap-2">
                                <ShieldCheck size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest">
                                    {t('standards.title')}
                                </span>
                            </div>
                        }
                    >
                        <StandardsManager />
                    </Card>
                </div>
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
                    config={config}
                    onConfigUpdated={updateConfigState}
                />
            )}

            {activeTab === 'security' && (
                <SecurityBackupTab
                    config={config}
                    onPatchSecurityPolicy={handlePatchSecurityPolicy}
                    onConfigUpdated={updateConfigState}
                />
            )}

            {activeTab === 'connection' && (
                <ConnectionTab
                    config={config}
                    onPatchUiPreferences={handlePatchUiPreferences}
                    onConfigUpdated={updateConfigState}
                />
            )}

            {activeTab === 'advanced' && (
                <AdvancedTab
                    config={config}
                    onConfigUpdated={updateConfigState}
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
                            : t('settings.devModeClicksRemaining', {
                                  count: Math.max(0, 7 - devClickCount),
                                  defaultValue: `Klick ${Math.max(0, 7 - devClickCount)} bis Entwicklermodus`,
                              })
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
