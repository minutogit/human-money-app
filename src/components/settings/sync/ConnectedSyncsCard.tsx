// src/components/settings/sync/ConnectedSyncsCard.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
    Users,
    Globe,
    RefreshCw,
    CheckCircle2,
    AlertTriangle,
    Trash2,
    Copy,
    Check,
    ExternalLink,
} from 'lucide-react';
import { FrontendDiscoveredSync, StoredPublicSync } from '../../../types';
import { normalizeSyncUrlInput } from '../../../utils/humocoUri';
import { copyToClipboard } from '../../../utils/clipboard';
import { translateError } from '../../../utils/errorHelper';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { InfoHint } from '../../ui/InfoHint';
import { ConfirmationModal } from '../../ui/ConfirmationModal';
import { SyncChannelPanel } from './SyncChannelPanel';
import {
    COMMUNITY_SYNC_PRESET_URL,
    ChannelSettingsState,
    SyncFeedback,
    SyncProbeResult,
    collectRevokedNotices,
    getDiscoveredDropBadge,
    isSafeExternalSyncUrl,
} from './syncHelpers';

export interface ConnectedSyncsCardProps {
    discoveredDrops: FrontendDiscoveredSync[];
    configuredPublicDrops: StoredPublicSync[];
    inputUrl: string;
    onInputUrlChange: (val: string) => void;
    isProbing: boolean;
    isProbingContacts: boolean;
    probeResult: SyncProbeResult | null;
    cooldownSeconds: number;
    isCoolingDown: boolean;
    channelSettings: ChannelSettingsState;
    onChannelSettingChange: <K extends keyof ChannelSettingsState>(
        key: K,
        value: ChannelSettingsState[K]
    ) => void;
    onProbeAndAdd: () => void;
    onProbeContactSyncs: () => void;
    onToggleDiscoveredDrop: (drop: FrontendDiscoveredSync) => void;
    onTogglePublicDrop: (drop: StoredPublicSync) => void;
    onRemovePublicSync: (writeUrl: string, deleteRemote: boolean) => Promise<boolean>;
    onCheckPublicDropAuth: (drop: StoredPublicSync) => void;
    onAdoptReplacementUrl?: (url: string) => void;
    onFeedback: (feedback: SyncFeedback | null) => void;
    checkingPublicUrl: string | null;
    isDeletingPublicDrop: boolean;
}

/**
 * Renders channel settings, manual sync URL probing, contact-discovered drops, and public sync targets.
 */
export const ConnectedSyncsCard: React.FC<ConnectedSyncsCardProps> = ({
    discoveredDrops,
    configuredPublicDrops,
    inputUrl,
    onInputUrlChange,
    isProbing,
    isProbingContacts,
    probeResult,
    cooldownSeconds,
    isCoolingDown,
    channelSettings,
    onChannelSettingChange,
    onProbeAndAdd,
    onProbeContactSyncs,
    onToggleDiscoveredDrop,
    onTogglePublicDrop,
    onRemovePublicSync,
    onCheckPublicDropAuth,
    onAdoptReplacementUrl,
    onFeedback,
    checkingPublicUrl,
    isDeletingPublicDrop,
}) => {
    const { t } = useTranslation();
    const [pendingPublicDelete, setPendingPublicDelete] = useState<StoredPublicSync | null>(null);
    const [deletePublicRemoteData, setDeletePublicRemoteData] = useState(true);
    const [copiedPublicKeyUrl, setCopiedPublicKeyUrl] = useState<string | null>(null);

    const revokedList = collectRevokedNotices(discoveredDrops, configuredPublicDrops);

    const handleOpenRegister = async (drop: StoredPublicSync) => {
        if (!isSafeExternalSyncUrl(drop.registerUrl)) return;
        const sep = drop.registerUrl.includes('?') ? '&' : '?';
        const fullUrl = `${drop.registerUrl}${sep}key=${encodeURIComponent(drop.accessPubkey)}`;
        try {
            await openUrl(fullUrl);
        } catch (err) {
            onFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleCopyKey = async (drop: StoredPublicSync) => {
        try {
            await copyToClipboard(drop.accessPubkey);
            setCopiedPublicKeyUrl(drop.writeUrl);
            onFeedback({ type: 'success', message: t('settings.syncPublicKeyCopied') });
            setTimeout(() => {
                setCopiedPublicKeyUrl((prev) => (prev === drop.writeUrl ? null : prev));
            }, 2000);
        } catch (err) {
            onFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleConfirmDelete = async () => {
        if (!pendingPublicDelete) return;
        const ok = await onRemovePublicSync(pendingPublicDelete.writeUrl, deletePublicRemoteData);
        if (ok) setPendingPublicDelete(null);
    };

    return (
        <div className="space-y-6 animate-in fade-in duration-200">
            <InfoHint topic="connection" />

            <SyncChannelPanel
                revokedList={revokedList}
                channelSettings={channelSettings}
                onChannelSettingChange={onChannelSettingChange}
                onUseReplacementUrl={onAdoptReplacementUrl || onInputUrlChange}
            />

            {/* Manual Sync URL Input */}
            <div className="p-4 bg-white rounded-2xl border border-theme-subtle shadow-xs space-y-3">
                <div>
                    <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                        {t('settings.syncAddTitle')}
                    </h4>
                    <p className="text-[11px] text-theme-light mt-0.5">{t('settings.syncAddDesc')}</p>
                </div>

                <div className="flex flex-col gap-2">
                    <div className="flex flex-col sm:flex-row gap-2">
                        <Input
                            value={inputUrl}
                            onChange={(e) => onInputUrlChange(e.target.value)}
                            onBlur={() => {
                                if (inputUrl.trim()) {
                                    const norm = normalizeSyncUrlInput(inputUrl);
                                    if (norm && norm !== inputUrl) onInputUrlChange(norm);
                                }
                            }}
                            placeholder={t('settings.syncAddPlaceholder')}
                            className="text-xs flex-1 font-mono"
                            onKeyDown={(e) => {
                                if (e.key === 'Enter') {
                                    e.preventDefault();
                                    onProbeAndAdd();
                                }
                            }}
                        />
                        <Button
                            type="button"
                            variant="primary"
                            size="sm"
                            onClick={onProbeAndAdd}
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
                            onClick={() => onInputUrlChange(COMMUNITY_SYNC_PRESET_URL)}
                            className="text-[11px] text-theme-primary hover:underline flex items-center gap-1 font-medium"
                        >
                            <Globe size={12} />
                            <span>{t('settings.syncCommunityQuickSelect')}</span>
                        </button>
                    </div>
                </div>

                {probeResult && (
                    <div
                        className={`p-2.5 rounded-xl border text-xs flex items-start gap-2 ${
                            probeResult.status === 'success'
                                ? 'bg-emerald-50 text-emerald-800 border-emerald-200'
                                : probeResult.status === 'unauthorized'
                                ? 'bg-amber-50 text-amber-800 border-amber-200'
                                : 'bg-rose-50 text-rose-800 border-rose-200'
                        }`}
                    >
                        {probeResult.status === 'success' ? (
                            <CheckCircle2 size={16} className="shrink-0 mt-0.5 text-emerald-600" />
                        ) : (
                            <AlertTriangle size={16} className="shrink-0 mt-0.5 text-amber-600" />
                        )}
                        <span className="leading-relaxed">{probeResult.message}</span>
                    </div>
                )}
            </div>

            {/* Discovered Friend Drops */}
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
                        onClick={onProbeContactSyncs}
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
                            const { status, badgeColor, statusText, statusTooltip } = getDiscoveredDropBadge(drop, t);
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
                                            onChange={() => onToggleDiscoveredDrop(drop)}
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
                                                    {t('settings.syncServiceOf', { name: drop.sourceLabel || displayUrl })}
                                                </span>
                                                {drop.isSelected && status === 'active' && (
                                                    <span className="px-1.5 py-0.2 rounded text-[9px] font-semibold bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300 shrink-0">
                                                        {t('settings.syncStatusConnected', 'Verbunden')}
                                                    </span>
                                                )}
                                            </div>
                                            <span className="font-mono text-[10px] text-theme-light truncate block">{displayUrl}</span>
                                        </div>
                                    </div>
                                    <span className={`px-2 py-0.5 rounded-full text-[10px] font-bold shrink-0 ${badgeColor}`} title={statusTooltip}>
                                        {statusText}
                                    </span>
                                </div>
                            );
                        })}
                    </div>
                )}
            </div>

            {/* Configured Public / Manual Drops */}
            <div className="space-y-3 pt-2 border-t border-theme-subtle/50">
                <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                    <Globe size={14} className="text-theme-primary" />
                    <span>{t('settings.syncPublicTitle')}</span>
                </h4>

                {(configuredPublicDrops || []).length === 0 ? (
                    <p className="text-xs text-theme-light">{t('settings.syncPublicEmpty')}</p>
                ) : (
                    <div className="space-y-2">
                        {(configuredPublicDrops || []).map((drop, idx) => {
                            const isChecking = checkingPublicUrl === drop.writeUrl;
                            const isCopied = copiedPublicKeyUrl === drop.writeUrl;
                            const hasRegisterUrl = isSafeExternalSyncUrl(drop.registerUrl);

                            return (
                                <div key={drop.writeUrl || idx} className="p-3 bg-white rounded-xl border border-theme-subtle space-y-2 text-xs">
                                    <div className="flex items-center justify-between gap-3">
                                        <div className="min-w-0">
                                            <span className="font-mono text-xs text-theme-primary truncate block">{drop.writeUrl}</span>
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
                                                onClick={() => onTogglePublicDrop(drop)}
                                                className="text-[11px] h-7 px-2"
                                            >
                                                {drop.enabled ? t('settings.syncPublicDisable') : t('settings.syncPublicEnable')}
                                            </Button>
                                            <button
                                                type="button"
                                                onClick={() => {
                                                    setDeletePublicRemoteData(true);
                                                    setPendingPublicDelete(drop);
                                                }}
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
                                                onClick={() => handleOpenRegister(drop)}
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
                                            onClick={() => handleCopyKey(drop)}
                                            className="gap-1.5 text-[11px] h-7 px-2.5"
                                        >
                                            {isCopied ? <Check size={12} className="text-emerald-600" /> : <Copy size={12} />}
                                            <span>{t('settings.syncPublicCopyKey')}</span>
                                        </Button>
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() => onCheckPublicDropAuth(drop)}
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
                onConfirm={handleConfirmDelete}
                onCancel={() => {
                    if (!isDeletingPublicDrop) setPendingPublicDelete(null);
                }}
            />
        </div>
    );
};
