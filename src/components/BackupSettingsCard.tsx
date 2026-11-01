// src/components/BackupSettingsCard.tsx
import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { save, open } from '@tauri-apps/plugin-dialog';
import { AppSettings, BackupSummaryDto } from '../types';
import { backupService } from '../services/backupService';
import { settingsService } from '../services/settingsService';
import { useSession } from '../context/SessionContext';
import { Card } from './ui/Card';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { translateError, stringifyError } from '../utils/errorHelper';
import { logger } from '../utils/log';
import { 
    Archive, 
    KeyRound, 
    FolderSync, 
    FileArchive, 
    CheckCircle2, 
    AlertCircle, 
    Clock, 
    ShieldCheck, 
    Folder,
    RefreshCw,
    X
} from 'lucide-react';

interface BackupSettingsCardProps {
    settings: AppSettings | null;
    onSettingsUpdated?: (newSettings: AppSettings) => void;
}

export const BackupSettingsCard: React.FC<BackupSettingsCardProps> = ({ settings, onSettingsUpdated }) => {
    const { t } = useTranslation();
    const { protectAction } = useSession();

    const [protectionMode, setProtectionMode] = useState<'mnemonic_only' | 'custom_password'>(
        settings?.backupProtectionMode === 'custom_password' ? 'custom_password' : 'mnemonic_only'
    );
    const [backupPassword, setBackupPassword] = useState('');
    const [confirmBackupPassword, setConfirmBackupPassword] = useState('');
    const [syncFolder, setSyncFolder] = useState<string>(settings?.backupSyncFolder || '');
    const [autoBackupEnabled, setAutoBackupEnabled] = useState<boolean>(settings?.autoBackupEnabled ?? true);

    const [isExporting, setIsExporting] = useState(false);
    const [isSyncing, setIsSyncing] = useState(false);
    const [feedback, setFeedback] = useState<{ type: 'success' | 'error'; message: string } | null>(null);

    const validateCustomPassword = (): boolean => {
        if (protectionMode !== 'custom_password') return true;
        if (backupPassword.length < 8) {
            setFeedback({ type: 'error', message: t('settings.backupPasswordMinLength') });
            return false;
        }
        if (backupPassword !== confirmBackupPassword) {
            setFeedback({ type: 'error', message: t('settings.backupPasswordMismatch') });
            return false;
        }
        return true;
    };

    const handleToggleAutoBackup = async (enabled: boolean) => {
        setAutoBackupEnabled(enabled);
        if (settings) {
            const updated: AppSettings = {
                ...settings,
                autoBackupEnabled: enabled,
            };
            try {
                await settingsService.saveSettings(updated);
                if (onSettingsUpdated) onSettingsUpdated(updated);
            } catch {
                // best-effort
            }
        }
    };

    const handleClearFolder = async () => {
        setFeedback(null);
        setSyncFolder('');
        if (settings) {
            const updated: AppSettings = {
                ...settings,
                backupSyncFolder: null,
            };
            try {
                await settingsService.saveSettings(updated);
                if (onSettingsUpdated) onSettingsUpdated(updated);
            } catch (err) {
                logger.error(`Clear backup folder failed: ${stringifyError(err)}`);
            }
        }
    };

    const handleExportFile = async () => {
        setFeedback(null);
        if (!validateCustomPassword()) return;

        try {
            const defaultName = `humoco-wallet-backup-${new Date().toISOString().slice(0, 10)}.hmcbackup`;
            const selectedPath = await save({
                defaultPath: defaultName,
                filters: [{ name: 'HuMoCo Backup', extensions: ['hmcbackup'] }],
            });

            if (!selectedPath) return;

            setIsExporting(true);
            const summary = await protectAction(async (walletPassword) => {
                return await backupService.exportBackupFile({
                    destinationPath: selectedPath,
                    protectionMode,
                    backupPassword: protectionMode === 'custom_password' ? backupPassword : null,
                    password: walletPassword,
                });
            }) as BackupSummaryDto | undefined;

            if (summary) {
                const updatedSettings: AppSettings = {
                    ...(settings || { sessionTimeoutSeconds: 300, privacyDefault: 'public' }),
                    lastBackupAt: summary.timestamp,
                    backupProtectionMode: protectionMode,
                    backupDirty: false,
                };
                if (onSettingsUpdated) onSettingsUpdated(updatedSettings);

                setFeedback({
                    type: 'success',
                    message: t('settings.exportSuccess', { path: summary.destinationPath, count: summary.totalBlobs }),
                });
            }
        } catch (err) {
            logger.error(`Backup export failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsExporting(false);
        }
    };

    const handleChooseFolder = async () => {
        setFeedback(null);
        try {
            const selected = await open({
                directory: true,
                multiple: false,
                defaultPath: syncFolder || undefined,
            });

            if (typeof selected === 'string') {
                setSyncFolder(selected);
                if (settings) {
                    const updated: AppSettings = {
                        ...settings,
                        backupSyncFolder: selected,
                        autoBackupEnabled,
                        backupProtectionMode: protectionMode,
                    };
                    try {
                        await settingsService.saveSettings(updated);
                        if (onSettingsUpdated) onSettingsUpdated(updated);
                    } catch {
                        // ignore
                    }
                }
            }
        } catch (err) {
            logger.error(`Choose backup folder failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        }
    };

    const handleSyncFolder = async () => {
        setFeedback(null);
        if (!syncFolder) {
            setFeedback({ type: 'error', message: t('settings.noFolderSelected') });
            return;
        }
        if (!validateCustomPassword()) return;

        setIsSyncing(true);
        try {
            const summary = await protectAction(async (walletPassword) => {
                return await backupService.syncBackupFolder({
                    folderPath: syncFolder,
                    protectionMode,
                    backupPassword: protectionMode === 'custom_password' ? backupPassword : null,
                    password: walletPassword,
                    forceFull: false,
                });
            }) as BackupSummaryDto | undefined;

            if (summary) {
                const updatedSettings: AppSettings = {
                    ...(settings || { sessionTimeoutSeconds: 300, privacyDefault: 'public' }),
                    backupSyncFolder: syncFolder,
                    backupProtectionMode: protectionMode,
                    autoBackupEnabled,
                    lastBackupAt: summary.timestamp,
                    backupDirty: false,
                };
                if (onSettingsUpdated) onSettingsUpdated(updatedSettings);

                const reused = Math.max(0, summary.totalBlobs - summary.changedBlobs);
                setFeedback({
                    type: 'success',
                    message: t('settings.syncSuccess', {
                        changed: summary.changedBlobs,
                        total: summary.totalBlobs,
                        reused,
                    }),
                });
            }
        } catch (err) {
            logger.error(`Folder sync failed: ${stringifyError(err)}`);
            setFeedback({ type: 'error', message: translateError(err, t) });
        } finally {
            setIsSyncing(false);
        }
    };

    const lastBackupDisplay = settings?.lastBackupAt
        ? new Date(settings.lastBackupAt).toLocaleString()
        : t('settings.backupNever');

    return (
        <Card
            header={
                <div className="flex items-center justify-between w-full">
                    <div className="flex items-center gap-2.5">
                        <Archive size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">
                            {t('settings.backupCardTitle')}
                        </span>
                    </div>
                    <div className="flex items-center gap-1.5 text-[10px] font-bold text-theme-light bg-slate-50 dark:bg-slate-800/50 px-3 py-1 rounded-full border border-theme-subtle/50">
                        <Clock size={12} className="text-theme-light" />
                        <span>{t('settings.lastBackup')}: {lastBackupDisplay}</span>
                    </div>
                </div>
            }
        >
            <div className="space-y-6">
                <p className="text-xs text-theme-light font-medium">
                    {t('settings.backupCardSubtitle')}
                </p>

                {/* Protection Mode Section */}
                <div className="space-y-3 p-4 bg-slate-50/70 dark:bg-slate-800/40 rounded-2xl border border-theme-subtle/60">
                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5">
                        <ShieldCheck size={13} className="text-theme-primary" />
                        {t('settings.protectionModeTitle')}
                    </label>

                    <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                        <label
                            className={`p-3.5 rounded-xl border-2 cursor-pointer transition-all flex flex-col gap-1.5 ${
                                protectionMode === 'mnemonic_only'
                                    ? 'bg-white dark:bg-slate-900 border-theme-primary shadow-sm'
                                    : 'bg-white/50 dark:bg-slate-900/50 border-theme-subtle/60 hover:border-theme-primary/30'
                            }`}
                        >
                            <div className="flex items-center gap-2">
                                <input
                                    type="radio"
                                    name="protectionMode"
                                    value="mnemonic_only"
                                    checked={protectionMode === 'mnemonic_only'}
                                    onChange={() => setProtectionMode('mnemonic_only')}
                                    className="sr-only"
                                />
                                <span className="font-bold text-xs text-theme-secondary flex items-center gap-1.5">
                                    <KeyRound size={14} className={protectionMode === 'mnemonic_only' ? 'text-theme-primary' : 'text-theme-light'} />
                                    {t('settings.protectionModeMnemonic')}
                                </span>
                            </div>
                            <p className="text-[10px] text-theme-light leading-relaxed">
                                {t('settings.protectionModeMnemonicDesc')}
                            </p>
                        </label>

                        <label
                            className={`p-3.5 rounded-xl border-2 cursor-pointer transition-all flex flex-col gap-1.5 ${
                                protectionMode === 'custom_password'
                                    ? 'bg-white dark:bg-slate-900 border-theme-primary shadow-sm'
                                    : 'bg-white/50 dark:bg-slate-900/50 border-theme-subtle/60 hover:border-theme-primary/30'
                            }`}
                        >
                            <div className="flex items-center gap-2">
                                <input
                                    type="radio"
                                    name="protectionMode"
                                    value="custom_password"
                                    checked={protectionMode === 'custom_password'}
                                    onChange={() => setProtectionMode('custom_password')}
                                    className="sr-only"
                                />
                                <span className="font-bold text-xs text-theme-secondary flex items-center gap-1.5">
                                    <ShieldCheck size={14} className={protectionMode === 'custom_password' ? 'text-theme-primary' : 'text-theme-light'} />
                                    {t('settings.protectionModeCustomPassword')}
                                </span>
                            </div>
                            <p className="text-[10px] text-theme-light leading-relaxed">
                                {t('settings.protectionModeCustomPasswordDesc')}
                            </p>
                        </label>
                    </div>

                    {/* Custom Password Fields */}
                    {protectionMode === 'custom_password' && (
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-3 pt-2 animate-in fade-in duration-300">
                            <div className="space-y-1">
                                <label className="text-[10px] font-bold text-theme-light uppercase tracking-wider">
                                    {t('settings.backupPasswordLabel')}
                                </label>
                                <Input
                                    type="password"
                                    value={backupPassword}
                                    onChange={(e) => setBackupPassword(e.target.value)}
                                    placeholder={t('settings.backupPasswordPlaceholder')}
                                    className="text-xs"
                                />
                            </div>
                            <div className="space-y-1">
                                <label className="text-[10px] font-bold text-theme-light uppercase tracking-wider">
                                    {t('settings.backupPasswordConfirmLabel')}
                                </label>
                                <Input
                                    type="password"
                                    value={confirmBackupPassword}
                                    onChange={(e) => setConfirmBackupPassword(e.target.value)}
                                    placeholder={t('settings.backupPasswordPlaceholder')}
                                    className="text-xs"
                                />
                            </div>
                        </div>
                    )}
                </div>

                {/* 3. Action Panels (Sync Folder & Single File) */}
                <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
                    {/* Action Panel 1: Local Backup Folder */}
                    <div className="p-4 rounded-2xl bg-white dark:bg-slate-900 border border-theme-subtle flex flex-col justify-between space-y-4 shadow-sm">
                        <div className="space-y-3">
                            <div className="flex items-center gap-2">
                                <FolderSync size={16} className="text-theme-primary" />
                                <h4 className="font-bold text-xs text-theme-secondary uppercase tracking-wider">
                                    {t('settings.syncFolderTitle')}
                                </h4>
                            </div>
                            <p className="text-[11px] text-theme-light leading-relaxed">
                                {t('settings.syncFolderDesc')}
                            </p>

                            <div className="p-2.5 rounded-xl bg-blue-50/60 border border-blue-200/50 text-[11px] text-blue-900 leading-relaxed">
                                {t('settings.syncFolderCloudTip')}
                            </div>

                            <div className="flex items-center gap-2 pt-1">
                                <div className="flex-1 min-w-0 px-3 py-2 bg-slate-50 dark:bg-slate-800 rounded-xl border border-theme-subtle/50 text-[10px] font-mono text-theme-secondary truncate">
                                    {syncFolder || t('settings.noFolderSelected')}
                                </div>
                                <Button
                                    type="button"
                                    variant="outline"
                                    onClick={handleChooseFolder}
                                    className="px-3 py-2 text-xs shrink-0"
                                >
                                    <Folder size={14} className="mr-1 inline" />
                                    {syncFolder ? t('settings.changeFolder') : t('settings.chooseFolderButton')}
                                </Button>
                                {syncFolder && (
                                    <Button
                                        type="button"
                                        variant="outline"
                                        onClick={handleClearFolder}
                                        className="p-2 text-xs shrink-0 text-slate-500 hover:text-rose-600"
                                        title={t('settings.clearFolderButton')}
                                        aria-label={t('settings.clearFolderButton')}
                                    >
                                        <X size={14} />
                                    </Button>
                                )}
                            </div>

                            {/* Schalter: Automatisch im Hintergrund sichern */}
                            <div className="pt-2 border-t border-theme-subtle/60 space-y-1.5">
                                <label className="flex items-start gap-2.5 cursor-pointer select-none">
                                    <input
                                        type="checkbox"
                                        checked={autoBackupEnabled}
                                        onChange={(e) => handleToggleAutoBackup(e.target.checked)}
                                        className="mt-0.5 rounded text-theme-primary focus:ring-theme-accent"
                                    />
                                    <div className="text-xs">
                                        <span className="font-bold text-theme-secondary block">
                                            {t('settings.autoBackupToggleLabel', 'Automatisch im Hintergrund sichern')}
                                        </span>
                                        <span className="text-[11px] text-theme-light block leading-relaxed">
                                            {protectionMode === 'custom_password'
                                                ? t('settings.autoBackupPasswordNotice', 'Hinweis: Bei eigenem Backup-Passwort ist Hintergrund-Sync pausiert (nur manuelle Sicherung).')
                                                : t('settings.autoBackupToggleDesc', 'Sichert deine Gutscheine und Kontakte nach jeder Änderung lautlos im Hintergrund.')}
                                        </span>
                                    </div>
                                </label>
                            </div>
                        </div>

                        <Button
                            type="button"
                            onClick={handleSyncFolder}
                            disabled={isSyncing || !syncFolder}
                            variant="primary"
                            className="w-full flex items-center justify-center gap-2 text-xs py-2.5"
                        >
                            {isSyncing ? (
                                <>
                                    <RefreshCw size={14} className="animate-spin" />
                                    {t('settings.syncing')}
                                </>
                            ) : (
                                <>
                                    <FolderSync size={14} />
                                    {t('settings.syncFolderNowButton')}
                                </>
                            )}
                        </Button>
                    </div>

                    {/* Action Panel 2: Export Single File */}
                    <div className="p-4 rounded-2xl bg-white dark:bg-slate-900 border border-theme-subtle flex flex-col justify-between space-y-4 shadow-sm">
                        <div className="space-y-1.5">
                            <div className="flex items-center gap-2">
                                <FileArchive size={16} className="text-theme-primary" />
                                <h4 className="font-bold text-xs text-theme-secondary uppercase tracking-wider">
                                    {t('settings.exportFileTitle')}
                                </h4>
                            </div>
                            <p className="text-[11px] text-theme-light leading-relaxed">
                                {t('settings.exportFileDesc')}
                            </p>
                        </div>

                        <Button
                            type="button"
                            onClick={handleExportFile}
                            disabled={isExporting}
                            className="w-full flex items-center justify-center gap-2 text-xs py-2.5"
                        >
                            {isExporting ? (
                                <>
                                    <RefreshCw size={14} className="animate-spin" />
                                    {t('settings.exporting')}
                                </>
                            ) : (
                                <>
                                    <FileArchive size={14} />
                                    {t('settings.exportFileButton')}
                                </>
                            )}
                        </Button>
                    </div>
                </div>

                {/* Feedback Notification */}
                {feedback && (
                    <div
                        className={`p-4 rounded-2xl border flex items-center gap-3 animate-in fade-in duration-300 ${
                            feedback.type === 'success'
                                ? 'bg-emerald-50 dark:bg-emerald-950/30 border-emerald-200 dark:border-emerald-800 text-emerald-800 dark:text-emerald-200'
                                : 'bg-rose-50 dark:bg-rose-950/30 border-rose-200 dark:border-rose-800 text-rose-800 dark:text-rose-200'
                        }`}
                    >
                        {feedback.type === 'success' ? (
                            <CheckCircle2 size={18} className="text-emerald-600 dark:text-emerald-400 shrink-0" />
                        ) : (
                            <AlertCircle size={18} className="text-rose-600 dark:text-rose-400 shrink-0" />
                        )}
                        <span className="text-xs font-semibold leading-relaxed">{feedback.message}</span>
                    </div>
                )}
            </div>
        </Card>
    );
};

export default BackupSettingsCard;
