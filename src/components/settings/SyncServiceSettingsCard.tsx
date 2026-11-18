// src/components/settings/SyncServiceSettingsCard.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Cloud,
    Server,
    Users,
    RefreshCw,
    CheckCircle2,
    AlertTriangle,
    HeartHandshake,
} from 'lucide-react';
import { AppSettings } from '../../types';
import { normalizeSyncUrlInput } from '../../utils/humocoUri';
import { Card } from '../ui/Card';
import { SegmentedTabs } from '../ui/SegmentedTabs';
import {
    HOST_SYNC_DRAFT_KEY,
    CONSUMER_SYNC_DRAFT_KEY,
    isSameSyncEndpoint,
} from './sync/syncHelpers';
import { useSyncServiceAdmin } from './sync/useSyncServiceAdmin';
import { ConnectedSyncsCard } from './sync/ConnectedSyncsCard';
import { HostSyncSetupWizard } from './sync/HostSyncSetupWizard';
import { HostSyncAdminPanel } from './sync/HostSyncAdminPanel';

export {
    normalizeSyncUrlInput,
    HOST_SYNC_DRAFT_KEY,
    CONSUMER_SYNC_DRAFT_KEY,
    isSameSyncEndpoint,
};

export interface SyncServiceSettingsCardProps {
    settings: AppSettings | null;
    onSettingsUpdated?: (newSettings: AppSettings) => void;
}

/**
 * Orchestrates connected sync endpoints, sync channel settings, and own-hosted sync administration.
 */
export const SyncServiceSettingsCard: React.FC<SyncServiceSettingsCardProps> = ({
    settings,
    onSettingsUpdated,
}) => {
    const { t } = useTranslation();
    const [activeTab, setActiveTab] = useState<'connected' | 'host'>('connected');
    const admin = useSyncServiceAdmin({ settings, onSettingsUpdated });

    return (
        <Card
            header={
                <div className="flex items-center justify-between gap-3 w-full">
                    <div className="flex items-center gap-2">
                        <Cloud size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">
                            {t('settings.syncServiceCardTitle')}
                        </span>
                    </div>
                    {admin.isLoading && <RefreshCw size={14} className="animate-spin text-theme-light" />}
                </div>
            }
        >
            <div className="space-y-6">
                <p className="text-xs text-theme-light font-medium">
                    {t('settings.syncServiceCardSubtitle')}
                </p>

                {admin.feedback && (
                    <div
                        className={`p-3 rounded-xl border text-xs flex items-center gap-2 ${
                            admin.feedback.type === 'success'
                                ? 'bg-emerald-50 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800'
                                : admin.feedback.type === 'info'
                                ? 'bg-amber-50 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800'
                                : 'bg-rose-50 text-rose-800 border-rose-200 dark:bg-rose-950/40 dark:text-rose-300 dark:border-rose-800'
                        }`}
                    >
                        {admin.feedback.type === 'success' ? (
                            <CheckCircle2 size={16} />
                        ) : (
                            <AlertTriangle size={16} />
                        )}
                        <span>{admin.feedback.message}</span>
                    </div>
                )}

                <SegmentedTabs
                    activeTab={activeTab}
                    onChange={(id) => setActiveTab(id as 'connected' | 'host')}
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

                {activeTab === 'connected' && (
                    <ConnectedSyncsCard
                        discoveredDrops={admin.discoveredDrops}
                        configuredPublicDrops={admin.configuredPublicDrops}
                        inputUrl={admin.inputUrl}
                        onInputUrlChange={admin.setInputUrl}
                        isProbing={admin.isProbing}
                        isProbingContacts={admin.isProbingContacts}
                        probeResult={admin.probeResult}
                        cooldownSeconds={admin.cooldownSeconds}
                        isCoolingDown={admin.isCoolingDown}
                        channelSettings={admin.channelSettings}
                        onChannelSettingChange={admin.updateChannelSetting}
                        onProbeAndAdd={admin.handleProbeAndAdd}
                        onProbeContactSyncs={admin.handleProbeContactSyncs}
                        onToggleDiscoveredDrop={admin.handleToggleDiscoveredDrop}
                        onTogglePublicDrop={admin.handleTogglePublicDrop}
                        onRemovePublicSync={admin.handleRemovePublicSync}
                        onCheckPublicDropAuth={admin.handleCheckPublicDropAuth}
                        onAdoptReplacementUrl={admin.handleAdoptReplacementUrl}
                        onFeedback={admin.setFeedback}
                        checkingPublicUrl={admin.checkingPublicUrl}
                        isDeletingPublicDrop={admin.isDeletingPublicDrop}
                    />
                )}

                {activeTab === 'host' && (
                    <div className="space-y-6 animate-in fade-in duration-200">
                        <div className="p-4 bg-emerald-500/10 dark:bg-emerald-950/20 rounded-2xl border border-emerald-500/20 space-y-1.5 text-xs text-theme-secondary">
                            <h4 className="font-bold text-emerald-800 dark:text-emerald-300 text-sm flex items-center gap-1.5">
                                <HeartHandshake size={16} />
                                <span>{t('settings.syncHostCommunityTitle')}</span>
                            </h4>
                            <p className="leading-relaxed text-emerald-900/90 dark:text-emerald-200/90 font-medium">
                                {t('settings.syncHostCommunityDesc')}
                            </p>
                        </div>

                        {admin.adminStatus == null ? (
                            <HostSyncSetupWizard
                                hostPolicy={admin.hostPolicy}
                                onHostPolicyChange={admin.setHostPolicy}
                                hostQuotas={admin.hostQuotas}
                                onHostQuotasChange={admin.setHostQuotas}
                                hostWebspaceUrl={admin.hostWebspaceUrl}
                                onHostWebspaceUrlChange={admin.setHostWebspaceUrl}
                                isCopyingRustConfig={admin.isCopyingRustConfig}
                                onCopyRustConfig={admin.handleCopyRustConfig}
                                onFeedback={admin.setFeedback}
                                onActivated={admin.setAdminStatus}
                            />
                        ) : (
                            <HostSyncAdminPanel
                                adminStatus={admin.adminStatus}
                                hostPolicy={admin.hostPolicy}
                                onHostPolicyChange={admin.setHostPolicy}
                                hostQuotas={admin.hostQuotas}
                                onHostQuotasChange={admin.setHostQuotas}
                                isUpdatingPolicy={admin.isUpdatingPolicy}
                                isCopyingRustConfig={admin.isCopyingRustConfig}
                                isRevokingServer={admin.isRevokingServer}
                                isDeletingAdmin={admin.isDeletingAdmin}
                                onUpdateHostPolicy={admin.handleUpdateHostPolicy}
                                onCopyRustConfig={admin.handleCopyRustConfig}
                                onRevokeServer={admin.handleRevokeServer}
                                onDeleteHost={admin.handleDeleteHost}
                            />
                        )}
                    </div>
                )}
            </div>
        </Card>
    );
};

export default SyncServiceSettingsCard;
