// src/components/settings/AutoSyncSummaryCard.tsx
import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Cloud, RefreshCw, CheckCircle2, AlertTriangle, Wifi, Image, ExternalLink, Copy } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { AppSettings, FrontendDiscoveredSync, StoredPublicSync } from '../../types';
import { marketService } from '../../services/marketService';
import { settingsService } from '../../services/settingsService';
import { useOptionalSession } from '../../context/SessionContext';
import { notifySyncStatusUpdated, SYNC_STATUS_UPDATED_EVENT } from '../../utils/syncEvents';
import { copyToClipboard } from '../../utils/clipboard';
import { translateError } from '../../utils/errorHelper';
import { collectRevokedNotices } from './sync/syncHelpers';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';

const COMMUNITY_SYNC_PRESET_URL = 'https://sync.menschlich-miteinander.org/sync.php';

function extractHostname(rawUrl?: string | null): string | null {
    if (!rawUrl) return null;
    try {
        return new URL(rawUrl).hostname || rawUrl;
    } catch {
        return rawUrl;
    }
}

export interface AutoSyncSummaryCardProps {
    settings: AppSettings | null;
    onPersistSettings: (updater: (prev: AppSettings | null) => AppSettings | null) => void;
    onSettingsSynced?: (newSettings: AppSettings) => void;
    showQuickConnect?: boolean;
}

export const AutoSyncSummaryCard: React.FC<AutoSyncSummaryCardProps> = ({
    settings,
    onPersistSettings,
    onSettingsSynced,
    showQuickConnect = true,
}) => {
    const { t } = useTranslation();
    const session = useOptionalSession();
    const [discoveredSyncs, setDiscoveredSyncs] = useState<FrontendDiscoveredSync[]>([]);
    const [isConnecting, setIsConnecting] = useState(false);
    const [feedback, setFeedback] = useState<{ type: 'success' | 'error' | 'info'; message: string } | null>(null);

    const loadDiscovered = useCallback(async () => {
        if (typeof marketService?.discoverAvailableSyncs !== 'function') return;
        try {
            const list = await marketService.discoverAvailableSyncs();
            setDiscoveredSyncs(Array.isArray(list) ? list : []);
        } catch {
            // Keep existing discovered sync list
        }
    }, []);

    useEffect(() => {
        loadDiscovered();
        const onSyncUpdate = () => {
            loadDiscovered();
        };
        window.addEventListener(SYNC_STATUS_UPDATED_EVENT, onSyncUpdate);
        return () => {
            window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, onSyncUpdate);
        };
    }, [loadDiscovered]);

    const connectedHosts = useMemo(() => {
        const hosts: string[] = [];
        const seen = new Set<string>();

        const addHost = (url?: string | null) => {
            const host = extractHostname(url);
            if (host && !seen.has(host)) {
                seen.add(host);
                hosts.push(host);
            }
        };

        for (const pub of settings?.publicSyncs || []) {
            if (pub.enabled !== false && pub.status !== 'revoked') {
                addHost(pub.readUrl || pub.writeUrl);
            }
        }

        for (const disc of discoveredSyncs) {
            const st = disc.status || disc.descriptor?.status;
            if (disc.isAuthorized && st !== 'revoked') {
                addHost(disc.descriptor?.readUrl || disc.descriptor?.writeUrl);
            }
        }

        return hosts;
    }, [settings?.publicSyncs, discoveredSyncs]);

    const postEnabled = settings?.postAcceptFromContacts ?? true;
    const marketEnabled = settings?.marketSyncEnabled ?? true;
    const wotEnabled = settings?.wotShareViaSync ?? true;
    const backupEnabled = settings?.syncBackupEnabled ?? true;

    const allChannelsOn = postEnabled && marketEnabled && wotEnabled && backupEnabled;
    const anyChannelOn = postEnabled || marketEnabled || wotEnabled || backupEnabled;
    const isPartiallyActive = anyChannelOn && !allChannelsOn;

    const handleMasterToggle = (enabled: boolean) => {
        onPersistSettings((prev) =>
            prev
                ? {
                      ...prev,
                      postAcceptFromContacts: enabled,
                      marketSyncEnabled: enabled,
                      wotShareViaSync: enabled,
                      syncBackupEnabled: enabled,
                  }
                : prev
        );
        notifySyncStatusUpdated();
    };

    const pendingPublicSyncs = useMemo(() => {
        return (settings?.publicSyncs || []).filter(
            (pub) =>
                pub.status !== 'revoked' &&
                (!pub.enabled || pub.status === 'pending_auth' || (pub.authorizedChannels?.length ?? 0) === 0)
        );
    }, [settings?.publicSyncs]);

    const revokedNotices = useMemo(
        () => collectRevokedNotices(discoveredSyncs, settings?.publicSyncs || []),
        [discoveredSyncs, settings?.publicSyncs]
    );

    const handleAdoptReplacementUrl = async (url: string) => {
        if (typeof marketService?.probePublicSync !== 'function') return;
        setIsConnecting(true);
        setFeedback(null);
        try {
            const runSave = async (pwd?: string | null) => {
                const info = await marketService.probePublicSync(url, pwd || undefined);
                if (!info) return null;
                const hasAuth = (info.authorizedChannels || []).length > 0;
                await marketService.savePublicSync(
                    {
                        writeUrl: info.writeUrl,
                        readUrl: info.readUrl,
                        dropPub: info.dropPub,
                        registerUrl: info.registerUrl,
                        enabled: hasAuth,
                    },
                    pwd || undefined
                );
                return hasAuth;
            };
            const hasAuth = session?.protectAction
                ? await session.protectAction((pwd) => runSave(pwd))
                : await runSave();
            if (hasAuth !== null && hasAuth !== undefined) {
                const fresh = await settingsService.getSettings().catch(() => null);
                if (fresh) {
                    onSettingsSynced?.(fresh);
                }
                await loadDiscovered();
                notifySyncStatusUpdated();
                setFeedback({
                    type: hasAuth ? 'success' : 'info',
                    message: hasAuth ? t('settings.syncProbeSuccess') : t('settings.syncProbeUnauthorized'),
                });
            }
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsConnecting(false);
        }
    };

    const handleOpenPendingRegister = async (drop: StoredPublicSync) => {
        if (!drop.registerUrl || (!drop.registerUrl.startsWith('https://') && !drop.registerUrl.startsWith('http://'))) {
            return;
        }
        const sep = drop.registerUrl.includes('?') ? '&' : '?';
        const fullUrl = `${drop.registerUrl}${sep}key=${encodeURIComponent(drop.accessPubkey)}`;
        try {
            await openUrl(fullUrl);
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleCopyPendingKey = async (drop: StoredPublicSync) => {
        try {
            await copyToClipboard(drop.accessPubkey);
            setFeedback({ type: 'success', message: t('settings.syncPublicKeyCopied') });
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleRecheckPendingDrop = async (drop: StoredPublicSync) => {
        if (typeof marketService?.probePublicSync !== 'function') return;
        setIsConnecting(true);
        setFeedback(null);
        try {
            const info = await marketService.probePublicSync(drop.writeUrl);
            if (info) {
                const hasAuth = (info.authorizedChannels || []).length > 0;
                const saveAction = async (pwd: string | null) => {
                    await marketService.savePublicSync(
                        {
                            writeUrl: info.writeUrl,
                            readUrl: info.readUrl,
                            dropPub: info.dropPub,
                            registerUrl: info.registerUrl,
                            enabled: hasAuth,
                        },
                        pwd || undefined,
                    );
                };
                if (session?.protectAction) {
                    await session.protectAction(saveAction);
                } else {
                    await saveAction(null);
                }
                const fresh = await settingsService.getSettings().catch(() => null);
                if (fresh) {
                    onSettingsSynced?.(fresh);
                }
                await loadDiscovered();
                notifySyncStatusUpdated();
                setFeedback({
                    type: hasAuth ? 'success' : 'info',
                    message: hasAuth ? t('settings.syncProbeSuccess') : t('settings.syncProbeUnauthorized'),
                });
            }
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsConnecting(false);
        }
    };

    const handleQuickConnectCommunity = async () => {
        if (typeof marketService?.probePublicSync !== 'function') return;
        setIsConnecting(true);
        setFeedback(null);
        try {
            const info = await marketService.probePublicSync(COMMUNITY_SYNC_PRESET_URL);
            if (info) {
                const hasAuth = info.authorizedChannels.length > 0;
                const saveAction = async (pwd: string | null) => {
                    await marketService.savePublicSync(
                        {
                            writeUrl: info.writeUrl,
                            readUrl: info.readUrl,
                            dropPub: info.dropPub,
                            registerUrl: info.registerUrl,
                            enabled: hasAuth,
                        },
                        pwd || undefined,
                    );
                };
                if (session?.protectAction) {
                    await session.protectAction(saveAction);
                } else {
                    await saveAction(null);
                }
                const fresh = await settingsService.getSettings().catch(() => null);
                if (fresh) {
                    onSettingsSynced?.(fresh);
                }
                await loadDiscovered();
                notifySyncStatusUpdated();
                setFeedback({
                    type: hasAuth ? 'success' : 'info',
                    message: hasAuth ? t('settings.syncProbeSuccess') : t('settings.syncProbeUnauthorized'),
                });
            }
        } catch (err) {
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsConnecting(false);
        }
    };

    const statusText =
        connectedHosts.length === 1
            ? t('settings.autoSyncConnectedHost', { host: connectedHosts[0] })
            : connectedHosts.length > 1
            ? t('settings.autoSyncConnectedMultiple', {
                  host: connectedHosts[0],
                  more: connectedHosts.length - 1,
              })
            : t('settings.autoSyncDisconnected');

    return (
        <Card
            header={
                <div className="flex items-center justify-between w-full">
                    <div className="flex items-center gap-2">
                        <Cloud size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">
                            {t('settings.autoSyncTitle')}
                        </span>
                    </div>
                    <span className="text-[11px] font-semibold text-theme-secondary flex items-center gap-1.5">
                        <span
                            className={`w-2 h-2 rounded-full ${
                                connectedHosts.length > 0 ? 'bg-emerald-500' : 'bg-slate-400'
                            }`}
                        />
                        <span data-testid="auto-sync-connection-status">{statusText}</span>
                    </span>
                </div>
            }
        >
            <div className="space-y-4">
                {showQuickConnect && connectedHosts.length === 0 && (
                    <div className="flex flex-wrap items-center justify-between gap-3 p-3 rounded-xl bg-amber-50/70 dark:bg-amber-950/20 border border-amber-200/70 dark:border-amber-800/50">
                        <p className="text-xs text-amber-900 dark:text-amber-200 font-medium">
                            {t('settings.autoSyncDisconnected')}
                        </p>
                        <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            disabled={isConnecting}
                            onClick={handleQuickConnectCommunity}
                            data-testid="auto-sync-quick-connect-btn"
                            className="gap-1.5 text-xs shrink-0"
                        >
                            {isConnecting ? (
                                <RefreshCw size={13} className="animate-spin" />
                            ) : (
                                <Cloud size={13} />
                            )}
                            <span>{t('settings.autoSyncConnectCommunityBtn')}</span>
                        </Button>
                    </div>
                )}

                {feedback && (
                    <div
                        className={`p-3 rounded-xl border text-xs flex items-center gap-2 ${
                            feedback.type === 'success'
                                ? 'bg-emerald-50 text-emerald-800 border-emerald-200'
                                : feedback.type === 'info'
                                ? 'bg-amber-50 text-amber-800 border-amber-200'
                                : 'bg-rose-50 text-rose-800 border-rose-200'
                        }`}
                    >
                        {feedback.type === 'success' ? (
                            <CheckCircle2 size={15} className="shrink-0" />
                        ) : (
                            <AlertTriangle size={15} className="shrink-0" />
                        )}
                        <span>{feedback.message}</span>
                    </div>
                )}

                {revokedNotices.length > 0 && (
                    <div
                        data-testid="auto-sync-revocation-alert"
                        className="p-3 rounded-xl border border-amber-300 bg-amber-50/90 dark:bg-amber-950/30 space-y-2 text-xs text-amber-950 dark:text-amber-200"
                    >
                        <div className="font-bold flex items-center gap-1.5">
                            <AlertTriangle size={14} className="text-amber-600 shrink-0" />
                            <span>{t('settings.syncRevocationBannerTitle')}</span>
                        </div>
                        {revokedNotices.map((item, idx) => (
                            <div key={item.url + idx} className="pt-1 border-t border-amber-200/70 space-y-1">
                                <div className="font-mono text-[11px] break-all font-semibold">{item.url}</div>
                                <div>
                                    {item.status === 'revoked'
                                        ? t('settings.syncStatusRevoked')
                                        : item.sunsetAt
                                        ? t('settings.syncStatusDeprecatingDate', {
                                              date: new Date(item.sunsetAt * 1000).toLocaleDateString(),
                                          })
                                        : t('settings.syncStatusDeprecating')}
                                    {item.reason ? ` — ${item.reason}` : ''}
                                </div>
                                {item.replacement?.writeUrl && (
                                    <div className="flex flex-wrap items-center gap-2 pt-1">
                                        <span className="font-mono text-[11px] break-all">
                                            → {item.replacement.writeUrl}
                                        </span>
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="secondary"
                                            disabled={isConnecting}
                                            onClick={() => void handleAdoptReplacementUrl(item.replacement!.writeUrl)}
                                            className="text-xs"
                                        >
                                            {t('settings.syncUseReplacement')}
                                        </Button>
                                    </div>
                                )}
                            </div>
                        ))}
                    </div>
                )}

                {pendingPublicSyncs.length > 0 && (
                    <div
                        data-testid="auto-sync-pending-auth-actions"
                        className="p-3 rounded-xl border border-amber-200/80 bg-amber-50/70 dark:bg-amber-950/20 space-y-2.5"
                    >
                        {pendingPublicSyncs.map((drop) => (
                            <div key={drop.writeUrl} className="space-y-2">
                                <p className="text-xs font-medium text-amber-900 dark:text-amber-200">
                                    {t('settings.syncProbeUnauthorized')}
                                </p>
                                <div className="flex flex-wrap gap-2">
                                    {drop.registerUrl && (
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="primary"
                                            onClick={() => void handleOpenPendingRegister(drop)}
                                            data-testid="auto-sync-open-register-btn"
                                            className="gap-1.5 text-xs"
                                        >
                                            <ExternalLink size={12} />
                                            <span>{t('settings.syncPublicOpenRegister')}</span>
                                        </Button>
                                    )}
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="secondary"
                                        onClick={() => void handleCopyPendingKey(drop)}
                                        data-testid="auto-sync-copy-access-key-btn"
                                        className="gap-1.5 text-xs"
                                    >
                                        <Copy size={12} />
                                        <span>{t('settings.syncPublicCopyKey')}</span>
                                    </Button>
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="outline"
                                        disabled={isConnecting}
                                        onClick={() => void handleRecheckPendingDrop(drop)}
                                        data-testid="auto-sync-recheck-auth-btn"
                                        className="gap-1.5 text-xs"
                                    >
                                        <RefreshCw size={12} className={isConnecting ? 'animate-spin' : ''} />
                                        <span>{t('settings.syncPublicCheckAuth')}</span>
                                    </Button>
                                </div>
                            </div>
                        ))}
                    </div>
                )}

                {/* Master toggle for background sync */}
                <label className="flex items-start gap-3 p-3.5 rounded-xl border border-theme-subtle bg-white dark:bg-slate-900 hover:border-theme-primary/40 transition-colors cursor-pointer">
                    <input
                        type="checkbox"
                        data-testid="auto-sync-master-toggle"
                        checked={allChannelsOn}
                        onChange={(e) => handleMasterToggle(e.target.checked)}
                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary cursor-pointer"
                    />
                    <div className="flex-1 space-y-1">
                        <div className="flex flex-wrap items-center gap-2">
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('settings.autoSyncLabel')}
                            </span>
                            {isPartiallyActive && (
                                <span
                                    data-testid="auto-sync-custom-badge"
                                    className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-amber-100 text-amber-800 border border-amber-200"
                                >
                                    {t('settings.autoSyncCustomBadge')}
                                </span>
                            )}
                        </div>
                        <p className="text-[11px] text-theme-light leading-relaxed">
                            {t('settings.autoSyncDesc')}
                        </p>
                    </div>
                </label>

                {/* WiFi-only toggle */}
                <label className="flex items-start gap-3 p-3 rounded-xl border border-theme-subtle/70 bg-bg-app/50 hover:border-theme-primary/30 transition-colors cursor-pointer">
                    <input
                        type="checkbox"
                        data-testid="settings-wifi-only-toggle"
                        checked={settings?.marketPollWifiOnly ?? false}
                        onChange={(e) =>
                            onPersistSettings((prev) =>
                                prev ? { ...prev, marketPollWifiOnly: e.target.checked } : prev
                            )
                        }
                        className="mt-0.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary cursor-pointer"
                    />
                    <div className="flex-1">
                        <span className="text-xs font-bold text-theme-secondary flex items-center gap-1.5">
                            <Wifi size={13} className="text-theme-primary" />
                            <span>{t('settings.marketPollWifiOnly')}</span>
                        </span>
                        <p className="text-[11px] text-theme-light mt-0.5">
                            {t('settings.marketPollWifiOnlyDesc')}
                        </p>
                    </div>
                </label>

                {/* Reader-side media auto-load setting */}
                <div className="p-3 rounded-xl border border-theme-subtle/70 bg-bg-app/50 flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                    <label
                        htmlFor="settingsMediaAutoLoadSelect"
                        className="text-xs font-bold text-theme-secondary flex items-center gap-1.5 cursor-pointer"
                    >
                        <Image size={13} className="text-theme-primary" />
                        <span>{t('settings.reachCustomMediaLabel')}</span>
                    </label>
                    <select
                        id="settingsMediaAutoLoadSelect"
                        data-testid="settings-media-auto-load-select"
                        value={settings?.mediaAutoLoadMaxHops ?? 2}
                        onChange={(e) => {
                            const maxHops = Number(e.target.value);
                            onPersistSettings((prev) =>
                                prev ? { ...prev, mediaAutoLoadMaxHops: maxHops } : prev
                            );
                        }}
                        className="h-8 px-2.5 bg-white dark:bg-slate-900 border border-theme-subtle rounded-lg text-xs font-semibold text-theme-secondary outline-none focus:ring-2 focus:ring-theme-primary/20 cursor-pointer shrink-0"
                    >
                        <option value={0}>{t('settings.mediaAutoLoad0')}</option>
                        <option value={1}>{t('settings.mediaAutoLoad1')}</option>
                        <option value={2}>{t('settings.mediaAutoLoad2')}</option>
                        <option value={3}>{t('settings.mediaAutoLoad3')}</option>
                    </select>
                </div>
            </div>
        </Card>
    );
};

export default AutoSyncSummaryCard;
