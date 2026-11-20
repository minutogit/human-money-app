// src/components/settings/sync/useSyncServiceAdmin.ts
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    AppConfigDto,
    FrontendDiscoveredSync,
    FrontendMemberPolicyDto,
    FrontendSyncAdminStatus,
    FrontendSyncDescriptor,
    StoredPublicSync,
} from '../../../types';
import { marketService } from '../../../services/marketService';
import { settingsService } from '../../../services/settingsService';
import { protectCommandOptions } from '../../../services/commandPolicy';
import { useSession, type ProtectActionOptions } from '../../../context/SessionContext';
import { copyToClipboard } from '../../../utils/clipboard';
import { runSyncBackupPass } from '../../../sync/syncBridge';
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
    config: AppConfigDto | null;
    onConfigUpdated?: (newConfig: AppConfigDto) => void;
}

/**
 * Manages sync drop discovery, channel settings, public sync registration, and active hosted sync admin actions.
 */
export function useSyncServiceAdmin({ config, onConfigUpdated }: UseSyncServiceAdminOptions) {
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
        buildChannelSettings(config?.uiPreferences)
    );
    const [isDeletingPublicDrop, setIsDeletingPublicDrop] = useState(false);
    const [deletePublicDropError, setDeletePublicDropError] = useState<string | null>(null);
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
        if (!config) return;
        setChannelSettings(buildChannelSettings(config.uiPreferences));
        if (Array.isArray(config.securityPolicy.publicSyncs)) {
            setConfiguredPublicDrops(config.securityPolicy.publicSyncs);
        }
    }, [config]);

    const loadData = useCallback(async () => {
        setIsLoading(true);
        try {
            const [discovered, admin, freshConfig] = await Promise.all([
                marketService.discoverAvailableSyncs().catch(() => []),
                marketService.getSyncAdminStatus().catch(() => null),
                settingsService.getConfig().catch(() => null),
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
            if (Array.isArray(freshConfig?.securityPolicy.publicSyncs)) {
                setConfiguredPublicDrops(freshConfig.securityPolicy.publicSyncs);
            } else if (!config?.securityPolicy.publicSyncs) {
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
    }, [config]);

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
        fn: (pwd: string | undefined) => Promise<boolean | void>,
        options?: ProtectActionOptions,
    ) =>
        runAction(setBusy, async () => {
            let ok = false;
            await protectAction(async (pwd) => {
                ok = (await fn(pwd || undefined)) !== false;
            }, options);
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
            const updatedUi = await protectAction(async (pwd) => {
                return await settingsService.patchUiPreferences(
                    { [key]: value },
                    pwd || undefined,
                );
            });
            if (updatedUi && config) {
                onConfigUpdated?.({ ...config, uiPreferences: updatedUi });
            }
            notifySyncStatusUpdated();
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const refreshPublicSyncSettings = async () => {
        const freshConfig = await settingsService.getConfig();
        if (freshConfig.securityPolicy.publicSyncs) {
            setConfiguredPublicDrops(freshConfig.securityPolicy.publicSyncs);
        }
        onConfigUpdated?.(freshConfig);
        notifySyncStatusUpdated();
    };

    const probeAndSavePublicSync = async (
        targetUrl: string,
        fallbackRegisterUrl?: string | null,
        password?: string
    ) => {
        const info = await marketService.probePublicSync(targetUrl, password);
        if (!info) return null;
        const hasAuth = (info.authorizedChannels || []).length > 0;
        await marketService.savePublicSync(
            {
                writeUrl: info.writeUrl,
                readUrl: info.readUrl,
                dropPub: info.dropPub,
                registerUrl: info.registerUrl ?? fallbackRegisterUrl,
                enabled: hasAuth,
            },
            password
        );
        await refreshPublicSyncSettings();
        return { info, hasAuth };
    };

    const handleProbeAndAdd = async () => {
        const targetUrl = resolveNormalizedSyncUrl(inputUrl, setInputUrl);
        if (!targetUrl) return;
        setIsProbing(true);
        setProbeResult(null);
        try {
            const res = await protectAction(async (pwd) => {
                return await probeAndSavePublicSync(targetUrl, undefined, pwd || undefined);
            }, protectCommandOptions('save_public_sync'));
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
        runProtected(null, async (pwd) => {
            await marketService.savePublicSync(
                {
                    writeUrl: drop.writeUrl,
                    readUrl: drop.readUrl,
                    dropPub: drop.dropPub,
                    registerUrl: drop.registerUrl,
                    enabled: !drop.enabled,
                },
                pwd
            );
            await refreshPublicSyncSettings();
        }, protectCommandOptions('save_public_sync'));

    const handleRemovePublicSync = async (writeUrl: string, deleteRemoteSlots: boolean) => {
        setDeletePublicDropError(null);
        setIsDeletingPublicDrop(true);
        setFeedback(null);
        try {
            let completed = false;
            await protectAction(async (pwd) => {
                await marketService.removePublicSync(writeUrl, deleteRemoteSlots, pwd || undefined);
                await refreshPublicSyncSettings();
                setFeedback({ type: 'success', message: t('settings.syncPublicRemovedSuccess') });
                completed = true;
            }, protectCommandOptions('remove_public_sync'));
            return completed;
        } catch (err) {
            const msg = translateError(err, t);
            setDeletePublicDropError(msg);
            setFeedback({ type: 'error', message: msg });
            return false;
        } finally {
            setIsDeletingPublicDrop(false);
        }
    };

    const handleAdoptReplacementUrl = async (targetUrl: string) => {
        const normalized = resolveNormalizedSyncUrl(targetUrl, () => {});
        if (!normalized) return;
        setIsProbing(true);
        setFeedback(null);
        try {
            const res = await protectAction(async (pwd) => {
                return await probeAndSavePublicSync(normalized, undefined, pwd || undefined);
            }, protectCommandOptions('save_public_sync'));
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
            setIsProbing(false);
        }
    };

    const handleCheckPublicDropAuth = async (drop: StoredPublicSync) => {
        setCheckingPublicUrl(drop.writeUrl);
        setFeedback(null);
        try {
            const res = await protectAction(async (pwd) => {
                return await probeAndSavePublicSync(drop.writeUrl, drop.registerUrl, pwd || undefined);
            }, protectCommandOptions('save_public_sync'));
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

            await protectAction(async (pwd) => {
                await marketService.setSelectedUploadSyncs(updatedList, pwd || undefined);
            }, protectCommandOptions('set_selected_upload_syncs'));
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
        }, protectCommandOptions('get_rust_sync_config'));

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
        }, protectCommandOptions('update_sync_admin_config'));

    const handleDeleteHost = () =>
        runProtected(setIsDeletingAdmin, async (pwd) => {
            await marketService.deleteSyncAdmin(pwd);
            setAdminStatus(null);
            notifySyncStatusUpdated();
            setFeedback({ type: 'success', message: t('settings.syncHostDeletedSuccess') });
        }, protectCommandOptions('delete_sync_admin'));

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
        }, protectCommandOptions('revoke_own_sync_server'));

    return {
        isLoading, feedback, setFeedback,
        discoveredDrops, configuredPublicDrops,
        inputUrl, setInputUrl, isProbing, isProbingContacts, probeResult,
        cooldownSeconds, isCoolingDown,
        channelSettings, updateChannelSetting,
        isDeletingPublicDrop, deletePublicDropError, clearDeletePublicDropError: () => setDeletePublicDropError(null),
        checkingPublicUrl,
        adminStatus, setAdminStatus,
        hostPolicy, setHostPolicy, hostQuotas, setHostQuotas,
        hostWebspaceUrl, setHostWebspaceUrl,
        isCopyingRustConfig, isUpdatingPolicy, isDeletingAdmin, isRevokingServer,
        handleProbeAndAdd, handleTogglePublicDrop, handleRemovePublicSync, handleAdoptReplacementUrl,
        handleCheckPublicDropAuth, handleProbeContactSyncs, handleToggleDiscoveredDrop,
        handleCopyRustConfig, handleUpdateHostPolicy, handleDeleteHost, handleRevokeServer,
    };
}
