// src/components/settings/SyncServiceSettingsCard.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Cloud,
    Server,
    Users,
    Globe,
    RefreshCw,
    CheckCircle2,
    AlertTriangle,
    Trash2,
    Copy,
    Check,
    Shield,
    HardDrive,
    KeyRound,
    FileArchive,
    HelpCircle,
} from 'lucide-react';
import { save } from '@tauri-apps/plugin-dialog';
import {
    AppSettings,
    FrontendDiscoveredSync,
    FrontendSyncAdminStatus,
    FrontendMemberPolicyDto,
    StoredPublicSync,
} from '../../types';
import { marketService } from '../../services/marketService';
import { settingsService } from '../../services/settingsService';
import { useSession } from '../../context/SessionContext';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { SegmentedTabs } from '../ui/SegmentedTabs';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { InfoHint } from '../ui/InfoHint';
import { copyToClipboard } from '../../utils/clipboard';
import { notifySyncStatusUpdated } from '../../utils/syncEvents';
import { translateError } from '../../utils/errorHelper';
import { useRateLimitCooldown } from '../../hooks/useRateLimitCooldown';

interface PolicyHelpPopoverProps {
    title: string;
    content: string;
}

const PolicyHelpPopover: React.FC<PolicyHelpPopoverProps> = ({ title, content }) => {
    const [isOpen, setIsOpen] = useState(false);
    return (
        <span className="relative inline-flex items-center ml-1">
            <button
                type="button"
                onClick={() => setIsOpen(!isOpen)}
                onMouseEnter={() => setIsOpen(true)}
                onMouseLeave={() => setIsOpen(false)}
                className="text-theme-light hover:text-theme-primary transition-colors focus:outline-none p-0.5 rounded-full hover:bg-theme-primary/5 cursor-pointer"
                title={title}
                aria-label={title}
            >
                <HelpCircle size={14} />
            </button>
            {isOpen && (
                <span className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 w-64 p-2.5 bg-slate-900 text-white text-[11px] font-normal leading-relaxed rounded-xl shadow-lg z-30 pointer-events-none animate-in fade-in zoom-in-95 duration-150">
                    <span className="font-bold text-slate-100 block mb-0.5">{title}</span>
                    <span className="opacity-90 block">{content}</span>
                </span>
            )}
        </span>
    );
};

interface PolicyEditorProps {
    policy: FrontendMemberPolicyDto;
    onChange: (updater: (prev: FrontendMemberPolicyDto) => FrontendMemberPolicyDto) => void;
    idPrefix: string;
}

const PolicyEditor: React.FC<PolicyEditorProps> = ({ policy, onChange, idPrefix }) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-4 text-xs">
            {/* Direct Contacts Selection */}
            <div>
                <label className="font-bold text-theme-secondary block mb-1.5">
                    {t('settings.syncHostDirectMinLabel')}
                </label>
                <div className="space-y-1.5">
                    <label className="flex items-center gap-2 cursor-pointer">
                        <input
                            type="radio"
                            name={`${idPrefix}-directMin`}
                            value={2}
                            checked={policy.directMin === 2}
                            onChange={() => onChange(p => ({ ...p, directMin: 2 }))}
                            className="text-theme-primary focus:ring-theme-primary"
                        />
                        <span className="text-theme-secondary font-medium">
                            {t('settings.syncHostDirectMinLevel2')}
                        </span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                        <input
                            type="radio"
                            name={`${idPrefix}-directMin`}
                            value={1}
                            checked={policy.directMin === 1}
                            onChange={() => onChange(p => ({ ...p, directMin: 1 }))}
                            className="text-theme-primary focus:ring-theme-primary"
                        />
                        <span className="text-theme-secondary font-medium">
                            {t('settings.syncHostDirectMinLevel1')}
                        </span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                        <input
                            type="radio"
                            name={`${idPrefix}-directMin`}
                            value={3}
                            checked={policy.directMin === 3}
                            onChange={() => onChange(p => ({ ...p, directMin: 3 }))}
                            className="text-theme-primary focus:ring-theme-primary"
                        />
                        <span className="text-theme-secondary font-medium">
                            {t('settings.syncHostDirectMinLevel3')}
                        </span>
                    </label>
                </div>
            </div>

            {/* Friends of Friends */}
            <div className="pt-2 border-t border-theme-subtle/50 space-y-3">
                <label className="flex items-center gap-2 cursor-pointer">
                    <input
                        type="checkbox"
                        checked={policy.hop2Enabled}
                        onChange={(e) => onChange(p => ({ ...p, hop2Enabled: e.target.checked }))}
                        className="rounded text-theme-primary focus:ring-theme-primary"
                    />
                    <span className="font-bold text-theme-secondary">
                        {t('settings.syncHostHop2Label')}
                    </span>
                </label>

                {policy.hop2Enabled && (
                    <div className="pl-6 space-y-3 pt-1 animate-in fade-in duration-150">
                        {/* bridgeMin selection */}
                        <div className="space-y-1">
                            <label className="font-medium text-theme-secondary flex items-center">
                                <span>{t('settings.syncHostBridgeMinLabel')}</span>
                                <PolicyHelpPopover
                                    title={t('settings.syncHostBridgeMinTitle')}
                                    content={t('settings.syncHostBridgeMinDesc')}
                                />
                            </label>
                            <select
                                value={policy.bridgeMin}
                                onChange={(e) => {
                                    const val = Number(e.target.value);
                                    onChange(p => ({ ...p, bridgeMin: val }));
                                }}
                                className="w-full sm:w-64 rounded-xl border border-theme-subtle bg-bg-app px-2.5 py-1.5 text-xs text-theme-secondary focus:outline-none focus:ring-1 focus:ring-theme-primary"
                            >
                                <option value={1}>{t('settings.trustLevel1Short')}</option>
                                <option value={2}>{t('settings.trustLevel2Short')}</option>
                                <option value={3}>{t('settings.trustLevel3Short')}</option>
                            </select>
                        </div>

                        {/* hop2Min selection */}
                        <div className="space-y-1">
                            <label className="font-medium text-theme-secondary flex items-center">
                                <span>{t('settings.syncHostHop2MinLabel')}</span>
                                <PolicyHelpPopover
                                    title={t('settings.syncHostHop2MinTitle')}
                                    content={t('settings.syncHostHop2MinDesc')}
                                />
                            </label>
                            <select
                                value={policy.hop2Min}
                                onChange={(e) => {
                                    const val = Number(e.target.value);
                                    onChange(p => ({ ...p, hop2Min: val }));
                                }}
                                className="w-full sm:w-64 rounded-xl border border-theme-subtle bg-bg-app px-2.5 py-1.5 text-xs text-theme-secondary focus:outline-none focus:ring-1 focus:ring-theme-primary"
                            >
                                <option value={1}>{t('settings.trustLevel1Short')}</option>
                                <option value={2}>{t('settings.trustLevel2Short')}</option>
                                <option value={3}>{t('settings.trustLevel3Short')}</option>
                            </select>
                        </div>
                    </div>
                )}
            </div>
        </div>
    );
};

export interface SyncServiceSettingsCardProps {
    settings: AppSettings | null;
    onSettingsUpdated?: (newSettings: AppSettings) => void;
    mode?: 'consumer' | 'host';
}

export const SyncServiceSettingsCard: React.FC<SyncServiceSettingsCardProps> = ({
    settings,
    onSettingsUpdated,
    mode,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [tabState, setTabState] = useState<'connected' | 'host'>('connected');
    const activeTab = mode === 'consumer' ? 'connected' : mode === 'host' ? 'host' : tabState;

    const [isLoading, setIsLoading] = useState(false);
    const [feedback, setFeedback] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    // Tab 1 state: Connected services & features
    const [discoveredDrops, setDiscoveredDrops] = useState<FrontendDiscoveredSync[]>([]);
    const [configuredPublicDrops, setConfiguredPublicDrops] = useState<StoredPublicSync[]>([]);
    const [inputUrl, setInputUrl] = useState('');
    const [isProbing, setIsProbing] = useState(false);
    const [probeResult, setProbeResult] = useState<{ status: 'success' | 'unauthorized' | 'error'; message: string } | null>(null);
    const { cooldownSeconds, isCoolingDown, checkAndTriggerCooldown } = useRateLimitCooldown();

    // Feature toggles
    const [postAcceptFromContacts, setPostAcceptFromContacts] = useState<boolean>(settings?.postAcceptFromContacts ?? true);
    const [marketSendCardOnPayment, setMarketSendCardOnPayment] = useState<boolean>(settings?.marketSendCardOnPayment ?? true);
    const [wotShareViaSync, setWotShareViaDrop] = useState<boolean>(settings?.wotShareViaSync ?? true);
    const [syncBackupEnabled, setDropBackupEnabled] = useState<boolean>(settings?.syncBackupEnabled ?? true);

    // Tab 2 state: Host / Own Sync Service
    const [adminStatus, setAdminStatus] = useState<FrontendSyncAdminStatus | null>(null);
    const [hostPolicy, setHostPolicy] = useState<FrontendMemberPolicyDto>({
        directMin: 2,
        hop2Enabled: true,
        bridgeMin: 2,
        hop2Min: 2,
    });
    const hostMaxMembers = 300;
    const hostMaxStorageMb = 50;
    const [hostWebspaceUrl, setHostWebspaceUrl] = useState('');
    const [isDownloadingZip, setIsDownloadingZip] = useState(false);
    const [isTestingHost, setIsTestingHost] = useState(false);
    const [isUpdatingPolicy, setIsUpdatingPolicy] = useState(false);
    const [isRotatingKey, setIsRotatingKey] = useState(false);
    const [isDeletingAdmin, setIsDeletingAdmin] = useState(false);
    const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
    const [copiedLink, setCopiedLink] = useState(false);

    // Synchronize local toggles when settings prop changes
    useEffect(() => {
        if (settings) {
            setPostAcceptFromContacts(settings.postAcceptFromContacts ?? true);
            setMarketSendCardOnPayment(settings.marketSendCardOnPayment ?? true);
            setWotShareViaDrop(settings.wotShareViaSync ?? true);
            setDropBackupEnabled(settings.syncBackupEnabled ?? true);
            if (Array.isArray(settings.publicSyncs)) {
                setConfiguredPublicDrops(settings.publicSyncs);
            }
        }
    }, [settings]);

    const loadData = useCallback(async () => {
        setIsLoading(true);
        try {
            const [discovered, admin, s] = await Promise.all([
                marketService.discoverAvailableSyncs().catch(() => []),
                marketService.getSyncAdminStatus().catch(() => null),
                settingsService.getSettings().catch(() => null),
            ]);
            setDiscoveredDrops(Array.isArray(discovered) ? discovered : []);
            setAdminStatus(admin || null);
            if (admin?.cardPolicy) {
                setHostPolicy(admin.cardPolicy);
            }
            if (Array.isArray(s?.publicSyncs)) {
                setConfiguredPublicDrops(s.publicSyncs);
            } else if (!settings?.publicSyncs) {
                setConfiguredPublicDrops([]);
            }
        } catch {
            // Silently retain last known data
        } finally {
            setIsLoading(false);
        }
    }, [settings]);

    useEffect(() => {
        loadData();
    }, [loadData]);

    const updateFeatureToggle = async (key: keyof AppSettings, value: boolean) => {
        try {
            const current = settings || await settingsService.getSettings();
            const updated: AppSettings = {
                ...current,
                [key]: value,
            };
            await settingsService.saveSettings(updated);
            if (onSettingsUpdated) {
                onSettingsUpdated(updated);
            }
            notifySyncStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    // --- Tab 1 Actions ---

    const handleProbeAndAdd = async () => {
        const trimmed = inputUrl.trim();
        if (!trimmed) return;
        setIsProbing(true);
        setProbeResult(null);
        try {
            const info = await marketService.probePublicSync(trimmed);
            if (info) {
                // Check if key is authorized
                const hasAuth = info.authorizedChannels.length > 0;
                await marketService.savePublicSync({
                    writeUrl: info.writeUrl,
                    readUrl: info.readUrl,
                    dropPub: info.dropPub,
                    registerUrl: info.registerUrl,
                    enabled: hasAuth,
                });
                setInputUrl('');
                const s = await settingsService.getSettings();
                if (s.publicSyncs) {
                    setConfiguredPublicDrops(s.publicSyncs);
                }
                if (onSettingsUpdated) {
                    onSettingsUpdated(s);
                }
                notifySyncStatusUpdated();

                if (hasAuth) {
                    setProbeResult({
                        status: 'success',
                        message: t('settings.syncProbeSuccess'),
                    });
                } else {
                    setProbeResult({
                        status: 'unauthorized',
                        message: t('settings.syncProbeUnauthorized'),
                    });
                }
            }
        } catch (err) {
            checkAndTriggerCooldown(err);
            setProbeResult({
                status: 'error',
                message: `${t('settings.syncProbeError')}: ${translateError(err, t)}`,
            });
        } finally {
            setIsProbing(false);
        }
    };

    const handleTogglePublicDrop = async (drop: StoredPublicSync) => {
        try {
            await marketService.savePublicSync({
                writeUrl: drop.writeUrl,
                readUrl: drop.readUrl,
                dropPub: drop.dropPub,
                registerUrl: drop.registerUrl,
                enabled: !drop.enabled,
            });
            const s = await settingsService.getSettings();
            if (s.publicSyncs) {
                setConfiguredPublicDrops(s.publicSyncs);
            }
            if (onSettingsUpdated) {
                onSettingsUpdated(s);
            }
            notifySyncStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleDeletePublicDrop = async (writeUrl: string) => {
        try {
            await marketService.removePublicSync(writeUrl, false);
            const s = await settingsService.getSettings();
            if (s.publicSyncs) {
                setConfiguredPublicDrops(s.publicSyncs);
            }
            if (onSettingsUpdated) {
                onSettingsUpdated(s);
            }
            notifySyncStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleToggleDiscoveredDrop = async (discovered: FrontendDiscoveredSync) => {
        try {
            const currentSelected = discoveredDrops.filter(d => d.isSelected).map(d => d.descriptor);
            const isCurrentlySelected = discovered.isSelected;
            let updatedList = [];
            if (isCurrentlySelected) {
                updatedList = currentSelected.filter(d => d.readUrl !== discovered.descriptor.readUrl);
            } else {
                updatedList = [...currentSelected, discovered.descriptor];
            }
            await marketService.setSelectedUploadSyncs(updatedList);
            const refetched = await marketService.discoverAvailableSyncs().catch(() => []);
            setDiscoveredDrops(refetched);
            notifySyncStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    // --- Tab 2 Actions (Host / Admin) ---

    const handleDownloadZip = async () => {
        setIsDownloadingZip(true);
        setFeedback(null);
        try {
            const filePath = await save({
                defaultPath: 'humoco-sync-setup.zip',
                filters: [{ name: 'ZIP Archive', extensions: ['zip'] }],
            });
            if (filePath) {
                await protectAction(async (pwd) => {
                    await marketService.prepareSyncSetup({
                        policy: hostPolicy,
                        maxMembers: hostMaxMembers,
                        maxTotalStorageMb: hostMaxStorageMb,
                    }, pwd || undefined);
                    await marketService.saveSyncSetupZip(filePath, pwd || undefined);
                });
                setFeedback({
                    type: 'success',
                    message: t('settings.syncZipSaved'),
                });
            }
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsDownloadingZip(false);
        }
    };

    const handleTestAndActivateHost = async () => {
        const trimmed = hostWebspaceUrl.trim();
        if (!trimmed) return;
        setIsTestingHost(true);
        setFeedback(null);
        try {
            await protectAction(async (pwd) => {
                const testRes = await marketService.testSyncConnection(trimmed, pwd || undefined);
                if (!testRes.success || !testRes.writeOk || !testRes.readOk) {
                    throw new Error(testRes.message || 'Verbindung zum Webspace konnte nicht verifiziert werden.');
                }
                const admin = await marketService.finalizeSyncSetup(trimmed, pwd || undefined);
                setAdminStatus(admin);
                notifySyncStatusUpdated();
                setFeedback({
                    type: 'success',
                    message: t('settings.syncHostActivatedSuccess'),
                });
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsTestingHost(false);
        }
    };

    const handleUpdateHostPolicy = async () => {
        setIsUpdatingPolicy(true);
        setFeedback(null);
        try {
            await protectAction(async (pwd) => {
                const updated = await marketService.updateSyncAdminConfig({
                    cardPolicy: hostPolicy,
                    maxMembers: hostMaxMembers,
                    maxTotalStorageMb: hostMaxStorageMb,
                }, true, pwd || undefined);
                setAdminStatus(updated);
                setFeedback({
                    type: 'success',
                    message: t('settings.syncHostPolicyUpdated'),
                });
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsUpdatingPolicy(false);
        }
    };

    const handleRotateKey = async () => {
        setIsRotatingKey(true);
        setFeedback(null);
        try {
            await protectAction(async (pwd) => {
                const updated = await marketService.rotateSyncAdminKey(pwd || undefined);
                setAdminStatus(updated);
                setFeedback({
                    type: 'success',
                    message: t('settings.syncHostKeyRotated'),
                });
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsRotatingKey(false);
        }
    };

    const handleDeleteHost = async () => {
        setIsDeletingAdmin(true);
        try {
            await protectAction(async (pwd) => {
                await marketService.deleteSyncAdmin(pwd || undefined);
                setAdminStatus(null);
                setIsDeleteConfirmOpen(false);
                notifySyncStatusUpdated();
                setFeedback({
                    type: 'success',
                    message: t('settings.syncHostDeletedSuccess'),
                });
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsDeletingAdmin(false);
        }
    };

    const handleCopyFriendLink = async (url: string) => {
        const ok = await copyToClipboard(url);
        if (ok) {
            setCopiedLink(true);
            setTimeout(() => setCopiedLink(false), 2000);
        }
    };

    const isConsumerOnly = mode === 'consumer';
    const isHostOnly = mode === 'host';

    return (
        <Card
            header={
                <div className="flex items-center justify-between gap-3 w-full">
                    <div className="flex items-center gap-2">
                        {isHostOnly ? <Server size={18} className="text-theme-primary" /> : <Cloud size={18} className="text-theme-primary" />}
                        <span className="font-black text-xs uppercase tracking-widest">
                            {isHostOnly ? t('settings.ownSyncServiceTitle') : t('settings.syncServiceCardTitle')}
                        </span>
                    </div>
                    {isLoading && <RefreshCw size={14} className="animate-spin text-theme-light" />}
                </div>
            }
        >
            <div className="space-y-6">
                <p className="text-xs text-theme-light font-medium">
                    {isHostOnly ? t('settings.ownSyncServiceDesc') : t('settings.syncServiceCardSubtitle')}
                </p>

                {isConsumerOnly && (
                    <InfoHint topic="connection" />
                )}

                {feedback && (
                    <div className={`p-3 rounded-xl border text-xs flex items-center gap-2 ${
                        feedback.type === 'success'
                            ? 'bg-emerald-50 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800'
                            : 'bg-rose-50 text-rose-800 border-rose-200 dark:bg-rose-950/40 dark:text-rose-300 dark:border-rose-800'
                    }`}>
                        {feedback.type === 'success' ? <CheckCircle2 size={16} /> : <AlertTriangle size={16} />}
                        <span>{feedback.message}</span>
                    </div>
                )}

                {/* Sub-Tabs: only rendered if mode is not fixed to consumer or host */}
                {!isConsumerOnly && !isHostOnly && (
                    <SegmentedTabs
                        activeTab={activeTab}
                        onChange={(id) => setTabState(id as 'connected' | 'host')}
                        tabs={[
                            {
                                id: 'connected',
                                label: t('settings.syncTabConnected'),
                                icon: <Users size={16} />,
                            },
                            {
                                id: 'host',
                                label: t('settings.syncTabHost'),
                                icon: <Server size={16} />,
                            },
                        ]}
                    />
                )}

                {/* TAB 1: Connected Services & Features */}
                {activeTab === 'connected' && (
                    <div className="space-y-6 animate-in fade-in duration-200">
                        {/* 1. Feature Comfort Toggles (Channels) */}
                        <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle space-y-3">
                            <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                {t('settings.syncChannelsTitle')}
                            </h4>

                            <div className="space-y-2.5 pt-1">
                                <label className="flex items-start gap-3 cursor-pointer group">
                                    <input
                                        type="checkbox"
                                        checked={postAcceptFromContacts}
                                        onChange={(e) => {
                                            setPostAcceptFromContacts(e.target.checked);
                                            updateFeatureToggle('postAcceptFromContacts', e.target.checked);
                                        }}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div>
                                        <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                            {t('settings.syncChannelPostLabel')}
                                        </span>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('settings.syncChannelPostDesc')}
                                        </p>
                                    </div>
                                </label>

                                <label className="flex items-start gap-3 cursor-pointer group pt-1 border-t border-theme-subtle/50">
                                    <input
                                        type="checkbox"
                                        checked={marketSendCardOnPayment}
                                        onChange={(e) => {
                                            setMarketSendCardOnPayment(e.target.checked);
                                            updateFeatureToggle('marketSendCardOnPayment', e.target.checked);
                                        }}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div>
                                        <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                            {t('settings.syncChannelMarketLabel')}
                                        </span>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('settings.syncChannelMarketDesc')}
                                        </p>
                                    </div>
                                </label>

                                <label className="flex items-start gap-3 cursor-pointer group pt-1 border-t border-theme-subtle/50">
                                    <input
                                        type="checkbox"
                                        checked={wotShareViaSync}
                                        onChange={(e) => {
                                            setWotShareViaDrop(e.target.checked);
                                            updateFeatureToggle('wotShareViaSync', e.target.checked);
                                        }}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div>
                                        <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                            {t('settings.syncChannelWotLabel')}
                                        </span>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('settings.syncChannelWotDesc')}
                                        </p>
                                    </div>
                                </label>

                                <label className="flex items-start gap-3 cursor-pointer group pt-1 border-t border-theme-subtle/50">
                                    <input
                                        type="checkbox"
                                        checked={syncBackupEnabled}
                                        onChange={(e) => {
                                            setDropBackupEnabled(e.target.checked);
                                            updateFeatureToggle('syncBackupEnabled', e.target.checked);
                                        }}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div>
                                        <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                            {t('settings.syncChannelBackupLabel')}
                                        </span>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('settings.syncChannelBackupDesc')}
                                        </p>
                                    </div>
                                </label>
                            </div>
                        </div>

                        {/* 2. URL-Schnelleingabe */}
                        <div className="p-4 bg-white rounded-2xl border border-theme-subtle shadow-xs space-y-3">
                            <div>
                                <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                    {t('settings.syncAddTitle')}
                                </h4>
                                <p className="text-[11px] text-theme-light mt-0.5">
                                    {t('settings.syncAddDesc')}
                                </p>
                            </div>

                            <div className="flex flex-col sm:flex-row gap-2">
                                <Input
                                    value={inputUrl}
                                    onChange={(e) => setInputUrl(e.target.value)}
                                    placeholder={t('settings.syncAddPlaceholder')}
                                    className="text-xs flex-1"
                                    onKeyDown={(e) => {
                                        if (e.key === 'Enter') {
                                            e.preventDefault();
                                            handleProbeAndAdd();
                                        }
                                    }}
                                />
                                <Button
                                    type="button"
                                    variant="primary"
                                    size="sm"
                                    onClick={handleProbeAndAdd}
                                    disabled={isProbing || !inputUrl.trim() || isCoolingDown}
                                    className="gap-1.5 shrink-0"
                                >
                                    <RefreshCw size={13} className={isProbing ? 'animate-spin' : ''} />
                                    <span>
                                        {isCoolingDown
                                            ? t('settings.syncRateLimitWait', { seconds: cooldownSeconds })
                                            : isProbing
                                            ? t('common.testing')
                                            : t('settings.syncAddBtn')}
                                    </span>
                                </Button>
                            </div>

                            {probeResult && (
                                <div className={`p-2.5 rounded-xl border text-xs flex items-start gap-2 ${
                                    probeResult.status === 'success'
                                        ? 'bg-emerald-50 text-emerald-800 border-emerald-200'
                                        : probeResult.status === 'unauthorized'
                                            ? 'bg-amber-50 text-amber-800 border-amber-200'
                                            : 'bg-rose-50 text-rose-800 border-rose-200'
                                }`}>
                                    {probeResult.status === 'success' ? (
                                        <CheckCircle2 size={16} className="shrink-0 mt-0.5 text-emerald-600" />
                                    ) : (
                                        <AlertTriangle size={16} className="shrink-0 mt-0.5 text-amber-600" />
                                    )}
                                    <span className="leading-relaxed">{probeResult.message}</span>
                                </div>
                            )}
                        </div>

                        {/* 3. Discovered Friend Drops (Auto-Discovery) */}
                        <div className="space-y-3">
                            <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                                <Users size={14} className="text-theme-primary" />
                                <span>{t('settings.syncDiscoveredTitle')}</span>
                            </h4>

                            {(discoveredDrops || []).length === 0 ? (
                                <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle text-xs text-theme-light text-center leading-relaxed">
                                    {t('settings.syncDiscoveredEmpty')}
                                </div>
                            ) : (
                                <div className="space-y-2">
                                    {(discoveredDrops || []).map((drop, idx) => (
                                        <div
                                            key={idx}
                                            className="p-3 bg-white rounded-xl border border-theme-subtle flex items-center justify-between gap-3 text-xs"
                                        >
                                            <div className="flex items-center gap-2.5 min-w-0">
                                                <input
                                                    type="checkbox"
                                                    checked={drop.isSelected}
                                                    onChange={() => handleToggleDiscoveredDrop(drop)}
                                                    className="rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                                    title="Für Synchronisation aktivieren"
                                                />
                                                <div className="min-w-0">
                                                    <span className="font-bold text-theme-primary truncate block">
                                                        {t('settings.syncConnectedVia', {
                                                            name: drop.sourceLabel || drop.descriptor.readUrl,
                                                        })}
                                                    </span>
                                                    <span className="font-mono text-[10px] text-theme-light truncate block">
                                                        {drop.descriptor.readUrl}
                                                    </span>
                                                </div>
                                            </div>

                                            <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-100 text-emerald-800 shrink-0">
                                                Verbunden
                                            </span>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>

                        {/* 4. Configured Public / Manual Drops */}
                        <div className="space-y-3 pt-2 border-t border-theme-subtle/50">
                            <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                                <Globe size={14} className="text-theme-primary" />
                                <span>{t('settings.syncPublicTitle')}</span>
                            </h4>

                            {(configuredPublicDrops || []).length === 0 ? (
                                <p className="text-xs text-theme-light">
                                    {t('settings.syncPublicEmpty')}
                                </p>
                            ) : (
                                <div className="space-y-2">
                                    {(configuredPublicDrops || []).map((drop, idx) => (
                                        <div
                                            key={idx}
                                            className="p-3 bg-white rounded-xl border border-theme-subtle flex items-center justify-between gap-3 text-xs"
                                        >
                                            <div className="min-w-0">
                                                <span className="font-mono text-xs text-theme-primary truncate block">
                                                    {drop.writeUrl}
                                                </span>
                                                <span className="text-[10px] text-theme-light block">
                                                    {(drop.authorizedChannels || []).length > 0
                                                        ? `Kanäle: ${(drop.authorizedChannels || []).join(', ')}`
                                                        : 'Schlüsselprüfung ausstehend'}
                                                </span>
                                            </div>

                                            <div className="flex items-center gap-2 shrink-0">
                                                <Button
                                                    type="button"
                                                    variant="outline"
                                                    size="sm"
                                                    onClick={() => handleTogglePublicDrop(drop)}
                                                    className="text-[11px] h-7 px-2"
                                                >
                                                    {drop.enabled ? 'Deaktivieren' : 'Aktivieren'}
                                                </Button>

                                                <button
                                                    type="button"
                                                    onClick={() => handleDeletePublicDrop(drop.writeUrl)}
                                                    className="text-theme-placeholder hover:text-rose-600 p-1 rounded transition-colors"
                                                    title={t('common.delete')}
                                                >
                                                    <Trash2 size={15} />
                                                </button>
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    </div>
                )}

                {/* TAB 2: Host / Own Sync Service */}
                {activeTab === 'host' && (
                    <div className="space-y-6 animate-in fade-in duration-200">
                        {adminStatus == null ? (
                            /* Setup Wizard for new Own Drop */
                            <div className="space-y-6">
                                {/* Intro banner */}
                                <div className="p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/20 space-y-2 text-xs text-theme-secondary">
                                    <h4 className="font-bold text-theme-primary text-sm flex items-center gap-1.5">
                                        <Server size={16} />
                                        <span>{t('settings.syncHostIntroTitle')}</span>
                                    </h4>
                                    <p className="leading-relaxed">
                                        {t('settings.syncHostIntroDesc')}
                                    </p>
                                    <div className="flex items-center gap-1.5 pt-1 text-[11px] font-semibold text-emerald-700 dark:text-emerald-400">
                                        <Shield size={13} />
                                        <span>{t('settings.syncHostPrivacyNotice')}</span>
                                    </div>
                                </div>

                                {/* Step 1: Policies & ZIP Download */}
                                <div className="p-4 bg-white rounded-2xl border border-theme-subtle space-y-4">
                                    <div className="flex items-center gap-2">
                                        <span className="w-5 h-5 rounded-full bg-theme-primary text-white text-xs font-bold flex items-center justify-center shrink-0">1</span>
                                        <h4 className="text-xs font-bold text-theme-primary uppercase tracking-wider">
                                            {t('settings.syncHostSetupStep1')}
                                        </h4>
                                    </div>

                                    <PolicyEditor
                                        policy={hostPolicy}
                                        onChange={setHostPolicy}
                                        idPrefix="setup"
                                    />

                                    <div className="pt-2">
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={handleDownloadZip}
                                            disabled={isDownloadingZip}
                                            className="gap-2"
                                        >
                                            <FileArchive size={14} className="text-theme-primary" />
                                            <span>{isDownloadingZip ? t('common.saving') : t('settings.syncHostDownloadZip')}</span>
                                        </Button>
                                    </div>
                                </div>

                                {/* Step 2: URL & Activation */}
                                <div className="p-4 bg-white rounded-2xl border border-theme-subtle space-y-4">
                                    <div className="flex items-center gap-2">
                                        <span className="w-5 h-5 rounded-full bg-theme-primary text-white text-xs font-bold flex items-center justify-center shrink-0">2</span>
                                        <h4 className="text-xs font-bold text-theme-primary uppercase tracking-wider">
                                            {t('settings.syncHostSetupStep2')}
                                        </h4>
                                    </div>

                                    <div className="flex flex-col sm:flex-row gap-2">
                                        <Input
                                            value={hostWebspaceUrl}
                                            onChange={(e) => setHostWebspaceUrl(e.target.value)}
                                            placeholder={t('settings.syncHostUrlPlaceholder')}
                                            className="text-xs flex-1"
                                        />
                                        <Button
                                            type="button"
                                            variant="primary"
                                            size="sm"
                                            onClick={handleTestAndActivateHost}
                                            disabled={isTestingHost || !hostWebspaceUrl.trim()}
                                            className="gap-1.5 shrink-0"
                                        >
                                            <RefreshCw size={13} className={isTestingHost ? 'animate-spin' : ''} />
                                            <span>{isTestingHost ? t('common.testing') : t('settings.syncHostTestActivate')}</span>
                                        </Button>
                                    </div>
                                </div>
                            </div>
                        ) : (
                            /* Admin View for Active Own Drop */
                            <div className="space-y-6">
                                <div className="p-4 bg-emerald-500/10 border border-emerald-500/20 rounded-2xl flex items-start justify-between gap-3 text-xs">
                                    <div className="flex items-start gap-2.5">
                                        <CheckCircle2 size={18} className="text-emerald-600 mt-0.5 shrink-0" />
                                        <div>
                                            <h4 className="font-bold text-emerald-950 text-sm">
                                                {t('settings.syncHostActiveTitle')}
                                            </h4>
                                            <p className="font-mono text-[11px] text-emerald-800 break-all mt-0.5">
                                                {adminStatus.descriptor.readUrl}
                                            </p>
                                        </div>
                                    </div>

                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => handleCopyFriendLink(adminStatus.descriptor.readUrl)}
                                        className="gap-1 text-xs shrink-0 bg-white"
                                    >
                                        {copiedLink ? <Check size={13} className="text-emerald-600" /> : <Copy size={13} />}
                                        <span>{copiedLink ? t('common.copied') : t('settings.syncHostCopyLink')}</span>
                                    </Button>
                                </div>

                                {/* Usage statistics */}
                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                                    <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle space-y-2">
                                        <div className="flex justify-between items-center text-xs">
                                            <span className="font-bold text-theme-secondary flex items-center gap-1.5">
                                                <HardDrive size={14} className="text-theme-primary" />
                                                {t('settings.syncHostStorageUsage')}
                                            </span>
                                            <span className="font-mono text-theme-light">
                                                {Math.round((adminStatus.usedBytes || 0) / 1024 / 1024 * 10) / 10} / {adminStatus.maxTotalStorageMb} MB
                                            </span>
                                        </div>
                                        <div className="w-full h-2 rounded-full bg-slate-200 overflow-hidden">
                                            <div
                                                className={`h-full transition-all ${
                                                    (adminStatus.usedBytes || 0) / (adminStatus.maxTotalStorageMb * 1024 * 1024) > 0.8
                                                        ? 'bg-amber-500'
                                                        : 'bg-emerald-500'
                                                }`}
                                                style={{
                                                    width: `${Math.min(100, Math.round(((adminStatus.usedBytes || 0) / (adminStatus.maxTotalStorageMb * 1024 * 1024)) * 100))}%`,
                                                }}
                                            />
                                        </div>
                                    </div>

                                    <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle space-y-2">
                                        <div className="flex justify-between items-center text-xs">
                                            <span className="font-bold text-theme-secondary flex items-center gap-1.5">
                                                <Users size={14} className="text-theme-primary" />
                                                {t('settings.syncHostMembers')}
                                            </span>
                                            <span className="font-mono text-theme-light">
                                                {adminStatus.totalMemberCount || 0} / {adminStatus.maxMembers}
                                            </span>
                                        </div>
                                        <div className="w-full h-2 rounded-full bg-slate-200 overflow-hidden">
                                            <div
                                                className="h-full bg-theme-primary transition-all"
                                                style={{
                                                    width: `${Math.min(100, Math.round(((adminStatus.totalMemberCount || 0) / adminStatus.maxMembers) * 100))}%`,
                                                }}
                                            />
                                        </div>
                                    </div>
                                </div>

                                {/* Access Rules Editor for Active Service */}
                                <div className="p-4 bg-white rounded-2xl border border-theme-subtle space-y-3">
                                    <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                                        <Shield size={14} className="text-theme-primary" />
                                        <span>{t('settings.syncHostAccessRulesTitle')}</span>
                                    </h4>

                                    <PolicyEditor
                                        policy={hostPolicy}
                                        onChange={setHostPolicy}
                                        idPrefix="admin"
                                    />

                                    <div className="pt-2">
                                        <Button
                                            type="button"
                                            variant="primary"
                                            size="sm"
                                            onClick={handleUpdateHostPolicy}
                                            disabled={isUpdatingPolicy}
                                            className="gap-1.5 text-xs"
                                        >
                                            <RefreshCw size={13} className={isUpdatingPolicy ? 'animate-spin' : ''} />
                                            <span>{isUpdatingPolicy ? t('common.updating') : t('settings.syncHostUpdatePolicy')}</span>
                                        </Button>
                                    </div>
                                </div>

                                {/* Danger / Management Actions */}
                                <div className="flex flex-wrap items-center gap-2 pt-2 border-t border-theme-subtle/50">
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={handleRotateKey}
                                        disabled={isRotatingKey}
                                        className="gap-1.5 text-xs"
                                    >
                                        <KeyRound size={13} />
                                        <span>{t('settings.syncHostRotateKey')}</span>
                                    </Button>

                                    <Button
                                        type="button"
                                        variant="danger"
                                        size="sm"
                                        onClick={() => setIsDeleteConfirmOpen(true)}
                                        className="gap-1.5 text-xs ml-auto"
                                    >
                                        <Trash2 size={13} />
                                        <span>{t('settings.syncHostDelete')}</span>
                                    </Button>
                                </div>
                            </div>
                        )}
                    </div>
                )}
            </div>

            {/* Confirmation dialog for deleting own drop */}
            <ConfirmationModal
                isOpen={isDeleteConfirmOpen}
                title={t('settings.syncHostDeleteConfirmTitle')}
                description={t('settings.syncHostDeleteConfirmDesc')}
                confirmVariant="danger"
                confirmText={t('common.delete')}
                isProcessing={isDeletingAdmin}
                onConfirm={handleDeleteHost}
                onCancel={() => setIsDeleteConfirmOpen(false)}
            />
        </Card>
    );
};

export default SyncServiceSettingsCard;
