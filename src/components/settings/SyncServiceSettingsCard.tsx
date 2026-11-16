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
    FileArchive,
    FileCode,
    ExternalLink,
    HelpCircle,
    HeartHandshake,
} from 'lucide-react';
import { save } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
    AppSettings,
    FrontendDiscoveredSync,
    FrontendSyncAdminStatus,
    FrontendMemberPolicyDto,
    FrontendSyncDescriptor,
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
import { runSyncBackupPass } from '../../utils/autoBackup';
import { notifySyncStatusUpdated, SYNC_STATUS_UPDATED_EVENT } from '../../utils/syncEvents';
import { translateError } from '../../utils/errorHelper';
import { useRateLimitCooldown } from '../../hooks/useRateLimitCooldown';

const COMMUNITY_SYNC_PRESET_URL = 'https://sync.menschlich-miteinander.org/sync.php';

const isSafeExternalSyncUrl = (raw?: string | null): raw is string => {
    if (!raw) return false;
    try {
        const u = new URL(raw.trim());
        if (u.protocol === 'https:') return true;
        if (u.protocol === 'http:') {
            return (
                u.hostname === '127.0.0.1' ||
                u.hostname === 'localhost' ||
                u.hostname === '::1' ||
                u.hostname === '[::1]'
            );
        }
        return false;
    } catch {
        return false;
    }
};

export const HOST_SYNC_DRAFT_KEY = 'humoco_draft_host_webspace_url';
export const CONSUMER_SYNC_DRAFT_KEY = 'humoco_draft_consumer_sync_url';

const getSessionDraft = (key: string): string => {
    try {
        if (typeof window !== 'undefined' && window.sessionStorage) {
            return window.sessionStorage.getItem(key) || '';
        }
    } catch {
        // ignore
    }
    return '';
};

const setSessionDraft = (key: string, value: string): void => {
    try {
        if (typeof window !== 'undefined' && window.sessionStorage) {
            if (value) {
                window.sessionStorage.setItem(key, value);
            } else {
                window.sessionStorage.removeItem(key);
            }
        }
    } catch {
        // ignore
    }
};

/**
 * Intelligently normalizes a user-entered Sync URL:
 * - Trims whitespace
 * - Prepends 'https://' if scheme is missing
 * - Upgrades 'http://' to 'https://' unless it's a local development loopback (localhost, 127.0.0.1, [::1])
 * - Removes query strings and fragments
 * - Normalizes path so that 'sync.php' is automatically recognized and appended if omitted,
 *   without duplicating it if the user already entered it.
 */
export function normalizeSyncUrlInput(raw: string): string {
    const trimmed = raw.trim();
    if (!trimmed) return '';

    let s = trimmed;
    if (!s.includes('://')) {
        s = 'https://' + s;
    } else if (s.startsWith('http://')) {
        const after = s.slice(7);
        const host = after.split(/[/:]/)[0].toLowerCase();
        if (host !== 'localhost' && host !== '127.0.0.1' && host !== '[::1]') {
            s = 'https://' + after;
        }
    }

    // Strip fragment and query
    s = s.replace(/[#?].*$/, '');

    const schemeIdx = s.indexOf('://');
    const scheme = s.substring(0, schemeIdx + 3);
    const rest = s.substring(schemeIdx + 3);

    const firstSlash = rest.indexOf('/');
    let authority = firstSlash === -1 ? rest : rest.substring(0, firstSlash);
    const rawPath = firstSlash === -1 ? '/' : rest.substring(firstSlash);

    // Lowercase host/authority
    authority = authority.toLowerCase();

    // Strip default port 443 for https, 80 for http
    if (scheme === 'https://' && authority.endsWith(':443')) {
        authority = authority.slice(0, -4);
    } else if (scheme === 'http://' && authority.endsWith(':80')) {
        authority = authority.slice(0, -3);
    }

    // Normalize path: trim trailing slashes
    const cleanPath = rawPath.replace(/\/+$/, '');
    let canonicalPath: string;
    if (!cleanPath || cleanPath === '') {
        canonicalPath = '/sync.php';
    } else if (cleanPath.endsWith('/sync.php') || cleanPath === '/sync.php') {
        canonicalPath = cleanPath;
    } else {
        canonicalPath = `${cleanPath}/sync.php`;
    }

    return `${scheme}${authority}${canonicalPath}`;
}

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

const normalizeEndpointUrl = (u?: string) => (u || '').trim().replace(/\/+$/, '').toLowerCase();

export const isSameSyncEndpoint = (
    a: { writeUrl?: string; readUrl?: string },
    b: { writeUrl?: string; readUrl?: string }
): boolean => {
    const aWrite = normalizeEndpointUrl(a.writeUrl);
    const bWrite = normalizeEndpointUrl(b.writeUrl);
    const aRead = normalizeEndpointUrl(a.readUrl);
    const bRead = normalizeEndpointUrl(b.readUrl);
    return (
        (Boolean(aWrite) && Boolean(bWrite) && aWrite === bWrite) ||
        (Boolean(aRead) && Boolean(bRead) && aRead === bRead)
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
            <div className="space-y-1">
                <label
                    htmlFor={`${idPrefix}-directMin`}
                    className="font-bold text-theme-secondary block"
                >
                    {t('settings.syncHostDirectMinLabel')}
                </label>
                <select
                    id={`${idPrefix}-directMin`}
                    aria-label={t('settings.syncHostDirectMinLabel')}
                    value={policy.directMin}
                    onChange={(e) => {
                        const val = Number(e.target.value);
                        onChange(p => ({ ...p, directMin: val }));
                    }}
                    className="w-full sm:w-64 rounded-xl border border-theme-subtle bg-bg-app px-2.5 py-1.5 text-xs text-theme-secondary focus:outline-none focus:ring-1 focus:ring-theme-primary"
                >
                    <option value={1}>{t('settings.syncHostDirectMinLevel1')}</option>
                    <option value={2}>{t('settings.syncHostDirectMinLevel2')}</option>
                    <option value={3}>{t('settings.syncHostDirectMinLevel3')}</option>
                </select>
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
    const [feedback, setFeedback] = useState<{ type: 'success' | 'error' | 'info'; message: string } | null>(null);

    // Tab 1 state: Connected services & features
    const [discoveredDrops, setDiscoveredDrops] = useState<FrontendDiscoveredSync[]>([]);
    const [configuredPublicDrops, setConfiguredPublicDrops] = useState<StoredPublicSync[]>([]);
    const [inputUrl, setInputUrlState] = useState(() => getSessionDraft(CONSUMER_SYNC_DRAFT_KEY));
    const setInputUrl = (val: string) => {
        setInputUrlState(val);
        setSessionDraft(CONSUMER_SYNC_DRAFT_KEY, val);
    };
    const [isProbing, setIsProbing] = useState(false);
    const [isProbingContacts, setIsProbingContacts] = useState(false);
    const [probeResult, setProbeResult] = useState<{ status: 'success' | 'unauthorized' | 'error'; message: string } | null>(null);
    const { cooldownSeconds, isCoolingDown, checkAndTriggerCooldown } = useRateLimitCooldown();

    // Feature toggles
    const [postAcceptFromContacts, setPostAcceptFromContacts] = useState<boolean>(settings?.postAcceptFromContacts ?? true);
    const [marketSyncEnabled, setMarketSyncEnabled] = useState<boolean>(settings?.marketSyncEnabled ?? true);
    const [wotShareViaSync, setWotShareViaDrop] = useState<boolean>(settings?.wotShareViaSync ?? true);
    const [shareMinLevel, setShareMinLevel] = useState<number>(settings?.shareMinLevel ?? 1);
    const [marketPollWifiOnly, setMarketPollWifiOnly] = useState<boolean>(settings?.marketPollWifiOnly ?? false);

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
    const [hostWebspaceUrl, setHostWebspaceUrlState] = useState(() => getSessionDraft(HOST_SYNC_DRAFT_KEY));
    const setHostWebspaceUrl = (val: string) => {
        setHostWebspaceUrlState(val);
        setSessionDraft(HOST_SYNC_DRAFT_KEY, val);
    };
    const [isDownloadingZip, setIsDownloadingZip] = useState(false);
    const [isCopyingRustConfig, setIsCopyingRustConfig] = useState(false);
    const [isTestingHost, setIsTestingHost] = useState(false);
    const [isUpdatingPolicy, setIsUpdatingPolicy] = useState(false);
    const [isDeletingAdmin, setIsDeletingAdmin] = useState(false);
    const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
    const [pendingPublicDelete, setPendingPublicDelete] = useState<StoredPublicSync | null>(null);
    const [deletePublicRemoteData, setDeletePublicRemoteData] = useState(true);
    const [isDeletingPublicDrop, setIsDeletingPublicDrop] = useState(false);
    const [checkingPublicUrl, setCheckingPublicUrl] = useState<string | null>(null);
    const [copiedPublicKeyUrl, setCopiedPublicKeyUrl] = useState<string | null>(null);
    const [isRevokeModalOpen, setIsRevokeModalOpen] = useState(false);
    const [revokeGraceDays, setRevokeGraceDays] = useState<'30' | '14' | '0'>('30');
    const [revokeReplacementUrl, setRevokeReplacementUrl] = useState('');
    const [revokeReason, setRevokeReason] = useState('');
    const [isRevokingServer, setIsRevokingServer] = useState(false);
    const [copiedLink, setCopiedLink] = useState(false);

    // Synchronize local toggles when settings prop changes
    useEffect(() => {
        if (settings) {
            setPostAcceptFromContacts(settings.postAcceptFromContacts ?? true);
            setMarketSyncEnabled(settings.marketSyncEnabled ?? true);
            setWotShareViaDrop(settings.wotShareViaSync ?? true);
            setShareMinLevel(settings.shareMinLevel ?? 1);
            setMarketPollWifiOnly(settings.marketPollWifiOnly ?? false);
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
            const discList = Array.isArray(discovered) ? discovered : [];
            setDiscoveredDrops(discList);
            setAdminStatus(admin || null);
            if (admin?.cardPolicy) {
                setHostPolicy(admin.cardPolicy);
            }
            if (Array.isArray(s?.publicSyncs)) {
                setConfiguredPublicDrops(s.publicSyncs);
            } else if (!settings?.publicSyncs) {
                setConfiguredPublicDrops([]);
            }

            if (discList.some(d => d.status === 'unverified') && typeof marketService.probeContactSyncs === 'function') {
                marketService.probeContactSyncs(false).then(() => {
                    marketService.discoverAvailableSyncs().then(refreshed => {
                        if (Array.isArray(refreshed)) setDiscoveredDrops(refreshed);
                    }).catch(() => {});
                }).catch(() => {});
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

    useEffect(() => {
        if (typeof marketService.probeContactSyncs === 'function') {
            marketService.probeContactSyncs(false).then(() => loadData()).catch(() => {});
        }

        const handleSyncUpdated = () => {
            loadData();
        };
        window.addEventListener(SYNC_STATUS_UPDATED_EVENT, handleSyncUpdated);
        return () => {
            window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, handleSyncUpdated);
        };
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

    const updateSettingNumber = async (key: keyof AppSettings, value: number) => {
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
        const normalized = normalizeSyncUrlInput(trimmed);
        if (normalized && normalized !== inputUrl) {
            setInputUrl(normalized);
        }
        const targetUrl = normalized || trimmed;

        setIsProbing(true);
        setProbeResult(null);
        try {
            const info = await marketService.probePublicSync(targetUrl);
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
                    if (info.authorizedChannels.includes('backup')) {
                        setTimeout(() => {
                            runSyncBackupPass(true).catch(() => {});
                        }, 1000);
                    }
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

    const handleOpenDeletePublicDrop = (drop: StoredPublicSync) => {
        setDeletePublicRemoteData(true);
        setPendingPublicDelete(drop);
    };

    const handleConfirmDeletePublicDrop = async () => {
        if (!pendingPublicDelete) return;
        setIsDeletingPublicDrop(true);
        setFeedback(null);
        try {
            await marketService.removePublicSync(pendingPublicDelete.writeUrl, deletePublicRemoteData);
            setPendingPublicDelete(null);
            const s = await settingsService.getSettings();
            if (s.publicSyncs) {
                setConfiguredPublicDrops(s.publicSyncs);
            }
            if (onSettingsUpdated) {
                onSettingsUpdated(s);
            }
            notifySyncStatusUpdated();
            setFeedback({
                type: 'success',
                message: t('settings.syncPublicRemovedSuccess'),
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsDeletingPublicDrop(false);
        }
    };

    const handleOpenPublicRegister = async (drop: StoredPublicSync) => {
        if (!isSafeExternalSyncUrl(drop.registerUrl)) return;
        const sep = drop.registerUrl.includes('?') ? '&' : '?';
        const fullUrl = `${drop.registerUrl}${sep}key=${encodeURIComponent(drop.accessPubkey)}`;
        try {
            await openUrl(fullUrl);
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleCopyPublicAccessKey = async (drop: StoredPublicSync) => {
        try {
            await copyToClipboard(drop.accessPubkey);
            setCopiedPublicKeyUrl(drop.writeUrl);
            setFeedback({ type: 'success', message: t('settings.syncPublicKeyCopied') });
            setTimeout(() => {
                setCopiedPublicKeyUrl((prev) => (prev === drop.writeUrl ? null : prev));
            }, 2000);
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleCheckPublicDropAuth = async (drop: StoredPublicSync) => {
        setCheckingPublicUrl(drop.writeUrl);
        setFeedback(null);
        try {
            const info = await marketService.probePublicSync(drop.writeUrl);
            if (info) {
                const hasAuth = (info.authorizedChannels || []).length > 0;
                await marketService.savePublicSync({
                    writeUrl: info.writeUrl,
                    readUrl: info.readUrl,
                    dropPub: info.dropPub,
                    registerUrl: info.registerUrl ?? drop.registerUrl,
                    enabled: hasAuth,
                });
                const s = await settingsService.getSettings();
                if (s.publicSyncs) {
                    setConfiguredPublicDrops(s.publicSyncs);
                }
                if (onSettingsUpdated) {
                    onSettingsUpdated(s);
                }
                notifySyncStatusUpdated();
                setFeedback({
                    type: hasAuth ? 'success' : 'info',
                    message: hasAuth
                        ? t('settings.syncProbeSuccess')
                        : t('settings.syncProbeUnauthorized'),
                });
            }
        } catch (err) {
            checkAndTriggerCooldown(err);
            setFeedback({
                type: 'error',
                message: `${t('settings.syncProbeError')}: ${translateError(err, t)}`,
            });
        } finally {
            setCheckingPublicUrl(null);
        }
    };

    const handleProbeContactSyncs = async () => {
        setIsProbingContacts(true);
        setFeedback(null);
        try {
            await marketService.probeContactSyncs(true);
            const refetched = await marketService.discoverAvailableSyncs().catch(() => []);
            setDiscoveredDrops(refetched);
            notifySyncStatusUpdated();
            setFeedback({
                type: 'success',
                message: t('settings.syncProbeSuccess', 'Sync-Dienste erfolgreich geprüft.'),
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsProbingContacts(false);
        }
    };

    const handleToggleDiscoveredDrop = async (discovered: FrontendDiscoveredSync) => {
        try {
            const currentSelected = discoveredDrops.filter(d => d.isSelected).map(d => d.descriptor);
            const isCurrentlySelected = discovered.isSelected;

            let descriptorToActivate = discovered.descriptor;

            const currentStatus = discovered.status || discovered.descriptor.status;
            if (!isCurrentlySelected && currentStatus !== 'active' && currentStatus !== 'deprecating') {
                setIsProbingContacts(true);
                try {
                    await marketService.probeContactSyncs(true);
                    const fresh = await marketService.discoverAvailableSyncs().catch(() => []);
                    setDiscoveredDrops(fresh);
                    notifySyncStatusUpdated();
                    const match = fresh.find(d => isSameSyncEndpoint(d.descriptor, discovered.descriptor));
                    const freshStatus = match ? (match.status || match.descriptor.status) : undefined;
                    if (!match || (freshStatus !== 'active' && freshStatus !== 'deprecating')) {
                        setFeedback({
                            type: 'info',
                            message: t('settings.syncStatusPendingAuthDesc', 'Dieser Dienst wartet noch auf die Freigabe durch den Kontakt.'),
                        });
                        return;
                    }
                    descriptorToActivate = match.descriptor;
                } finally {
                    setIsProbingContacts(false);
                }
            }

            let updatedList: FrontendSyncDescriptor[] = [];
            if (isCurrentlySelected) {
                updatedList = currentSelected.filter(d => !isSameSyncEndpoint(d, discovered.descriptor));
            } else {
                updatedList = [...currentSelected.filter(d => !isSameSyncEndpoint(d, descriptorToActivate)), descriptorToActivate];
            }
            await marketService.setSelectedUploadSyncs(updatedList);
            const refetched = await marketService.discoverAvailableSyncs().catch(() => []);
            setDiscoveredDrops(refetched);
            notifySyncStatusUpdated();
            if (!isCurrentlySelected) {
                setTimeout(() => {
                    runSyncBackupPass(true).catch(() => {});
                }, 1000);
            }
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

    const handleCopyRustConfig = async (useWizardPolicy: boolean) => {
        setIsCopyingRustConfig(true);
        setFeedback(null);
        try {
            await protectAction(async (pwd) => {
                let toml = '';
                if (useWizardPolicy) {
                    const draft = await marketService.prepareSyncSetup({
                        policy: hostPolicy,
                        maxMembers: hostMaxMembers,
                        maxTotalStorageMb: hostMaxStorageMb,
                    }, pwd || undefined);
                    toml = draft?.rustConfigToml || '';
                }
                if (!toml) {
                    toml = await marketService.getRustSyncConfig(pwd || undefined);
                }
                await copyToClipboard(toml);
            });
            setFeedback({
                type: 'success',
                message: t('settings.syncHostRustConfigCopied'),
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsCopyingRustConfig(false);
        }
    };

    const handleTestAndActivateHost = async () => {
        const trimmed = hostWebspaceUrl.trim();
        if (!trimmed) return;
        const normalized = normalizeSyncUrlInput(trimmed);
        if (normalized && normalized !== hostWebspaceUrl) {
            setHostWebspaceUrl(normalized);
        }
        const targetUrl = normalized || trimmed;

        setIsTestingHost(true);
        setFeedback(null);
        try {
            await protectAction(async (pwd) => {
                const testRes = await marketService.testSyncConnection(targetUrl, pwd || undefined);
                if (!testRes.success || !testRes.writeOk || !testRes.readOk) {
                    throw new Error(testRes.message || 'Verbindung zum Webspace konnte nicht verifiziert werden.');
                }
                const admin = await marketService.finalizeSyncSetup(targetUrl, pwd || undefined);
                setAdminStatus(admin);
                setSessionDraft(HOST_SYNC_DRAFT_KEY, '');
                notifySyncStatusUpdated();
                setTimeout(() => {
                    runSyncBackupPass(true).catch(() => {});
                }, 1000);
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

    const handleRevokeServer = async (clearRevocation: boolean) => {
        setIsRevokingServer(true);
        try {
            await protectAction(async (pwd) => {
                const nowSec = Math.floor(Date.now() / 1000);
                const days = parseInt(revokeGraceDays, 10) || 0;
                const effectiveAt = clearRevocation ? null : (days > 0 ? nowSec + days * 86400 : nowSec);
                const updated = await marketService.revokeOwnSyncServer(
                    effectiveAt,
                    clearRevocation ? undefined : revokeReason.trim() || undefined,
                    clearRevocation ? undefined : revokeReplacementUrl.trim() || undefined,
                    pwd || undefined
                );
                if (updated) {
                    setAdminStatus(updated);
                }
                setIsRevokeModalOpen(false);
                notifySyncStatusUpdated();
                setFeedback({
                    type: 'success',
                    message: clearRevocation
                        ? t('settings.syncHostRevokeCleared', 'Abkündigung für Sync-Dienst wurde aufgehoben.')
                        : t('settings.syncHostRevokeSaved', 'Abkündigung für Sync-Dienst wurde veröffentlicht.'),
                });
            });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsRevokingServer(false);
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

                {activeTab === 'connected' && (
                    <InfoHint topic="connection" />
                )}

                {feedback && (
                    <div className={`p-3 rounded-xl border text-xs flex items-center gap-2 ${
                        feedback.type === 'success'
                            ? 'bg-emerald-50 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800'
                            : feedback.type === 'info'
                            ? 'bg-amber-50 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800'
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
                        {(() => {
                            const revokedList = [
                                ...discoveredDrops
                                    .filter((d) => {
                                        const st = d.status || d.descriptor.status;
                                        return st === 'deprecating' || st === 'revoked';
                                    })
                                    .map((d) => ({
                                        url: d.descriptor.writeUrl || d.descriptor.readUrl,
                                        status: (d.status || d.descriptor.status) as string,
                                        sunsetAt: d.descriptor.sunsetAt,
                                        reason: d.descriptor.revocationReason,
                                        replacement: d.descriptor.replacement,
                                    })),
                                ...configuredPublicDrops
                                    .filter((p) => p.status === 'deprecating' || p.status === 'revoked')
                                    .map((p) => ({
                                        url: p.writeUrl || p.readUrl,
                                        status: p.status as string,
                                        sunsetAt: p.sunsetAt,
                                        reason: p.revocationReason,
                                        replacement: p.replacement,
                                    })),
                            ];
                            if (revokedList.length === 0) return null;
                            return (
                                <div className="p-3.5 bg-amber-50 border border-amber-300 rounded-2xl space-y-2 text-xs text-amber-900">
                                    <div className="font-bold flex items-center gap-2">
                                        <AlertTriangle size={15} className="text-amber-600 shrink-0" />
                                        <span>{t('settings.syncRevocationBannerTitle', 'Abschaltung eines Sync-Dienstes angekündigt')}</span>
                                    </div>
                                    {revokedList.map((item, idx) => (
                                        <div key={item.url + idx} className="pt-1 border-t border-amber-200/70 space-y-1">
                                            <div className="font-mono text-[11px] break-all font-semibold">{item.url}</div>
                                            <div>
                                                {item.status === 'revoked'
                                                    ? t('settings.syncStatusRevoked', 'Geschlossen')
                                                    : item.sunsetAt
                                                    ? t('settings.syncStatusDeprecatingDate', {
                                                          date: new Date(item.sunsetAt * 1000).toLocaleDateString(),
                                                          defaultValue: `Schließt am ${new Date(item.sunsetAt * 1000).toLocaleDateString()}`,
                                                      })
                                                    : t('settings.syncStatusDeprecating', 'Wird bald geschlossen')}
                                                {item.reason ? ` — ${item.reason}` : ''}
                                            </div>
                                            {item.replacement?.writeUrl && (
                                                <div className="flex flex-wrap items-center gap-2 pt-1">
                                                    <span className="font-mono text-[11px] break-all">
                                                        → {item.replacement.writeUrl}
                                                    </span>
                                                    <Button
                                                        type="button"
                                                        size="sm"
                                                        variant="secondary"
                                                        onClick={() => setInputUrl(item.replacement!.writeUrl)}
                                                        className="text-xs"
                                                    >
                                                        {t('settings.syncUseReplacement', 'Neuen Sync-Dienst übernehmen')}
                                                    </Button>
                                                </div>
                                            )}
                                        </div>
                                    ))}
                                </div>
                            );
                        })()}
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
                                        checked={marketSyncEnabled}
                                        onChange={(e) => {
                                            setMarketSyncEnabled(e.target.checked);
                                            updateFeatureToggle('marketSyncEnabled', e.target.checked);
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

                                {/* Share Min Level for Card / Offers */}
                                <div className="pt-3 border-t border-theme-subtle/50 flex flex-col gap-1.5">
                                    <div className="flex items-center justify-between gap-3">
                                        <label htmlFor="shareMinLevel" className="text-xs font-bold text-theme-primary cursor-pointer">
                                            {t('settings.shareMinLevel')}
                                        </label>
                                        <select
                                            id="shareMinLevel"
                                            value={shareMinLevel}
                                            onChange={(e) => {
                                                const val = Number(e.target.value);
                                                setShareMinLevel(val);
                                                updateSettingNumber('shareMinLevel', val);
                                            }}
                                            className="h-8 px-2.5 bg-bg-app border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20 cursor-pointer"
                                        >
                                            <option value={0}>{t('contacts.trustLevel0')}</option>
                                            <option value={1}>{t('contacts.trustLevel1')}</option>
                                            <option value={2}>{t('contacts.trustLevel2')}</option>
                                            <option value={3}>{t('contacts.trustLevel3')}</option>
                                        </select>
                                    </div>
                                    <p className="text-[11px] text-theme-light">
                                        {t('settings.shareMinLevelDesc')}
                                    </p>
                                </div>

                                {/* Sync Services Poll Wifi Only */}
                                <label className="flex items-start gap-3 cursor-pointer group pt-3 border-t border-theme-subtle/50">
                                    <input
                                        type="checkbox"
                                        checked={marketPollWifiOnly}
                                        onChange={(e) => {
                                            setMarketPollWifiOnly(e.target.checked);
                                            updateFeatureToggle('marketPollWifiOnly', e.target.checked);
                                        }}
                                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                                    />
                                    <div>
                                        <span className="text-xs font-bold text-theme-primary group-hover:text-theme-accent">
                                            {t('settings.marketPollWifiOnly')}
                                        </span>
                                        <p className="text-[11px] text-theme-light mt-0.5">
                                            {t('settings.marketPollWifiOnlyDesc')}
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

                            <div className="flex flex-col gap-2">
                                <div className="flex flex-col sm:flex-row gap-2">
                                    <Input
                                        value={inputUrl}
                                        onChange={(e) => setInputUrl(e.target.value)}
                                        onBlur={() => {
                                            if (inputUrl.trim()) {
                                                const norm = normalizeSyncUrlInput(inputUrl);
                                                if (norm && norm !== inputUrl) {
                                                    setInputUrl(norm);
                                                }
                                            }
                                        }}
                                        placeholder={t('settings.syncAddPlaceholder')}
                                        className="text-xs flex-1 font-mono"
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

                                {inputUrl.trim() && (
                                    <div className="flex items-center gap-1.5 text-[11px] text-theme-light pl-1">
                                        <span className="font-semibold text-theme-secondary">{t('settings.syncDetectedUrl')}</span>
                                        <span className="font-mono text-theme-primary bg-bg-app px-2 py-0.5 rounded-lg border border-theme-subtle break-all">
                                            {normalizeSyncUrlInput(inputUrl)}
                                        </span>
                                    </div>
                                )}

                                <div className="flex items-center flex-wrap gap-2 pt-0.5">
                                    <button
                                        type="button"
                                        onClick={() => setInputUrl(COMMUNITY_SYNC_PRESET_URL)}
                                        className="text-[11px] text-theme-primary hover:underline flex items-center gap-1 font-medium"
                                    >
                                        <Globe size={12} />
                                        <span>{t('settings.syncCommunityQuickSelect')}</span>
                                    </button>
                                </div>
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
                            <div className="flex items-center justify-between gap-2">
                                <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                                    <Users size={14} className="text-theme-primary" />
                                    <span>{t('settings.syncDiscoveredTitle')}</span>
                                </h4>
                                <Button
                                    size="xs"
                                    variant="outline"
                                    disabled={isProbingContacts}
                                    onClick={handleProbeContactSyncs}
                                    className="text-[11px] h-7 px-2 flex items-center gap-1"
                                    data-testid="probe-contact-syncs-btn"
                                >
                                    <RefreshCw size={12} className={isProbingContacts ? 'animate-spin' : ''} />
                                    <span>{t('settings.syncProbeNow', 'Jetzt prüfen')}</span>
                                </Button>
                            </div>

                            {(discoveredDrops || []).length === 0 ? (
                                <div className="p-4 bg-bg-app rounded-2xl border border-theme-subtle text-xs text-theme-light text-center leading-relaxed">
                                    {t('settings.syncDiscoveredEmpty')}
                                </div>
                            ) : (
                                <div className="space-y-2">
                                    {(discoveredDrops || []).map((drop, idx) => {
                                        const status = drop.status || drop.descriptor.status || 'unverified';
                                        let badgeColor = 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300';
                                        let statusText = t('settings.syncStatusUnverified', 'Noch nicht geprüft');
                                        let statusTooltip: string | undefined = undefined;

                                        if (status === 'active') {
                                            if (drop.isSelected) {
                                                badgeColor = 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-300';
                                                statusText = t('settings.syncStatusConnected', 'Verbunden');
                                            } else {
                                                badgeColor = 'bg-emerald-50 text-emerald-700 border border-emerald-200 dark:bg-emerald-950/20 dark:text-emerald-300 dark:border-emerald-800';
                                                statusText = t('settings.syncStatusActiveInactive', 'Bereit (nicht aktiviert)');
                                            }
                                        } else if (status === 'pending_auth') {
                                            badgeColor = 'bg-amber-100 text-amber-800 border border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800';
                                            statusText = t('settings.syncStatusPendingAuthBadge', 'Erreichbar, nicht freigeschaltet');
                                            statusTooltip = t('settings.syncStatusPendingAuthDesc', 'Server erreichbar, aber dein Schlüssel ist beim Betreiber noch nicht freigeschaltet.');
                                        } else if (status === 'unreachable') {
                                            badgeColor = 'bg-rose-100 text-rose-800 border border-rose-200 dark:bg-rose-950/40 dark:text-rose-300 dark:border-rose-800';
                                            statusText = t('settings.syncStatusOffline', 'Nicht erreichbar (Offline)');
                                        } else if (status === 'deprecating') {
                                            badgeColor = 'bg-amber-100 text-amber-800 border border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800';
                                            const dateStr = drop.descriptor.sunsetAt
                                                ? new Date(drop.descriptor.sunsetAt * 1000).toLocaleDateString()
                                                : '';
                                            statusText = dateStr
                                                ? t('settings.syncStatusDeprecatingDate', { date: dateStr, defaultValue: `Schließt am ${dateStr}` })
                                                : t('settings.syncStatusDeprecating', 'Wird bald geschlossen');
                                            statusTooltip = drop.descriptor.revocationReason || undefined;
                                        } else if (status === 'revoked') {
                                            badgeColor = 'bg-rose-100 text-rose-800 border border-rose-200 dark:bg-rose-950/40 dark:text-rose-300 dark:border-rose-800';
                                            statusText = t('settings.syncStatusRevoked', 'Geschlossen');
                                            statusTooltip = drop.descriptor.revocationReason || undefined;
                                        }

                                        const displayUrl = drop.descriptor.writeUrl || drop.descriptor.readUrl;

                                        return (
                                            <div
                                                key={drop.descriptor.writeUrl || drop.descriptor.readUrl || idx}
                                                className="p-3 bg-white dark:bg-slate-900 rounded-xl border border-theme-subtle flex items-center justify-between gap-3 text-xs"
                                            >
                                                <div className="flex items-center gap-2.5 min-w-0">
                                                    <input
                                                        type="checkbox"
                                                        checked={drop.isSelected}
                                                        onChange={() => handleToggleDiscoveredDrop(drop)}
                                                        className="rounded border-theme-subtle text-theme-primary focus:ring-theme-primary cursor-pointer"
                                                        title={
                                                            status === 'active'
                                                                ? t('settings.syncActivateTooltip', 'Für Synchronisation aktivieren')
                                                                : t('settings.syncStatusPendingAuth', 'Wartet auf Freigabe durch Kontakt')
                                                        }
                                                    />
                                                    <div className="min-w-0">
                                                        <div className="flex items-center gap-1.5 min-w-0">
                                                            <span className="font-bold text-theme-primary truncate block">
                                                                {t('settings.syncServiceOf', {
                                                                    name: drop.sourceLabel || displayUrl,
                                                                })}
                                                            </span>
                                                            {drop.isSelected && status === 'active' && (
                                                                <span className="px-1.5 py-0.2 rounded text-[9px] font-semibold bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300 shrink-0">
                                                                    {t('settings.syncStatusConnected', 'Verbunden')}
                                                                </span>
                                                            )}
                                                        </div>
                                                        <span className="font-mono text-[10px] text-theme-light truncate block">
                                                            {displayUrl}
                                                        </span>
                                                    </div>
                                                </div>

                                                <span
                                                    className={`px-2 py-0.5 rounded-full text-[10px] font-bold shrink-0 ${badgeColor}`}
                                                    title={statusTooltip}
                                                >
                                                    {statusText}
                                                </span>
                                            </div>
                                        );
                                    })}
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
                                    {(configuredPublicDrops || []).map((drop, idx) => {
                                        const isChecking = checkingPublicUrl === drop.writeUrl;
                                        const isCopied = copiedPublicKeyUrl === drop.writeUrl;
                                        const hasRegisterUrl = isSafeExternalSyncUrl(drop.registerUrl);
                                        return (
                                            <div
                                                key={drop.writeUrl || idx}
                                                className="p-3 bg-white rounded-xl border border-theme-subtle space-y-2 text-xs"
                                            >
                                                <div className="flex items-center justify-between gap-3">
                                                    <div className="min-w-0">
                                                        <span className="font-mono text-xs text-theme-primary truncate block">
                                                            {drop.writeUrl}
                                                        </span>
                                                        <span className="text-[10px] text-theme-light block">
                                                            {(drop.authorizedChannels || []).length > 0
                                                                ? t('settings.syncPublicChannelsLabel', {
                                                                      channels: (drop.authorizedChannels || []).join(', '),
                                                                  })
                                                                : t('settings.syncPublicPendingKey')}
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
                                                            {drop.enabled
                                                                ? t('settings.syncPublicDisable')
                                                                : t('settings.syncPublicEnable')}
                                                        </Button>

                                                        <button
                                                            type="button"
                                                            onClick={() => handleOpenDeletePublicDrop(drop)}
                                                            className="text-theme-placeholder hover:text-rose-600 p-1 rounded transition-colors"
                                                            title={t('common.delete')}
                                                            aria-label={t('common.delete')}
                                                        >
                                                            <Trash2 size={15} />
                                                        </button>
                                                    </div>
                                                </div>

                                                <div className="flex flex-wrap items-center gap-2 pt-1 border-t border-theme-subtle/40">
                                                    {hasRegisterUrl && (
                                                        <Button
                                                            type="button"
                                                            variant="primary"
                                                            size="sm"
                                                            onClick={() => handleOpenPublicRegister(drop)}
                                                            className="gap-1.5 text-[11px] h-7 px-2.5"
                                                        >
                                                            <ExternalLink size={12} />
                                                            <span>{t('settings.syncPublicOpenRegister')}</span>
                                                        </Button>
                                                    )}

                                                    <Button
                                                        type="button"
                                                        variant="outline"
                                                        size="sm"
                                                        onClick={() => handleCopyPublicAccessKey(drop)}
                                                        className="gap-1.5 text-[11px] h-7 px-2.5"
                                                    >
                                                        {isCopied ? <Check size={12} className="text-emerald-600" /> : <Copy size={12} />}
                                                        <span>{t('settings.syncPublicCopyKey')}</span>
                                                    </Button>

                                                    <Button
                                                        type="button"
                                                        variant="outline"
                                                        size="sm"
                                                        onClick={() => handleCheckPublicDropAuth(drop)}
                                                        disabled={isChecking}
                                                        className="gap-1.5 text-[11px] h-7 px-2.5"
                                                    >
                                                        <RefreshCw size={12} className={isChecking ? 'animate-spin' : ''} />
                                                        <span>{t('settings.syncPublicCheckAuth')}</span>
                                                    </Button>
                                                </div>
                                            </div>
                                        );
                                    })}
                                </div>
                            )}
                        </div>
                    </div>
                )}

                {/* TAB 2: Host / Own Sync Service */}
                {activeTab === 'host' && (
                    <div className="space-y-6 animate-in fade-in duration-200">
                        {/* Community Service Banner */}
                        <div className="p-4 bg-emerald-500/10 dark:bg-emerald-950/20 rounded-2xl border border-emerald-500/20 space-y-1.5 text-xs text-theme-secondary">
                            <h4 className="font-bold text-emerald-800 dark:text-emerald-300 text-sm flex items-center gap-1.5">
                                <HeartHandshake size={16} />
                                <span>{t('settings.syncHostCommunityTitle')}</span>
                            </h4>
                            <p className="leading-relaxed text-emerald-900/90 dark:text-emerald-200/90 font-medium">
                                {t('settings.syncHostCommunityDesc')}
                            </p>
                        </div>

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

                                    <div className="pt-2 flex flex-wrap items-center gap-2">
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

                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() => handleCopyRustConfig(true)}
                                            disabled={isCopyingRustConfig}
                                            className="gap-2"
                                        >
                                            <FileCode size={14} className="text-theme-primary" />
                                            <span>{t('settings.syncHostCopyRustConfig')}</span>
                                        </Button>
                                    </div>

                                    <div className="p-3 bg-bg-app rounded-xl border border-theme-subtle space-y-1.5 text-[11px] text-theme-secondary">
                                        <p className="font-semibold text-theme-primary">
                                            {t('settings.syncHostNextStepsTitle', 'Next steps before entering the URL below:')}
                                        </p>
                                        <ul className="list-disc pl-4 space-y-1 leading-relaxed">
                                            <li>{t('settings.syncHostNextStepPhp', 'PHP webspace (humoco-sync-setup.zip): Extract the ZIP archive and upload its contents (sync.php & config) to your webspace directory via FTP/SFTP.')}</li>
                                            <li>{t('settings.syncHostNextStepRust', 'Rust server (config.toml): Save the copied configuration as config.toml on your server and start humoco-sync-server.')}</li>
                                            <li>{t('settings.syncHostNextStepVerify', 'Activate: Enter your server\'s public URL in Step 2 below and click "Test & Activate".')}</li>
                                        </ul>
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

                                    <p className="text-[11px] text-theme-light leading-relaxed">
                                        {t('settings.syncHostSetupStep2Desc')}
                                    </p>

                                    <div className="flex flex-col gap-2">
                                        <div className="flex flex-col sm:flex-row gap-2">
                                            <Input
                                                value={hostWebspaceUrl}
                                                onChange={(e) => setHostWebspaceUrl(e.target.value)}
                                                onBlur={() => {
                                                    if (hostWebspaceUrl.trim()) {
                                                        const norm = normalizeSyncUrlInput(hostWebspaceUrl);
                                                        if (norm && norm !== hostWebspaceUrl) {
                                                            setHostWebspaceUrl(norm);
                                                        }
                                                    }
                                                }}
                                                placeholder={t('settings.syncHostUrlPlaceholder')}
                                                className="text-xs flex-1 font-mono"
                                                onKeyDown={(e) => {
                                                    if (e.key === 'Enter') {
                                                        e.preventDefault();
                                                        handleTestAndActivateHost();
                                                    }
                                                }}
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

                                        {hostWebspaceUrl.trim() && (
                                            <div className="flex items-center gap-1.5 text-[11px] text-theme-light pl-1">
                                                <span className="font-semibold text-theme-secondary">{t('settings.syncDetectedUrl')}</span>
                                                <span className="font-mono text-theme-primary bg-bg-app px-2 py-0.5 rounded-lg border border-theme-subtle break-all">
                                                    {normalizeSyncUrlInput(hostWebspaceUrl)}
                                                </span>
                                            </div>
                                        )}
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

                                {/* Active Revocation Banner */}
                                {adminStatus.revocation && (
                                    <div className="p-3.5 bg-amber-50 border border-amber-300 rounded-2xl text-xs text-amber-900 space-y-2">
                                        <div className="font-semibold flex items-center justify-between gap-2">
                                            <span>
                                                {t('settings.syncHostRevokeActiveTitle', 'Abkündigung aktiv')} (
                                                {new Date(adminStatus.revocation.effective_at * 1000).toLocaleDateString()})
                                            </span>
                                            <Button
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                onClick={() => handleRevokeServer(true)}
                                                disabled={isRevokingServer}
                                                className="text-xs"
                                            >
                                                {t('settings.syncHostRevokeCancel', 'Abkündigung aufheben')}
                                            </Button>
                                        </div>
                                        {adminStatus.revocation.reason && (
                                            <p className="text-amber-800">{adminStatus.revocation.reason}</p>
                                        )}
                                        {adminStatus.revocation.replacement?.write_url && (
                                            <p className="font-mono text-[11px] break-all text-amber-800">
                                                → {adminStatus.revocation.replacement.write_url}
                                            </p>
                                        )}
                                    </div>
                                )}

                                {/* Danger / Management Actions */}
                                <div className="flex flex-wrap items-center gap-2 pt-2 border-t border-theme-subtle/50">
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => handleCopyRustConfig(false)}
                                        disabled={isCopyingRustConfig}
                                        className="gap-1.5 text-xs"
                                    >
                                        <FileCode size={13} />
                                        <span>{t('settings.syncHostCopyRustConfig')}</span>
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        onClick={() => setIsRevokeModalOpen(true)}
                                        className="gap-1.5 text-xs"
                                    >
                                        <AlertTriangle size={13} />
                                        <span>{t('settings.syncHostRevokeButton', 'Sync-Dienst abkündigen / umziehen')}</span>
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

                                {isRevokeModalOpen && (
                                    <div className="p-4 bg-amber-50/70 border border-amber-200 rounded-2xl space-y-3 text-xs">
                                        <h4 className="font-bold text-amber-900">
                                            {t('settings.syncHostRevokeModalTitle', 'Sync-Dienst abkündigen oder umziehen')}
                                        </h4>
                                        <div className="space-y-1">
                                            <label className="block font-semibold text-theme-secondary">
                                                {t('settings.syncHostRevokeGraceLabel', 'Übergangsfrist')}
                                            </label>
                                            <select
                                                value={revokeGraceDays}
                                                onChange={(e) => setRevokeGraceDays(e.target.value as '30' | '14' | '0')}
                                                className="w-full px-3 py-2 rounded-xl border border-theme-subtle bg-white text-xs"
                                            >
                                                <option value="30">{t('settings.syncHostRevoke30Days', '30 Tage Übergangsfrist (empfohlen)')}</option>
                                                <option value="14">{t('settings.syncHostRevoke14Days', '14 Tage Übergangsfrist')}</option>
                                                <option value="0">{t('settings.syncHostRevokeNow', 'Sofort abschalten')}</option>
                                            </select>
                                        </div>
                                        <div className="space-y-1">
                                            <label className="block font-semibold text-theme-secondary">
                                                {t('settings.syncHostRevokeReplacementLabel', 'Neue Sync-Dienst-Adresse (optional)')}
                                            </label>
                                            <input
                                                type="url"
                                                value={revokeReplacementUrl}
                                                onChange={(e) => setRevokeReplacementUrl(e.target.value)}
                                                placeholder="https://neu.example.org/sync.php"
                                                className="w-full px-3 py-2 rounded-xl border border-theme-subtle bg-white text-xs font-mono"
                                            />
                                        </div>
                                        <div className="space-y-1">
                                            <label className="block font-semibold text-theme-secondary">
                                                {t('settings.syncHostRevokeReasonLabel', 'Grund (optional)')}
                                            </label>
                                            <input
                                                type="text"
                                                maxLength={200}
                                                value={revokeReason}
                                                onChange={(e) => setRevokeReason(e.target.value)}
                                                placeholder={t('settings.syncHostRevokeReasonPlaceholder', 'z. B. Umzug auf neue Domain')}
                                                className="w-full px-3 py-2 rounded-xl border border-theme-subtle bg-white text-xs"
                                            />
                                        </div>
                                        <div className="flex items-center gap-2 pt-1">
                                            <Button
                                                type="button"
                                                variant="primary"
                                                size="sm"
                                                onClick={() => handleRevokeServer(false)}
                                                disabled={isRevokingServer}
                                                className="text-xs"
                                            >
                                                {isRevokingServer
                                                    ? t('common.saving', 'Speichern…')
                                                    : t('settings.syncHostRevokeConfirm', 'Abkündigung veröffentlichen')}
                                            </Button>
                                            <Button
                                                type="button"
                                                variant="outline"
                                                size="sm"
                                                onClick={() => setIsRevokeModalOpen(false)}
                                                disabled={isRevokingServer}
                                                className="text-xs"
                                            >
                                                {t('common.cancel', 'Abbrechen')}
                                            </Button>
                                        </div>
                                    </div>
                                )}
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

            {/* Confirmation dialog for removing a public sync service */}
            <ConfirmationModal
                isOpen={pendingPublicDelete !== null}
                title={t('settings.syncPublicDeleteConfirmTitle')}
                description={
                    <div className="space-y-3 text-sm text-theme-secondary">
                        <p>{t('settings.syncPublicDeleteConfirmDesc')}</p>
                        {pendingPublicDelete && (
                            <p className="font-mono text-xs bg-bg-app px-2.5 py-1.5 rounded-lg border border-theme-subtle break-all">
                                {pendingPublicDelete.writeUrl}
                            </p>
                        )}
                        <label className="flex items-start gap-2 cursor-pointer text-xs text-theme-primary pt-1">
                            <input
                                type="checkbox"
                                checked={deletePublicRemoteData}
                                onChange={(e) => setDeletePublicRemoteData(e.target.checked)}
                                className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary"
                            />
                            <span>{t('settings.syncPublicDeleteRemoteLabel')}</span>
                        </label>
                    </div>
                }
                confirmVariant="danger"
                confirmText={t('common.delete')}
                isProcessing={isDeletingPublicDrop}
                onConfirm={handleConfirmDeletePublicDrop}
                onCancel={() => {
                    if (!isDeletingPublicDrop) setPendingPublicDelete(null);
                }}
            />
        </Card>
    );
};

export default SyncServiceSettingsCard;
