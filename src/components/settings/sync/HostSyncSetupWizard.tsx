// src/components/settings/sync/HostSyncSetupWizard.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Server, Shield, FileArchive, FileCode, RefreshCw } from 'lucide-react';
import { FrontendMemberPolicyDto, FrontendSyncAdminStatus } from '../../../types';
import { marketService } from '../../../services/marketService';
import { useSession } from '../../../context/SessionContext';
import { normalizeSyncUrlInput } from '../../../utils/humocoUri';
import { pickSaveFilePath } from '../../../utils/fileIo';
import { runSyncBackupPass } from '../../../sync/autoBackup';
import { notifySyncStatusUpdated } from '../../../utils/syncEvents';
import { translateError } from '../../../utils/errorHelper';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import { PolicyEditor, QuotaEditor } from './QuotaPolicyEditors';
import {
    HOST_SYNC_DRAFT_KEY,
    HostQuotasState,
    SyncFeedback,
    normalizeHostQuotas,
    resolveNormalizedSyncUrl,
    setSessionDraft,
} from './syncHelpers';

export interface HostSyncSetupWizardProps {
    hostPolicy: FrontendMemberPolicyDto;
    onHostPolicyChange: (updater: (prev: FrontendMemberPolicyDto) => FrontendMemberPolicyDto) => void;
    hostQuotas: HostQuotasState;
    onHostQuotasChange: (updater: (prev: HostQuotasState) => HostQuotasState) => void;
    hostWebspaceUrl: string;
    onHostWebspaceUrlChange: (val: string) => void;
    isCopyingRustConfig: boolean;
    onCopyRustConfig: (useWizardPolicy: boolean) => void;
    onFeedback: (feedback: SyncFeedback | null) => void;
    onActivated: (admin: FrontendSyncAdminStatus) => void;
}

/**
 * Two-step wizard for generating setup artifacts (PHP ZIP / Rust config.toml) and verifying a new hosted sync endpoint.
 */
export const HostSyncSetupWizard: React.FC<HostSyncSetupWizardProps> = ({
    hostPolicy,
    onHostPolicyChange,
    hostQuotas,
    onHostQuotasChange,
    hostWebspaceUrl,
    onHostWebspaceUrlChange,
    isCopyingRustConfig,
    onCopyRustConfig,
    onFeedback,
    onActivated,
}) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();
    const [isDownloadingZip, setIsDownloadingZip] = useState(false);
    const [isTestingHost, setIsTestingHost] = useState(false);

    const handleDownloadZip = async () => {
        setIsDownloadingZip(true);
        onFeedback(null);
        try {
            const filePath = await pickSaveFilePath({
                defaultFilename: 'humoco-sync-setup.zip',
                filters: [{ name: 'ZIP Archive', extensions: ['zip'] }],
            });
            if (filePath) {
                await protectAction(async (pwd) => {
                    await marketService.prepareSyncSetup(
                        { policy: hostPolicy, ...normalizeHostQuotas(hostQuotas) },
                        pwd || undefined
                    );
                    await marketService.saveSyncSetupZip(filePath, pwd || undefined);
                });
                onFeedback({ type: 'success', message: t('settings.syncZipSaved') });
            }
        } catch (err) {
            onFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsDownloadingZip(false);
        }
    };

    const handleTestAndActivateHost = async () => {
        const targetUrl = resolveNormalizedSyncUrl(hostWebspaceUrl, onHostWebspaceUrlChange);
        if (!targetUrl) return;
        setIsTestingHost(true);
        onFeedback(null);
        try {
            await protectAction(async (pwd) => {
                const testRes = await marketService.testSyncConnection(targetUrl, pwd || undefined);
                if (!testRes.success || !testRes.writeOk || !testRes.readOk) {
                    throw new Error(testRes.message || 'Verbindung zum Webspace konnte nicht verifiziert werden.');
                }
                const admin = await marketService.finalizeSyncSetup(targetUrl, pwd || undefined);
                onActivated(admin);
                setSessionDraft(HOST_SYNC_DRAFT_KEY, '');
                notifySyncStatusUpdated();
                setTimeout(() => {
                    runSyncBackupPass(true).catch(() => {});
                }, 1000);
                onFeedback({ type: 'success', message: t('settings.syncHostActivatedSuccess') });
            });
        } catch (err) {
            onFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsTestingHost(false);
        }
    };

    return (
        <div className="space-y-6">
            <div className="p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/20 space-y-2 text-xs text-theme-secondary">
                <h4 className="font-bold text-theme-primary text-sm flex items-center gap-1.5">
                    <Server size={16} />
                    <span>{t('settings.syncHostIntroTitle')}</span>
                </h4>
                <p className="leading-relaxed">{t('settings.syncHostIntroDesc')}</p>
                <div className="flex items-center gap-1.5 pt-1 text-[11px] font-semibold text-emerald-700 dark:text-emerald-400">
                    <Shield size={13} />
                    <span>{t('settings.syncHostPrivacyNotice')}</span>
                </div>
            </div>

            {/* Step 1: Policies, Quotas & Artifact Download */}
            <div className="p-4 bg-white rounded-2xl border border-theme-subtle space-y-4">
                <div className="flex items-center gap-2">
                    <span className="w-5 h-5 rounded-full bg-theme-primary text-white text-xs font-bold flex items-center justify-center shrink-0">
                        1
                    </span>
                    <h4 className="text-xs font-bold text-theme-primary uppercase tracking-wider">
                        {t('settings.syncHostSetupStep1')}
                    </h4>
                </div>

                <PolicyEditor policy={hostPolicy} onChange={onHostPolicyChange} idPrefix="setup" />

                <QuotaEditor
                    maxTotalStorageMb={hostQuotas.maxTotalStorageMb}
                    maxMembers={hostQuotas.maxMembers}
                    maxUserStorageMb={hostQuotas.maxUserStorageMb}
                    onChangeTotalStorage={(val) => onHostQuotasChange((q) => ({ ...q, maxTotalStorageMb: val }))}
                    onChangeMembers={(val) => onHostQuotasChange((q) => ({ ...q, maxMembers: val }))}
                    onChangeUserStorage={(val) => onHostQuotasChange((q) => ({ ...q, maxUserStorageMb: val }))}
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
                        onClick={() => onCopyRustConfig(true)}
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
                        <li>
                            {t(
                                'settings.syncHostNextStepPhp',
                                'PHP webspace (humoco-sync-setup.zip): Extract the ZIP archive and upload its contents (sync.php & config) to your webspace directory via FTP/SFTP.'
                            )}
                        </li>
                        <li>
                            {t(
                                'settings.syncHostNextStepRust',
                                'Rust server (config.toml): Save the copied configuration as config.toml on your server and start humoco-sync-server.'
                            )}
                        </li>
                        <li>
                            {t(
                                'settings.syncHostNextStepVerify',
                                'Activate: Enter your server\'s public URL in Step 2 below and click "Test & Activate".'
                            )}
                        </li>
                    </ul>
                </div>
            </div>

            {/* Step 2: URL & Live Finalize */}
            <div className="p-4 bg-white rounded-2xl border border-theme-subtle space-y-4">
                <div className="flex items-center gap-2">
                    <span className="w-5 h-5 rounded-full bg-theme-primary text-white text-xs font-bold flex items-center justify-center shrink-0">
                        2
                    </span>
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
                            onChange={(e) => onHostWebspaceUrlChange(e.target.value)}
                            onBlur={() => {
                                if (hostWebspaceUrl.trim()) {
                                    const norm = normalizeSyncUrlInput(hostWebspaceUrl);
                                    if (norm && norm !== hostWebspaceUrl) onHostWebspaceUrlChange(norm);
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
    );
};
