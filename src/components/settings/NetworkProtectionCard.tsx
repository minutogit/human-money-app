// src/components/settings/NetworkProtectionCard.tsx
import React, { useState, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Network,
    Server,
    ShieldAlert,
    Wifi,
    WifiOff,
    CheckCircle2,
} from 'lucide-react';
import { AppConfigDto } from '../../types';
import { settingsService } from '../../services/settingsService';
import { Card } from '../ui/Card';
import { HelpIcon } from '../ui/HelpIcon';
import { AdvancedDisclosure } from '../ui/AdvancedDisclosure';
import { stringifyError } from '../../utils/errorHelper';
import { logger } from '../../utils/log';
import { LockServerProbeSection } from './network/LockServerProbeSection';
import { AnchorTemplateBuilder } from './network/AnchorTemplateBuilder';
import { L2LifecycleSection } from './network/L2LifecycleSection';

export interface NetworkProtectionCardProps {
    config: AppConfigDto | null;
    onConfigUpdated?: (newConfig: AppConfigDto) => void;
    isDevUnlocked: boolean;
    initialOpen?: boolean;
}

/**
 * Orchestrates L2 lock protection overview, server probing, anchor templates, and lifecycle status.
 */
export const NetworkProtectionCard: React.FC<NetworkProtectionCardProps> = ({
    config,
    onConfigUpdated,
    isDevUnlocked,
    initialOpen = false,
}) => {
    const { t } = useTranslation();

    const [actionError, setActionError] = useState<string | null>(null);
    const [actionSuccess, setActionSuccess] = useState<string | null>(null);

    const clearFeedback = useCallback(() => {
        setActionError(null);
        setActionSuccess(null);
    }, []);

    const handleError = useCallback((message: string) => {
        setActionSuccess(null);
        setActionError(message);
    }, []);

    const handleSuccess = useCallback((message: string) => {
        setActionError(null);
        setActionSuccess(message);
    }, []);

    const refreshConfig = useCallback(async () => {
        try {
            const fresh = await settingsService.getConfig();
            if (fresh) {
                onConfigUpdated?.(fresh);
            }
        } catch (e) {
            logger.warn(`Failed to reload config after lock update: ${stringifyError(e)}`);
        }
    }, [onConfigUpdated]);

    const probeList = Object.values(config?.lifecycle.l2ProbeCache || {}).sort(
        (a, b) => (b.probedAtSecs || 0) - (a.probedAtSecs || 0)
    );
    const passingProbes = probeList.filter((p) => p.passed);
    const templates = config?.lifecycle.l2AnchorTemplatesMeta || [];
    const ownNominatedLockServer = config?.securityPolicy.ownNominatedLockServer ?? null;
    const hasActiveLockConfig = Boolean(templates.length > 0 || ownNominatedLockServer);

    return (
        <div id="settings-network-section">
            <Card
                header={
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
                }
            >
                <div className="space-y-6">
                    {/* Overview Banner */}
                    <div
                        className={`p-4 rounded-2xl border flex items-start gap-3.5 ${
                            hasActiveLockConfig
                                ? 'bg-emerald-500/10 border-emerald-500/20 text-emerald-950 dark:text-emerald-200'
                                : 'bg-slate-500/10 border-slate-500/20 text-slate-900 dark:text-slate-200'
                        }`}
                    >
                        {hasActiveLockConfig ? (
                            <Wifi size={20} className="text-emerald-600 shrink-0 mt-0.5" />
                        ) : (
                            <WifiOff size={20} className="text-slate-600 shrink-0 mt-0.5" />
                        )}
                        <div className="space-y-1">
                            <div className="flex items-center gap-2">
                                <span className="text-xs font-black uppercase tracking-wider">
                                    {hasActiveLockConfig
                                        ? t('settings.l2ModeConnectedTitle')
                                        : t('settings.l2ModePureOfflineTitle')}
                                </span>
                                <span
                                    className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${
                                        hasActiveLockConfig
                                            ? 'bg-emerald-100 text-emerald-800'
                                            : 'bg-slate-200 text-slate-700'
                                    }`}
                                >
                                    {hasActiveLockConfig ? 'Sync-Locks' : 'N = 0'}
                                </span>
                            </div>
                            <p className="text-xs text-theme-secondary/80 leading-relaxed font-medium">
                                {hasActiveLockConfig
                                    ? t('settings.l2ModeConnectedDesc')
                                    : t('settings.l2ModePureOfflineDesc')}
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

                    <AdvancedDisclosure
                        icon={<Server size={14} className="text-theme-primary" />}
                        title={
                            <span className="text-xs font-bold text-theme-secondary">
                                {t('settings.advancedNetworkToggle')}
                            </span>
                        }
                        defaultOpen={initialOpen || hasActiveLockConfig || probeList.length > 0}
                        isCustom={hasActiveLockConfig}
                        className="border border-theme-subtle rounded-2xl overflow-hidden"
                        buttonClassName="w-full flex items-center justify-between gap-2 p-4 bg-slate-50/70 dark:bg-slate-800/40 hover:bg-slate-100/70 transition-colors text-left cursor-pointer"
                        contentClassName="p-4 space-y-6 border-t border-theme-subtle"
                    >
                        <LockServerProbeSection
                            ownNominatedLockServer={ownNominatedLockServer}
                            probeList={probeList}
                            passingProbes={passingProbes}
                            onSuccess={handleSuccess}
                            onError={handleError}
                            onClearFeedback={clearFeedback}
                            onRefreshConfig={refreshConfig}
                        />

                        <AnchorTemplateBuilder
                            passingProbes={passingProbes}
                            templates={templates}
                            ownNominatedLockServer={ownNominatedLockServer}
                            onSuccess={handleSuccess}
                            onError={handleError}
                            onClearFeedback={clearFeedback}
                            onRefreshSettings={refreshConfig}
                        />

                        <L2LifecycleSection
                            onSuccess={handleSuccess}
                            onError={handleError}
                            onClearFeedback={clearFeedback}
                        />
                    </AdvancedDisclosure>
                </div>
            </Card>
        </div>
    );
};

export default NetworkProtectionCard;
