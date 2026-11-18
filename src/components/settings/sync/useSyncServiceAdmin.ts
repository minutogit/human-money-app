// src/components/settings/sync/useSyncServiceAdmin.ts
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    AppSettings,
    FrontendDiscoveredSync,
    FrontendMemberPolicyDto,
    FrontendSyncAdminStatus,
    FrontendSyncDescriptor,
    StoredPublicSync,
} from '../../../types';
import { marketService } from '../../../services/marketService';
import { settingsService } from '../../../services/settingsService';
import { useSession } from '../../../context/SessionContext';
import { copyToClipboard } from '../../../utils/clipboard';
import { runSyncBackupPass } from '../../../sync/autoBackup';
import { notifySyncStatusUpdated, SYNC_STATUS_UPDATED_EVENT } from '../../../utils/syncEvents';
import { translateError } from '../../../utils/errorHelper';
import { useRateLimitCooldown } from '../../../hooks/useRateLimitCooldown';
import {
    ChannelSettingsState,
    CONSUMER_SYNC_DRAFT_KEY,
    HOST_SYNC_DRAFT_KEY,
    HostQuotasState,
    SyncFeedback,
    SyncProbeResult,
    buildChannelSettings,
    isSameSyncEndpoint,
    normalizeHostQuotas,
    resolveNormalizedSyncUrl,
    useSessionDraft,
} from './syncHelpers';

export interface UseSyncServiceAdminOptions {
    settings: AppSettings | null;
    onSettingsUpdated?: (newSettings: AppSettings) => void;
}

/**
 * Manages sync drop discovery, channel settings, public sync registration, and active hosted sync admin actions.
 */
export function useSyncServiceAdmin({ settings, onSettingsUpdated }: UseSyncServiceAdminOptions) {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [isLoading, setIsLoading] = useState(false);
    const [feedback, setFeedback] = useState<SyncFeedback | null>(null);

    const [discoveredDrops, setDiscoveredDrops] = useState<FrontendDiscoveredSync[]>([]);
    const [configuredPublicDrops, setConfiguredPublicDrops] = useState<StoredPublicSync[]>([]);
    const [inputUrl, setInputUrl] = useSessionDraft(CONSUMER_SYNC_DRAFT_KEY);
    const [isProbing, setIsProbing] = useState(false);
    const [isProbingContacts, setIsProbingContacts] = useState(false);
    const [probeResult, setProbeResult] = useState<SyncProbeResult | null>(null);
    const { cooldownSeconds, isCoolingDown, checkAndTriggerCooldown } = useRateLimitCooldown();

    const [channelSettings, setChannelSettings] = useState<ChannelSettingsState>(() =>
        buildChannelSettings(settings)
    );
    const [isDeletingPublicDrop, setIsDeletingPublicDrop] = useState(false);
    const [checkingPublicUrl, setCheckingPublicUrl] = useState<string | null>(null);

    const [adminStatus, setAdminStatus] = useState<FrontendSyncAdminStatus | null>(null);
    const [hostPolicy, setHostPolicy] = useState<FrontendMemberPolicyDto>({
        directMin: 2,
        hop2Enabled: true,
        bridgeMin: 2,
        hop2Min: 2,
    });
    const [hostQuotas, setHostQuotas] = useState<HostQuotasState>({
        maxMembers: 300,
        maxTotalStorageMb: 2048,
        maxUserStorageMb: 25,
    });
    const [hostWebspaceUrl, setHostWebspaceUrl] = useSessionDraft(HOST_SYNC_DRAFT_KEY);
    const [isCopyingRustConfig, setIsCopyingRustConfig] = useState(false);
    const [isUpdatingPolicy, setIsUpdatingPolicy] = useState(false);
    const [isDeletingAdmin, setIsDeletingAdmin] = useState(false);
    const [isRevokingServer, setIsRevokingServer] = useState(false);

    useEffect(() => {
        if (!settings) return;
        setChannelSettings(buildChannelSettings(settings));
        if (Array.isArray(settings.publicSyncs)) {
            setConfiguredPublicDrops(settings.publicSyncs);
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
            if (admin?.cardPolicy) setHostPolicy(admin.cardPolicy);
            if (admin) {
                setHostQuotas({
                    maxMembers: admin.maxMembers || 300,
                    maxTotalStorageMb: admin.maxTotalStorageMb || 2048,
                    maxUserStorageMb: admin.maxUserStorageMb || 25,
                });
            }
            if (Array.isArray(s?.publicSyncs)) {
                setConfiguredPublicDrops(s.publicSyncs);
            } else if (!settings?.publicSyncs) {
                setConfiguredPublicDrops([]);
            }

            if (discList.some((d) => d.status === 'unverified') && typeof marketService.probeContactSyncs === 'function') {
                marketService.probeContactSyncs(false).then(() => {
                    marketService.discoverAvailableSyncs().then((refreshed) => {
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

    const runAction = async (
        setBusy: ((busy: boolean) => void) | null,
        fn: () => Promise<boolean | void>
    ): Promise<boolean> => {
        setBusy?.(true);
        setFeedback(null);
        try {
            const result = await fn();
            return result !== false;
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
            return false;
        } finally {
            setBusy?.(false);
        }
    };

    const runProtected = (
        setBusy: ((busy: boolean) => void) | null,
        fn: (pwd: string | undefined) => Promise<boolean | void>
    ) =>
        runAction(setBusy, async () => {
            let ok = false;
            await protectAction(async (pwd) => {
                ok = (await fn(pwd || undefined)) !== false;
            });
            return ok;
        });

    const refreshDiscoveredDrops = async () => {
        const fresh = await marketService.discoverAvailableSyncs().catch(() => []);
        setDiscoveredDrops(fresh);
        notifySyncStatusUpdated();
        return fresh;
    };

    const updateChannelSetting = async <K extends keyof ChannelSettingsState>(
        key: K,
        value: ChannelSettingsState[K]
    ) => {
        setChannelSettings((prev) => ({ ...prev, [key]: value }));
        try {
            const current = settings || await settingsService.getSettings();
            const updated: AppSettings = { ...current, [key]: value };
            await protectAction(async (pwd) => {
                await settingsService.saveSettings(updated, pwd || undefined);
            });
            onSettingsUpdated?.(updated);
            notifySyncStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const refreshPublicSyncSettings = async () => {
        const s = await settingsService.getSettings();
        if (s.publicSyncs) setConfiguredPublicDrops(s.publicSyncs);
        onSettingsUpdated?.(s);
        notifySyncStatusUpdated();
    };

    const probeAndSavePublicSync = async (targetUrl: string, fallbackRegisterUrl?: string | null) => {
        const info = await marketService.probePublicSync(targetUrl);
        if (!info) return null;
        const hasAuth = (info.authorizedChannels || []).length > 0;
        await marketService.savePublicSync({
            writeUrl: info.writeUrl,
            readUrl: info.readUrl,
            dropPub: info.dropPub,
            registerUrl: info.registerUrl ?? fallbackRegisterUrl,
            enabled: hasAuth,
        });
        await refreshPublicSyncSettings();
        return { info, hasAuth };
    };

    const handleProbeAndAdd = async () => {
        const targetUrl = resolveNormalizedSyncUrl(inputUrl, setInputUrl);
        if (!targetUrl) return;
        setIsProbing(true);
        setProbeResult(null);
        try {
            const res = await probeAndSavePublicSync(targetUrl);
            if (res) {
                setInputUrl('');
                if (res.hasAuth) {
                    setProbeResult({ status: 'success', message: t('settings.syncProbeSuccess') });
                    if (res.info.authorizedChannels.includes('backup')) {
                        setTimeout(() => {
                            runSyncBackupPass(true).catch(() => {});
                        }, 1000);
                    }
                } else {
                    setProbeResult({ status: 'unauthorized', message: t('settings.syncProbeUnauthorized') });
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

    const handleTogglePublicDrop = (drop: StoredPublicSync) =>
        runAction(null, async () => {
            await marketService.savePublicSync({
                writeUrl: drop.writeUrl,
                readUrl: drop.readUrl,
                dropPub: drop.dropPub,
                registerUrl: drop.registerUrl,
                enabled: !drop.enabled,
            });
            await refreshPublicSyncSettings();
        });

    const handleRemovePublicSync = (writeUrl: string, deleteRemoteSlots: boolean) =>
        runAction(setIsDeletingPublicDrop, async () => {
            await marketService.removePublicSync(writeUrl, deleteRemoteSlots);
            await refreshPublicSyncSettings();
            setFeedback({ type: 'success', message: t('settings.syncPublicRemovedSuccess') });
        });

    const handleCheckPublicDropAuth = async (drop: StoredPublicSync) => {
        setCheckingPublicUrl(drop.writeUrl);
        setFeedback(null);
        try {
            const res = await probeAndSavePublicSync(drop.writeUrl, drop.registerUrl);
            if (res) {
                setFeedback({
                    type: res.hasAuth ? 'success' : 'info',
                    message: res.hasAuth ? t('settings.syncProbeSuccess') : t('settings.syncProbeUnauthorized'),
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

    const handleProbeContactSyncs = () =>
        runAction(setIsProbingContacts, async () => {
            await marketService.probeContactSyncs(true);
            await refreshDiscoveredDrops();
            setFeedback({
                type: 'success',
                message: t('settings.syncProbeSuccess', 'Sync-Dienste erfolgreich geprüft.'),
            });
        });

    const handleToggleDiscoveredDrop = async (discovered: FrontendDiscoveredSync) => {
        try {
            const currentSelected = discoveredDrops.filter((d) => d.isSelected).map((d) => d.descriptor);
            const isCurrentlySelected = discovered.isSelected;
            let descriptorToActivate = discovered.descriptor;

            const currentStatus = discovered.status || discovered.descriptor.status;
            if (!isCurrentlySelected && currentStatus !== 'active' && currentStatus !== 'deprecating') {
                setIsProbingContacts(true);
                try {
                    await marketService.probeContactSyncs(true);
                    const fresh = await refreshDiscoveredDrops();
                    const match = fresh.find((d) => isSameSyncEndpoint(d.descriptor, discovered.descriptor));
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

            const updatedList: FrontendSyncDescriptor[] = isCurrentlySelected
                ? currentSelected.filter((d) => !isSameSyncEndpoint(d, discovered.descriptor))
                : [...currentSelected.filter((d) => !isSameSyncEndpoint(d, descriptorToActivate)), descriptorToActivate];

            await marketService.setSelectedUploadSyncs(updatedList);
            await refreshDiscoveredDrops();
            if (!isCurrentlySelected) {
                setTimeout(() => {
                    runSyncBackupPass(true).catch(() => {});
                }, 1000);
            }
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleCopyRustConfig = (useWizardPolicy: boolean) =>
        runProtected(setIsCopyingRustConfig, async (pwd) => {
            let toml = '';
            if (useWizardPolicy) {
                const draft = await marketService.prepareSyncSetup(
                    { policy: hostPolicy, ...normalizeHostQuotas(hostQuotas) },
                    pwd
                );
                toml = draft?.rustConfigToml || '';
            }
            if (!toml) toml = await marketService.getRustSyncConfig(pwd);
            await copyToClipboard(toml);
            setFeedback({ type: 'success', message: t('settings.syncHostRustConfigCopied') });
        });

    const handleUpdateHostPolicy = () =>
        runProtected(setIsUpdatingPolicy, async (pwd) => {
            const updated = await marketService.updateSyncAdminConfig(
                { cardPolicy: hostPolicy, ...normalizeHostQuotas(hostQuotas) },
                true,
                pwd
            );
            setAdminStatus(updated);
            setHostQuotas({
                maxMembers: updated.maxMembers,
                maxTotalStorageMb: updated.maxTotalStorageMb,
                maxUserStorageMb: updated.maxUserStorageMb,
            });
            setFeedback({ type: 'success', message: t('settings.syncHostPolicyUpdated') });
        });

    const handleDeleteHost = () =>
        runProtected(setIsDeletingAdmin, async (pwd) => {
            await marketService.deleteSyncAdmin(pwd);
            setAdminStatus(null);
            notifySyncStatusUpdated();
            setFeedback({ type: 'success', message: t('settings.syncHostDeletedSuccess') });
        });

    const handleRevokeServer = (opts: {
        clearRevocation: boolean;
        graceDays?: '30' | '14' | '0';
        reason?: string;
        replacementUrl?: string;
    }) =>
        runProtected(setIsRevokingServer, async (pwd) => {
            const nowSec = Math.floor(Date.now() / 1000);
            const days = parseInt(opts.graceDays || '0', 10) || 0;
            const effectiveAt = opts.clearRevocation ? null : days > 0 ? nowSec + days * 86400 : nowSec;
            const updated = await marketService.revokeOwnSyncServer(
                effectiveAt,
                opts.clearRevocation ? undefined : opts.reason?.trim() || undefined,
                opts.clearRevocation ? undefined : opts.replacementUrl?.trim() || undefined,
                pwd
            );
            if (updated) setAdminStatus(updated);
            notifySyncStatusUpdated();
            setFeedback({
                type: 'success',
                message: opts.clearRevocation
                    ? t('settings.syncHostRevokeCleared', 'Abkündigung für Sync-Dienst wurde aufgehoben.')
                    : t('settings.syncHostRevokeSaved', 'Abkündigung für Sync-Dienst wurde veröffentlicht.'),
            });
        });

    return {
        isLoading, feedback, setFeedback,
        discoveredDrops, configuredPublicDrops,
        inputUrl, setInputUrl, isProbing, isProbingContacts, probeResult,
        cooldownSeconds, isCoolingDown,
        channelSettings, updateChannelSetting,
        isDeletingPublicDrop, checkingPublicUrl,
        adminStatus, setAdminStatus,
        hostPolicy, setHostPolicy, hostQuotas, setHostQuotas,
        hostWebspaceUrl, setHostWebspaceUrl,
        isCopyingRustConfig, isUpdatingPolicy, isDeletingAdmin, isRevokingServer,
        handleProbeAndAdd, handleTogglePublicDrop, handleRemovePublicSync,
        handleCheckPublicDropAuth, handleProbeContactSyncs, handleToggleDiscoveredDrop,
        handleCopyRustConfig, handleUpdateHostPolicy, handleDeleteHost, handleRevokeServer,
    };
}
