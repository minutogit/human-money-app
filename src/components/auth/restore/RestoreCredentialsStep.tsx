import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    CheckCircle2,
    FileText,
    Folder,
    Hash,
    History,
    KeyRound,
    Layers,
    Lock,
    RefreshCw,
    Server,
    ShieldCheck,
    Sparkles,
} from 'lucide-react';
import { BackupInspectionDto, BackupProbeDto, LocalProfileCollisionDto } from '../../../types';
import { AlertBanner } from '../../ui/AlertBanner';
import { Button } from '../../ui/Button';
import { Card } from '../../ui/Card';
import { Input } from '../../ui/Input';
import { PasswordPairFields } from '../../ui/PasswordPairFields';
import { SeedPhraseInput, SeedPhraseValue } from '../SeedPhraseInput';

export interface FileUnlockCredentialsCardProps {
    probe: BackupProbeDto;
    sourcePath: string;
    credentialMode: 'password' | 'mnemonic';
    backupPassword: string;
    isBusy: boolean;
    onChangeSource: () => void;
    onCredentialModeChange: (mode: 'password' | 'mnemonic') => void;
    onBackupPasswordChange: (value: string) => void;
    onFileSeedChange: (value: SeedPhraseValue) => void;
    onInspect: () => void;
}

/**
 * Backup file/folder source summary and unlock credentials card (backup password
 * or mnemonic phrase) used to decrypt and inspect the backup manifest.
 */
export const FileUnlockCredentialsCard: React.FC<FileUnlockCredentialsCardProps> = ({
    probe,
    sourcePath,
    credentialMode,
    backupPassword,
    isBusy,
    onChangeSource,
    onCredentialModeChange,
    onBackupPasswordChange,
    onFileSeedChange,
    onInspect,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-4">
            <div className="p-4 bg-slate-50 border border-slate-200 rounded-2xl flex items-center justify-between gap-4">
                <div className="flex items-center gap-3 min-w-0">
                    <Folder size={18} className="text-theme-primary shrink-0" />
                    <div className="min-w-0">
                        <p className="text-[10px] font-black uppercase tracking-wider text-theme-light">
                            {probe.sourceKind === 'folder'
                                ? t('backup.sourceKindFolder')
                                : t('backup.sourceKindArchive')}
                        </p>
                        <p className="text-xs font-mono text-theme-secondary truncate max-w-md">
                            {sourcePath}
                        </p>
                    </div>
                </div>
                <Button variant="secondary" size="xs" onClick={onChangeSource}>
                    {t('common.change')}
                </Button>
            </div>

            <div className="space-y-4">
                <div className="flex items-center justify-between flex-wrap gap-2">
                    <h4 className="text-sm font-black uppercase tracking-widest text-theme-secondary flex items-center gap-2">
                        <KeyRound size={16} className="text-theme-primary" />
                        {t('profile.backupRestoreUnlockTitle')}
                    </h4>

                    {probe.supportsPasswordRestore && (
                        <div className="flex bg-slate-100 p-1 rounded-xl">
                            <button
                                type="button"
                                onClick={() => onCredentialModeChange('password')}
                                className={`px-3 py-1 text-xs font-bold rounded-lg transition-all cursor-pointer ${
                                    credentialMode === 'password'
                                        ? 'bg-white text-theme-primary shadow-sm'
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                {t('profile.backupRestorePasswordTab')}
                            </button>
                            <button
                                type="button"
                                onClick={() => onCredentialModeChange('mnemonic')}
                                className={`px-3 py-1 text-xs font-bold rounded-lg transition-all cursor-pointer ${
                                    credentialMode === 'mnemonic'
                                        ? 'bg-white text-theme-primary shadow-sm'
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                {t('profile.backupRestoreMnemonicTab')}
                            </button>
                        </div>
                    )}
                </div>

                {credentialMode === 'password' ? (
                    <div className="space-y-2 p-4 bg-white border border-theme-subtle rounded-2xl">
                        <label className="block text-xs font-bold text-theme-secondary">
                            {t('settings.backupPasswordLabel')}
                        </label>
                        <Input
                            data-testid="backup-password-input"
                            type="password"
                            showToggle
                            value={backupPassword}
                            onChange={(e) => onBackupPasswordChange(e.target.value)}
                            placeholder={t('settings.backupPasswordPlaceholder')}
                            className="font-mono"
                        />
                    </div>
                ) : (
                    <SeedPhraseInput
                        allowKitUri={false}
                        showUserPrefix
                        idPrefix="restore-file"
                        onChange={onFileSeedChange}
                    />
                )}

                <Button
                    variant="primary"
                    data-testid="inspect-file-backup-btn"
                    onClick={onInspect}
                    disabled={isBusy}
                    className="w-full"
                >
                    {isBusy ? (
                        <>
                            <RefreshCw size={14} className="animate-spin mr-2" />
                            {t('profile.backupRestoreVerifying')}
                        </>
                    ) : (
                        <>
                            <ShieldCheck size={16} className="mr-2" />
                            {t('profile.backupRestoreVerifyButton')}
                        </>
                    )}
                </Button>
            </div>
        </div>
    );
};

export interface BackupManifestCardProps {
    inspection: BackupInspectionDto;
    matchedSyncKeyKind?: string | null;
    syncSourceUrl?: string | null;
}

/**
 * Displays verified backup metadata (profile name, userId, blobs, epoch, txNonce, standards).
 */
export const BackupManifestCard: React.FC<BackupManifestCardProps> = ({
    inspection,
    matchedSyncKeyKind,
    syncSourceUrl,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-4">
            {matchedSyncKeyKind && (
                <div className="flex items-center justify-between px-4 py-2.5 bg-slate-50 border border-slate-200 rounded-2xl text-xs">
                    <span className="font-semibold text-theme-secondary flex items-center gap-2">
                        <Server size={15} className="text-theme-primary" />
                        {syncSourceUrl}
                    </span>
                    <span className="px-2 py-0.5 rounded-full bg-theme-primary/10 text-theme-primary font-bold text-[10px] uppercase">
                        {matchedSyncKeyKind}
                    </span>
                </div>
            )}

            <Card className="border-emerald-200 bg-emerald-50/50">
                <div className="space-y-4">
                    <div className="flex items-center justify-between">
                        <span className="px-2.5 py-1 rounded-full text-[10px] font-black uppercase tracking-wider bg-emerald-600 text-white flex items-center gap-1.5 shadow-sm">
                            <CheckCircle2 size={12} />
                            {t('profile.backupRestoreVerifiedBadge')}
                        </span>
                        <span className="text-xs text-emerald-800 font-mono">
                            {new Date(inspection.sealTimestamp).toLocaleString()}
                        </span>
                    </div>

                    <div>
                        <h3 className="text-base font-black text-theme-secondary">
                            {inspection.profileName}
                        </h3>
                        <p className="text-[11px] font-mono text-theme-light truncate">
                            {t('profile.userId')}: {inspection.userId}
                        </p>
                    </div>

                    <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 pt-2">
                        <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                            <FileText size={16} className="text-emerald-600 shrink-0" />
                            <div>
                                <p className="text-xs font-black text-theme-secondary">
                                    {inspection.blobCount}
                                </p>
                                <p className="text-[10px] text-theme-light">{t('backup.dataBlobs')}</p>
                            </div>
                        </div>
                        <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                            <History size={16} className="text-emerald-600 shrink-0" />
                            <div>
                                <p className="text-xs font-black text-theme-secondary">
                                    {inspection.epoch}
                                </p>
                                <p className="text-[10px] text-theme-light">{t('backup.epoch')}</p>
                            </div>
                        </div>
                        <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                            <Hash size={16} className="text-emerald-600 shrink-0" />
                            <div>
                                <p className="text-xs font-black text-theme-secondary">
                                    {inspection.txNonce}
                                </p>
                                <p className="text-[10px] text-theme-light">{t('backup.txNonce')}</p>
                            </div>
                        </div>
                        <div className="p-3 bg-white rounded-xl border border-emerald-100 flex items-center gap-2.5">
                            <Layers size={16} className="text-emerald-600 shrink-0" />
                            <div>
                                <p className="text-xs font-black text-theme-secondary">
                                    {inspection.customStandardCount}
                                </p>
                                <p className="text-[10px] text-theme-light">{t('backup.standards')}</p>
                            </div>
                        </div>
                    </div>
                </div>
            </Card>
        </div>
    );
};

const DevicePasswordInputs: React.FC<{
    newPassword: string;
    confirmNewPassword: string;
    onNewPasswordChange: (value: string) => void;
    onConfirmNewPasswordChange: (value: string) => void;
}> = ({
    newPassword,
    confirmNewPassword,
    onNewPasswordChange,
    onConfirmNewPasswordChange,
}) => {
    const { t } = useTranslation();
    return (
        <PasswordPairFields
            password={newPassword}
            confirmPassword={confirmNewPassword}
            onPasswordChange={onNewPasswordChange}
            onConfirmPasswordChange={onConfirmNewPasswordChange}
            passwordLabel={t('profile.backupRestoreNewPasswordLabel')}
            confirmLabel={t('profile.backupRestoreConfirmPasswordLabel')}
            passwordPlaceholder={t('auth.passwordMinChars')}
            confirmPlaceholder={t('auth.repeatPassword')}
            passwordTestId="new-device-password-input"
            confirmTestId="confirm-device-password-input"
            minLengthHint={null}
            showToggle
            autoComplete="new-password"
        />
    );
};

export interface DevicePasswordRestoreFormProps {
    collision: LocalProfileCollisionDto | null;
    collisionChoice: 'reset_pw' | 'replace';
    newPassword: string;
    confirmNewPassword: string;
    isBusy: boolean;
    onNewPasswordChange: (value: string) => void;
    onConfirmNewPasswordChange: (value: string) => void;
    onSubmit: (e: React.FormEvent) => void;
}

/**
 * Sets the local device password when finalizing a verified backup restore
 * (from sync or file/folder).
 */
export const DevicePasswordRestoreForm: React.FC<DevicePasswordRestoreFormProps> = ({
    collision,
    collisionChoice,
    newPassword,
    confirmNewPassword,
    isBusy,
    onNewPasswordChange,
    onConfirmNewPasswordChange,
    onSubmit,
}) => {
    const { t } = useTranslation();

    return (
        <form
            onSubmit={onSubmit}
            className="space-y-4 p-5 bg-white border border-theme-subtle rounded-3xl shadow-sm"
        >
            <div className="space-y-1">
                <h3 className="text-sm font-black uppercase tracking-widest text-theme-secondary flex items-center gap-2">
                    <Lock size={16} className="text-theme-primary" />
                    {t('profile.backupRestoreDevicePasswordTitle')}
                </h3>
                <p className="text-xs text-theme-light">
                    {t('profile.backupRestoreDevicePasswordDesc')}
                </p>
            </div>

            <div className="pt-2">
                <DevicePasswordInputs
                    newPassword={newPassword}
                    confirmNewPassword={confirmNewPassword}
                    onNewPasswordChange={onNewPasswordChange}
                    onConfirmNewPasswordChange={onConfirmNewPasswordChange}
                />
            </div>

            <Button
                data-testid="restore-submit-btn"
                type="submit"
                variant="primary"
                disabled={isBusy}
                className="w-full mt-2"
            >
                {isBusy ? (
                    <>
                        <RefreshCw size={14} className="animate-spin mr-2" />
                        {t('profile.backupRestoreRestoring')}
                    </>
                ) : (
                    <>
                        <ShieldCheck size={16} className="mr-2" />
                        {collision && collisionChoice === 'reset_pw'
                            ? t('backup.collisionResetPasswordBtn', { name: collision.profileName })
                            : collision && collisionChoice === 'replace'
                              ? t('backup.collisionReplaceProfileBtn', {
                                    name: collision.profileName,
                                })
                              : t('profile.backupRestoreSubmitButton')}
                    </>
                )}
            </Button>
        </form>
    );
};

export interface DirectProfileSetupFormProps {
    showNoSyncHint: boolean;
    userPrefix: string;
    profileName: string;
    newPassword: string;
    confirmNewPassword: string;
    isBusy: boolean;
    onProfileNameChange: (value: string) => void;
    onNewPasswordChange: (value: string) => void;
    onConfirmNewPasswordChange: (value: string) => void;
    onSwitchToSocial: () => void;
    onSubmit: (e: React.FormEvent) => void;
}

/**
 * Sets profile name and local device password when opening a profile directly
 * from recovery words without a backup file.
 */
export const DirectProfileSetupForm: React.FC<DirectProfileSetupFormProps> = ({
    showNoSyncHint,
    userPrefix,
    profileName,
    newPassword,
    confirmNewPassword,
    isBusy,
    onProfileNameChange,
    onNewPasswordChange,
    onConfirmNewPasswordChange,
    onSwitchToSocial,
    onSubmit,
}) => {
    const { t } = useTranslation();

    return (
        <form
            onSubmit={onSubmit}
            className="space-y-4 p-5 bg-white border border-theme-subtle rounded-3xl shadow-sm"
        >
            <div className="space-y-1">
                <h3 className="text-sm font-black uppercase tracking-wider text-theme-secondary flex items-center gap-2">
                    <Sparkles size={16} className="text-theme-primary" />
                    {userPrefix.trim()
                        ? t('backup.directSubaccountTitle', { prefix: userPrefix.trim() })
                        : t('backup.directOpenProfileTitle')}
                </h3>
                <p className="text-xs text-theme-light">
                    {t('backup.directOpenProfileDesc')}
                </p>
            </div>

            {showNoSyncHint && (
                <AlertBanner
                    variant="info"
                    data-testid="direct-open-no-sync-hint"
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            size="xs"
                            data-testid="switch-to-social-from-words-btn"
                            onClick={onSwitchToSocial}
                        >
                            {t('backup.restorePrimaryContacts')}
                        </Button>
                    }
                >
                    {t('backup.directOpenNoSyncWarning')}
                </AlertBanner>
            )}

            <div className="space-y-1.5">
                <label className="block text-xs font-bold text-theme-secondary">
                    {t('backup.profileNameLabel')}
                </label>
                <Input
                    data-testid="direct-profile-name-input"
                    type="text"
                    value={profileName}
                    onChange={(e) => onProfileNameChange(e.target.value)}
                    placeholder={t('profile.profileNamePlaceholder')}
                />
            </div>

            <DevicePasswordInputs
                newPassword={newPassword}
                confirmNewPassword={confirmNewPassword}
                onNewPasswordChange={onNewPasswordChange}
                onConfirmNewPasswordChange={onConfirmNewPasswordChange}
            />

            <Button
                data-testid="direct-open-profile-submit-btn"
                type="submit"
                variant="primary"
                disabled={isBusy}
                className="w-full mt-2"
            >
                {isBusy ? (
                    <>
                        <RefreshCw size={14} className="animate-spin mr-2" />
                        {t('profile.creatingProgress')}
                    </>
                ) : (
                    <>
                        <CheckCircle2 size={16} className="mr-2" />
                        {t('backup.directOpenProfileSubmit')}
                    </>
                )}
            </Button>
        </form>
    );
};
