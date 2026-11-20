// src/components/settings/NetworkProtectionCard.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Network,
    RefreshCw,
    ShieldAlert,
    ShieldCheck,
    Wifi,
    WifiOff,
    CheckCircle2,
} from 'lucide-react';
import { AppConfigDto, LockProtectionOverviewDto, LockServerItemDto } from '../../types';
import { settingsService } from '../../services/settingsService';
import { useSession } from '../../context/SessionContext';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { HelpIcon } from '../ui/HelpIcon';
import { stringifyError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';

export interface NetworkProtectionCardProps {
    config: AppConfigDto | null;
    onConfigUpdated?: (newConfig: AppConfigDto) => void;
    isDevUnlocked: boolean;
    initialOpen?: boolean;
}

export const NetworkProtectionCard: React.FC<NetworkProtectionCardProps> = ({
    config,
    onConfigUpdated,
    isDevUnlocked,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [overview, setOverview] = useState<LockProtectionOverviewDto | null>(null);
    const [isRefreshing, setIsRefreshing] = useState(false);
    const [updatingAdminPub, setUpdatingAdminPub] = useState<string | null>(null);
    const [actionError, setActionError] = useState<string | null>(null);
    const [actionSuccess, setActionSuccess] = useState<string | null>(null);

    const loadOverview = useCallback(async () => {
        if (typeof settingsService.getLockProtectionOverview !== 'function') return;
        try {
            const res = await settingsService.getLockProtectionOverview();
            if (res && typeof res.status === 'string') {
                setOverview(res);
            }
        } catch (e) {
            logger.warn(`Failed to load lock protection overview: ${stringifyError(e)}`);
        }
    }, []);

    useEffect(() => {
        void loadOverview();
    }, [loadOverview, config]);

    const handleRefresh = useCallback(async () => {
        if (typeof settingsService.refreshLockServers !== 'function') return;
        setIsRefreshing(true);
        setActionError(null);
        setActionSuccess(null);
        try {
            const next = await settingsService.refreshLockServers();
            setOverview(next);
            const freshCfg = await settingsService.getConfig().catch(() => null);
            if (freshCfg) onConfigUpdated?.(freshCfg);
        } catch (e) {
            setActionError(stringifyError(e));
        } finally {
            setIsRefreshing(false);
        }
    }, [onConfigUpdated]);

    const handleToggleTrust = useCallback(
        async (srv: LockServerItemDto) => {
            if (!srv.adminPub || typeof settingsService.setLockServerTrust !== 'function') return;
            setUpdatingAdminPub(srv.adminPub);
            setActionError(null);
            setActionSuccess(null);
            try {
                const next = await protectAction(
                    async (password) => {
                        return await settingsService.setLockServerTrust(
                            srv.adminPub,
                            !srv.trusted,
                            password ?? null
                        );
                    },
                    { requireFullAccess: true }
                );
                if (next) {
                    setOverview(next);
                    setActionSuccess(t('settings.l2ModeTrustUpdated', 'Trust setting saved.'));
                    const freshCfg = await settingsService.getConfig().catch(() => null);
                    if (freshCfg) onConfigUpdated?.(freshCfg);
                }
            } catch (e) {
                setActionError(stringifyError(e));
            } finally {
                setUpdatingAdminPub(null);
            }
        },
        [onConfigUpdated, protectAction, t]
    );

    const isReady = overview?.status === 'ready' || (overview?.readyTrustedCount ?? 0) > 0;
    const isDegraded = overview?.status === 'degraded';
    const isSyncing = overview?.status === 'syncing';
    const servers = overview?.servers ?? [];

    const bannerStyle = isReady
        ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-950 dark:text-emerald-200'
        : isDegraded || isSyncing
            ? 'bg-amber-500/10 border-amber-500/20 text-amber-950 dark:text-amber-200'
            : 'bg-slate-500/10 border-slate-500/20 text-slate-900 dark:text-slate-200';

    const sourceLabel = (source: string) => {
        switch (source) {
            case 'own':
                return t('settings.l2ModeSourceOwn', 'Own');
            case 'circle':
                return t('settings.l2ModeSourceCircle', 'Circle');
            case 'public':
                return t('settings.l2ModeSourcePublic', 'Public');
            default:
                return t('settings.l2ModeSourceCache', 'Saved');
        }
    };

    const statusBadge = (status: string) => {
        switch (status) {
            case 'ok':
                return (
                    <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800">
                        {t('settings.l2ModeStatusOk', 'Ready')}
                    </span>
                );
            case 'no_lock':
                return (
                    <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-slate-200 text-slate-700">
                        {t('settings.l2ModeStatusNoLock', 'No protection')}
                    </span>
                );
            case 'unverified':
                return (
                    <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-amber-100 text-amber-800">
                        {t('settings.l2ModeStatusUnverified', 'Unverified')}
                    </span>
                );
            default:
                return (
                    <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-rose-100 text-rose-800">
                        {t('settings.l2ModeStatusUnreachable', 'Unreachable')}
                    </span>
                );
        }
    };

    return (
        <div id="settings-network-section">
            <Card
                header={
                    <div className="flex items-center justify-between gap-2 w-full">
                        <div className="flex items-center gap-2">
                            <Network size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest">
                                {t('settings.l2ProtectionTitle')}
                            </span>
                            <HelpIcon topic="l2Protection" size={14} />
                            {isDevUnlocked && (
                                <span className="ml-2 text-[9px] font-bold px-1.5 py-0.5 rounded bg-amber-100 text-amber-800 border border-amber-200">
                                    DEV
                                </span>
                            )}
                        </div>
                        <Button
                            type="button"
                            size="xs"
                            variant="outline"
                            onClick={handleRefresh}
                            disabled={isRefreshing}
                            data-testid="refresh-lock-servers-btn"
                            className="gap-1.5"
                        >
                            <RefreshCw size={12} className={isRefreshing ? 'animate-spin' : ''} />
                            {t('settings.l2ModeRefreshBtn', 'Refresh status')}
                        </Button>
                    </div>
                }
            >
                <div className="space-y-5">
                    <div className={`p-4 rounded-2xl border flex items-start gap-3.5 ${bannerStyle}`}>
                        {isReady ? (
                            <Wifi size={20} className="text-emerald-600 shrink-0 mt-0.5" />
                        ) : (
                            <WifiOff size={20} className="text-slate-600 shrink-0 mt-0.5" />
                        )}
                        <div className="space-y-1">
                            <div className="flex items-center gap-2 flex-wrap">
                                <span className="text-xs font-black uppercase tracking-wider">
                                    {isReady
                                        ? t('settings.l2ModeConnectedTitle')
                                        : isDegraded || isSyncing
                                            ? t('settings.l2ModeDegradedTitle', 'Limited online protection')
                                            : t('settings.l2ModePureOfflineTitle')}
                                </span>
                                <span
                                    className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                                        isReady
                                            ? 'bg-emerald-100 text-emerald-800'
                                            : 'bg-slate-200 text-slate-700'
                                    }`}
                                >
                                    {isReady
                                        ? `${overview?.readyTrustedCount ?? 0} ${t('settings.l2ModeActiveBadge', 'active')}`
                                        : 'N = 0'}
                                </span>
                            </div>
                            <p className="text-xs text-theme-secondary/80 leading-relaxed font-medium">
                                {t(
                                    'settings.l2ModeAutoSummary',
                                    'Configured sync services are used automatically for online double-spend protection. Disable trust on any service below if you do not want to use it for payment locks.'
                                )}
                            </p>
                        </div>
                    </div>

                    {actionError && (
                        <div className="p-3 rounded-xl bg-rose-50 border border-rose-200 text-xs text-rose-800 font-medium flex items-start gap-2">
                            <ShieldAlert size={16} className="shrink-0 mt-0.5 text-rose-600" />
                            <span>{actionError}</span>
                        </div>
                    )}
                    {actionSuccess && (
                        <div className="p-3 rounded-xl bg-emerald-50 border border-emerald-200 text-xs text-emerald-800 font-medium flex items-start gap-2">
                            <CheckCircle2 size={16} className="shrink-0 mt-0.5 text-emerald-600" />
                            <span>{actionSuccess}</span>
                        </div>
                    )}

                    {servers.length === 0 ? (
                        <p className="text-xs text-theme-light italic">
                            {t(
                                'settings.l2ModeNoSyncsHint',
                                'No sync services configured yet. Vouchers work offline with mathematical signature traps; add a sync service above whenever you also want instant online protection.'
                            )}
                        </p>
                    ) : (
                        <div className="space-y-2">
                            {servers.map((srv) => (
                                <div
                                    key={`${srv.source}:${srv.writeUrl}`}
                                    className="p-3.5 rounded-2xl border border-theme-subtle bg-white/70 dark:bg-slate-900/40 flex flex-col sm:flex-row sm:items-center justify-between gap-3"
                                >
                                    <div className="min-w-0 space-y-1">
                                        <div className="flex items-center gap-2 flex-wrap">
                                            <span className="text-xs font-bold text-theme-primary truncate max-w-[320px]">
                                                {srv.writeUrl}
                                            </span>
                                            <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-theme-subtle/20 text-theme-secondary">
                                                {sourceLabel(srv.source)}
                                            </span>
                                            {statusBadge(srv.status)}
                                        </div>
                                        {srv.error && srv.status !== 'ok' && (
                                            <p className="text-[11px] text-theme-light break-all">
                                                {srv.error}
                                            </p>
                                        )}
                                    </div>
                                    {srv.adminPub && (
                                        <button
                                            type="button"
                                            disabled={updatingAdminPub === srv.adminPub}
                                            onClick={() => handleToggleTrust(srv)}
                                            data-testid={`lock-trust-toggle-${srv.adminPub.slice(0, 8)}`}
                                            className={`shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-bold border transition-colors cursor-pointer ${
                                                srv.trusted
                                                    ? 'bg-emerald-50 border-emerald-200 text-emerald-800 hover:bg-emerald-100'
                                                    : 'bg-slate-100 border-slate-200 text-slate-600 hover:bg-slate-200'
                                            }`}
                                        >
                                            <ShieldCheck size={14} />
                                            {srv.trusted
                                                ? t('settings.l2ModeTrustedLabel', 'Trusted')
                                                : t('settings.l2ModeUntrustedLabel', 'Ignored')}
                                        </button>
                                    )}
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </Card>
        </div>
    );
};

export default NetworkProtectionCard;
