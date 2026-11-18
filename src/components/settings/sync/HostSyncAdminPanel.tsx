// src/components/settings/sync/HostSyncAdminPanel.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
    CheckCircle2,
    Copy,
    Check,
    HardDrive,
    Users,
    Shield,
    RefreshCw,
    FileCode,
    AlertTriangle,
    Trash2,
} from 'lucide-react';
import { FrontendMemberPolicyDto, FrontendSyncAdminStatus } from '../../../types';
import { copyToClipboard } from '../../../utils/clipboard';
import { Button } from '../../ui/Button';
import { ConfirmationModal } from '../../ui/ConfirmationModal';
import { PolicyEditor, QuotaEditor } from './QuotaPolicyEditors';
import { HostQuotasState } from './syncHelpers';

export interface HostSyncAdminPanelProps {
    adminStatus: FrontendSyncAdminStatus;
    hostPolicy: FrontendMemberPolicyDto;
    onHostPolicyChange: (updater: (prev: FrontendMemberPolicyDto) => FrontendMemberPolicyDto) => void;
    hostQuotas: HostQuotasState;
    onHostQuotasChange: (updater: (prev: HostQuotasState) => HostQuotasState) => void;
    isUpdatingPolicy: boolean;
    isCopyingRustConfig: boolean;
    isRevokingServer: boolean;
    isDeletingAdmin: boolean;
    onUpdateHostPolicy: () => void;
    onCopyRustConfig: (useWizardPolicy: boolean) => void;
    onRevokeServer: (opts: {
        clearRevocation: boolean;
        graceDays?: '30' | '14' | '0';
        reason?: string;
        replacementUrl?: string;
    }) => Promise<boolean>;
    onDeleteHost: () => Promise<boolean>;
}

/**
 * Displays active hosted sync status, quota/policy administration, revocation controls, and deletion.
 */
export const HostSyncAdminPanel: React.FC<HostSyncAdminPanelProps> = ({
    adminStatus,
    hostPolicy,
    onHostPolicyChange,
    hostQuotas,
    onHostQuotasChange,
    isUpdatingPolicy,
    isCopyingRustConfig,
    isRevokingServer,
    isDeletingAdmin,
    onUpdateHostPolicy,
    onCopyRustConfig,
    onRevokeServer,
    onDeleteHost,
}) => {
    const { t } = useTranslation();
    const [copiedLink, setCopiedLink] = useState(false);
    const [isDeleteConfirmOpen, setIsDeleteConfirmOpen] = useState(false);
    const [isRevokeModalOpen, setIsRevokeModalOpen] = useState(false);
    const [revokeGraceDays, setRevokeGraceDays] = useState<'30' | '14' | '0'>('30');
    const [revokeReplacementUrl, setRevokeReplacementUrl] = useState('');
    const [revokeReason, setRevokeReason] = useState('');

    const usedMb = Math.round(((adminStatus.usedBytes || 0) / 1024 / 1024) * 10) / 10;
    const storageRatio = (adminStatus.usedBytes || 0) / (adminStatus.maxTotalStorageMb * 1024 * 1024);
    const storagePercent = Math.min(100, Math.round(storageRatio * 100));
    const memberPercent = Math.min(
        100,
        Math.round(((adminStatus.totalMemberCount || 0) / adminStatus.maxMembers) * 100)
    );

    const handleCopyFriendLink = async () => {
        const ok = await copyToClipboard(adminStatus.descriptor.readUrl);
        if (ok) {
            setCopiedLink(true);
            setTimeout(() => setCopiedLink(false), 2000);
        }
    };

    const handleSubmitRevoke = async (clearRevocation: boolean) => {
        const ok = await onRevokeServer({
            clearRevocation,
            graceDays: revokeGraceDays,
            reason: revokeReason,
            replacementUrl: revokeReplacementUrl,
        });
        if (ok) setIsRevokeModalOpen(false);
    };

    const handleConfirmDelete = async () => {
        const ok = await onDeleteHost();
        if (ok) setIsDeleteConfirmOpen(false);
    };

    return (
        <div className="space-y-6">
            {adminStatus.softWarning && (
                <div className="p-3.5 bg-amber-50 border border-amber-300 rounded-2xl text-xs text-amber-900 flex items-start gap-2.5">
                    <AlertTriangle size={16} className="text-amber-600 shrink-0 mt-0.5" />
                    <span>{t('settings.syncHostSoftWarning')}</span>
                </div>
            )}

            <div className="p-4 bg-emerald-500/10 border border-emerald-500/20 rounded-2xl flex items-start justify-between gap-3 text-xs">
                <div className="flex items-start gap-2.5 min-w-0">
                    <CheckCircle2 size={18} className="text-emerald-600 mt-0.5 shrink-0" />
                    <div className="min-w-0 space-y-1">
                        <h4 className="font-bold text-emerald-950 text-sm">
                            {t('settings.syncHostActiveTitle')}
                        </h4>
                        <p className="font-mono text-[11px] text-emerald-800 break-all">
                            {adminStatus.descriptor.readUrl}
                        </p>
                        {adminStatus.descriptor.writeUrl &&
                            adminStatus.descriptor.writeUrl !== adminStatus.descriptor.readUrl && (
                                <p className="font-mono text-[10px] text-emerald-700/90 break-all">
                                    {t('settings.syncHostWriteUrlLabel')}: {adminStatus.descriptor.writeUrl}
                                </p>
                            )}
                    </div>
                </div>

                <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={handleCopyFriendLink}
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
                            {usedMb} / {adminStatus.maxTotalStorageMb} MB
                        </span>
                    </div>
                    <div className="w-full h-2 rounded-full bg-slate-200 overflow-hidden">
                        <div
                            className={`h-full transition-all ${storageRatio > 0.8 ? 'bg-amber-500' : 'bg-emerald-500'}`}
                            style={{ width: `${storagePercent}%` }}
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
                        <div className="h-full bg-theme-primary transition-all" style={{ width: `${memberPercent}%` }} />
                    </div>
                </div>
            </div>

            {/* Access Rules Editor */}
            <div className="p-4 bg-white rounded-2xl border border-theme-subtle space-y-3">
                <h4 className="text-xs font-bold text-theme-secondary uppercase tracking-wider flex items-center gap-1.5">
                    <Shield size={14} className="text-theme-primary" />
                    <span>{t('settings.syncHostAccessRulesTitle')}</span>
                </h4>

                <PolicyEditor policy={hostPolicy} onChange={onHostPolicyChange} idPrefix="admin" />

                <QuotaEditor
                    maxTotalStorageMb={hostQuotas.maxTotalStorageMb}
                    maxMembers={hostQuotas.maxMembers}
                    maxUserStorageMb={hostQuotas.maxUserStorageMb}
                    onChangeTotalStorage={(val) => onHostQuotasChange((q) => ({ ...q, maxTotalStorageMb: val }))}
                    onChangeMembers={(val) => onHostQuotasChange((q) => ({ ...q, maxMembers: val }))}
                    onChangeUserStorage={(val) => onHostQuotasChange((q) => ({ ...q, maxUserStorageMb: val }))}
                    idPrefix="admin"
                />

                <div className="pt-2">
                    <Button
                        type="button"
                        variant="primary"
                        size="sm"
                        onClick={onUpdateHostPolicy}
                        disabled={isUpdatingPolicy}
                        className="gap-1.5 text-xs"
                    >
                        <RefreshCw size={13} className={isUpdatingPolicy ? 'animate-spin' : ''} />
                        <span>{isUpdatingPolicy ? t('common.updating') : t('settings.syncHostUpdatePolicy')}</span>
                    </Button>
                </div>
            </div>

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
                            onClick={() => handleSubmitRevoke(true)}
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

            {/* Management Actions */}
            <div className="flex flex-wrap items-center gap-2 pt-2 border-t border-theme-subtle/50">
                <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => onCopyRustConfig(false)}
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
                            onClick={() => handleSubmitRevoke(false)}
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

            <ConfirmationModal
                isOpen={isDeleteConfirmOpen}
                title={t('settings.syncHostDeleteConfirmTitle')}
                description={t('settings.syncHostDeleteConfirmDesc')}
                confirmVariant="danger"
                confirmText={t('common.delete')}
                isProcessing={isDeletingAdmin}
                onConfirm={handleConfirmDelete}
                onCancel={() => setIsDeleteConfirmOpen(false)}
            />
        </div>
    );
};
