// src/components/settings/SecurityBackupTab.tsx
import React, { useState, useEffect, FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import {
    Shield,
    Fingerprint,
    Eye,
    EyeOff,
    CheckCircle2,
    Clock,
    Save,
    AlertTriangle,
    Trash2,
    Users,
    FileText,
    ShieldCheck,
} from 'lucide-react';
import { AppSettings, PrivacyDefault, RecoveryProtectionStatusDto } from '../../types';
import { BackupSettingsCard } from '../BackupSettingsCard';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { HelpIcon } from '../ui/HelpIcon';
import { AlertBanner } from '../ui/AlertBanner';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { GuardianSetupModal } from '../GuardianSetupModal';
import { EmergencyKitModal } from '../EmergencyKitModal';
import { useSession } from '../../context/SessionContext';
import { useOptionalNavigation } from '../../context/NavigationContext';
import { authService } from '../../services/authService';
import { recoveryService } from '../../services/recoveryService';
import { translateError } from '../../utils/errorHelper';

export interface SecurityBackupTabProps {
    settings: AppSettings | null;
    onSettingsUpdated: (updater: AppSettings | ((prev: AppSettings | null) => AppSettings | null)) => void;
    onSave: (event: FormEvent) => void;
    isSaving: boolean;
    saveError: string;
    saveSuccess: string;
}

export const SecurityBackupTab: React.FC<SecurityBackupTabProps> = ({
    settings,
    onSettingsUpdated,
    onSave,
    isSaving,
    saveError,
    saveSuccess,
}) => {
    const { t } = useTranslation();
    const { profileName, notifyLogout, clearLocks, protectAction } = useSession();
    const nav = useOptionalNavigation();

    const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false);
    const [isDeleting, setIsDeleting] = useState(false);
    const [deleteError, setDeleteError] = useState('');

    const [protectionStatus, setProtectionStatus] = useState<RecoveryProtectionStatusDto | null>(null);
    const [isGuardianSetupOpen, setIsGuardianSetupOpen] = useState(false);
    const [isEmergencyKitOpen, setIsEmergencyKitOpen] = useState(false);
    const [isRevokingGuardians, setIsRevokingGuardians] = useState(false);
    const [isRevokeModalOpen, setIsRevokeModalOpen] = useState(false);
    const [revokeFeedback, setRevokeFeedback] = useState<{ type: 'error' | 'success'; text: string } | null>(null);

    const loadProtectionStatus = async () => {
        try {
            const st = await recoveryService.getRecoveryProtectionStatus();
            setProtectionStatus(st);
        } catch {
            // ignore
        }
    };

    useEffect(() => {
        loadProtectionStatus();
    }, []);

    const handleRevokeGuardians = async () => {
        setIsRevokingGuardians(true);
        setRevokeFeedback(null);
        try {
            await protectAction(async (pwd) => {
                await recoveryService.revokeSocialRecovery(pwd || '');
            });
            setRevokeFeedback({ type: 'success', text: t('backup.revokeGuardiansSuccess') });
            await loadProtectionStatus();
        } catch (err) {
            setRevokeFeedback({ type: 'error', text: translateError(err, t) });
        } finally {
            setIsRevokingGuardians(false);
            setIsRevokeModalOpen(false);
        }
    };

    const handleDeleteProfile = async () => {
        setIsDeleting(true);
        setDeleteError('');
        try {
            const profiles = await authService.listProfiles();
            const active = profiles.find(p => p.profileName === profileName) || profiles[0];
            if (!active) {
                throw new Error('No active profile found');
            }
            await protectAction(async (pwd) => {
                await authService.deleteProfile({
                    folderName: active.folderName,
                    password: pwd || undefined,
                });
            });
            clearLocks();
            notifyLogout();
            const remaining = await authService.listProfiles().catch(() => []);
            if (remaining.length > 0) {
                nav?.navigate({ view: 'needs_login' });
            } else {
                nav?.navigate({ view: 'needs_profile' });
            }
        } catch (err) {
            setDeleteError(translateError(err, t));
        } finally {
            setIsDeleting(false);
            setIsDeleteModalOpen(false);
        }
    };

    return (
        <form onSubmit={onSave} className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Encrypted Profile Backups */}
            <div id="settings-backup-section">
                <BackupSettingsCard
                    settings={settings}
                    onSettingsUpdated={(updated) => onSettingsUpdated(updated)}
                />
            </div>

            {/* Identity Recovery Section */}
            <div id="settings-recovery-section" className="space-y-6">
                <Card header={
                    <div className="flex items-center justify-between w-full">
                        <div className="flex items-center gap-2">
                            <ShieldCheck size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest">{t('backup.socialRecoveryTitle')}</span>
                        </div>
                        <span className={`text-[10px] font-bold px-2 py-0.5 rounded-full ${protectionStatus?.hasIdentityRecovery ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300' : 'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300'}`}>
                            {protectionStatus?.hasIdentityRecovery ? t('backup.emergencyKitConfirmedBadge') : t('backup.emergencyKitNotConfirmedBadge')}
                        </span>
                    </div>
                }>
                    <div className="space-y-6">
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                            {/* Emergency Sheet Card */}
                            <div className="p-4 rounded-2xl border border-theme-subtle bg-bg-surface flex flex-col justify-between space-y-4">
                                <div className="space-y-2">
                                    <div className="flex items-center justify-between">
                                        <div className="flex items-center gap-2">
                                            <FileText size={16} className="text-theme-primary" />
                                            <h4 className="text-xs font-bold text-theme-secondary">{t('backup.emergencyKitStatusLabel')}</h4>
                                        </div>
                                        <span className={`text-[9px] font-bold px-2 py-0.5 rounded-full ${protectionStatus?.emergencySheetConfirmedAt ? 'bg-emerald-100 text-emerald-800' : 'bg-slate-100 text-slate-600'}`}>
                                            {protectionStatus?.emergencySheetConfirmedAt ? t('backup.emergencyKitConfirmedBadge') : t('backup.emergencyKitNotConfirmedBadge')}
                                        </span>
                                    </div>
                                    <p className="text-[11px] text-theme-light leading-relaxed">{t('backup.emergencyKitDesc')}</p>
                                </div>
                                <Button
                                    type="button"
                                    variant="secondary"
                                    onClick={() => setIsEmergencyKitOpen(true)}
                                    className="w-full text-xs py-2 gap-1.5"
                                >
                                    <FileText size={14} />
                                    <span>{t('backup.emergencyKitPrintButton')}</span>
                                </Button>
                            </div>

                            {/* Social Recovery / Guardians Card */}
                            <div className="p-4 rounded-2xl border border-theme-subtle bg-bg-surface flex flex-col justify-between space-y-4">
                                <div className="space-y-2">
                                    <div className="flex items-center justify-between">
                                        <div className="flex items-center gap-2">
                                            <Users size={16} className="text-theme-primary" />
                                            <h4 className="text-xs font-bold text-theme-secondary">{t('backup.socialRecoveryStatusLabel')}</h4>
                                        </div>
                                        <span className={`text-[9px] font-bold px-2 py-0.5 rounded-full ${protectionStatus?.guardians && protectionStatus.guardians.length > 0 ? 'bg-emerald-100 text-emerald-800' : 'bg-slate-100 text-slate-600'}`}>
                                            {protectionStatus?.guardians && protectionStatus.guardians.length > 0
                                                ? `${protectionStatus.requiredThreshold}-von-${protectionStatus.guardians.length}`
                                                : t('backup.emergencyKitNotConfirmedBadge')}
                                        </span>
                                    </div>
                                    <p className="text-[11px] text-theme-light leading-relaxed">{t('backup.socialRecoveryDesc')}</p>
                                </div>
                                <div className="flex gap-2">
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        onClick={() => setIsGuardianSetupOpen(true)}
                                        className="flex-1 text-xs py-2 gap-1.5"
                                    >
                                        <Users size={14} />
                                        <span>{t('backup.manageGuardiansButton')}</span>
                                    </Button>
                                    {protectionStatus?.guardians && protectionStatus.guardians.length > 0 && (
                                        <Button
                                            type="button"
                                            variant="danger"
                                            onClick={() => setIsRevokeModalOpen(true)}
                                            className="text-xs py-2 px-3"
                                            title={t('backup.revokeGuardiansButton')}
                                        >
                                            <Trash2 size={14} />
                                        </Button>
                                    )}
                                </div>
                            </div>
                        </div>

                        {/* Guardian Health Status List (when configured) */}
                        {protectionStatus?.guardians && protectionStatus.guardians.length > 0 && (
                            <div className="space-y-2 pt-2 border-t border-theme-subtle/50">
                                <h5 className="text-[10px] font-black uppercase tracking-wider text-theme-light">
                                    {t('backup.emergencyKitGuardianNames')}
                                </h5>
                                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                                    {protectionStatus.guardians.map((g) => {
                                        const isConfirmed = g.status.status === 'confirmed';
                                        const isStale = g.isStale90d;
                                        return (
                                            <div key={g.userId} className="p-2.5 rounded-xl border border-theme-subtle/70 bg-bg-app flex items-center justify-between text-xs">
                                                <div className="min-w-0 pr-2">
                                                    <p className="font-bold text-theme-secondary truncate">{g.displayName || g.userId.slice(0, 16) + '...'}</p>
                                                    <p className="text-[10px] text-theme-light font-mono truncate">{g.userId.slice(0, 20)}...</p>
                                                </div>
                                                <span className={`text-[9px] font-bold px-2 py-0.5 rounded-full shrink-0 ${
                                                    isStale
                                                        ? 'bg-amber-100 text-amber-800'
                                                        : isConfirmed
                                                        ? 'bg-emerald-100 text-emerald-800'
                                                        : 'bg-slate-100 text-slate-600'
                                                }`}>
                                                    {isStale
                                                        ? t('backup.guardianStatusStale90d')
                                                        : isConfirmed
                                                        ? t('backup.guardianStatusConfirmed')
                                                        : t('backup.guardianStatusPending')}
                                                </span>
                                            </div>
                                        );
                                    })}
                                </div>
                            </div>
                        )}

                        {revokeFeedback && (
                            <AlertBanner variant={revokeFeedback.type}>{revokeFeedback.text}</AlertBanner>
                        )}
                    </div>
                </Card>
            </div>

            {/* Privacy Protocols */}
            <div id="settings-security-section">
                <Card header={
                    <div className="flex items-center gap-2">
                        <Shield size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">{t('settings.privacyProtocolsTitle')}</span>
                        <HelpIcon topic="settingsPrivacy" size={15} />
                    </div>
                }>
                    <div className="space-y-4">
                        <p className="text-xs text-theme-light font-medium mb-4">{t('settings.privacyProtocolsDescription')}</p>
                        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                            {[
                                { id: 'ask', label: t('settings.privacyModeInteractive'), icon: Fingerprint, desc: t('settings.privacyModeInteractiveDesc') },
                                { id: 'stealth', label: t('settings.privacyModeStealth'), icon: EyeOff, desc: t('settings.privacyModeStealthDesc') },
                                { id: 'public', label: t('settings.privacyModePublic'), icon: Eye, desc: t('settings.privacyModePublicDesc') }
                            ].map((mode) => (
                                <label 
                                    key={mode.id}
                                    className={`relative flex flex-col p-4 rounded-2xl border-2 transition-all cursor-pointer group ${settings?.privacyDefault === mode.id ? 'bg-theme-primary/5 border-theme-primary shadow-sm' : 'bg-white border-theme-subtle hover:border-theme-primary/30'}`}
                                >
                                    <input
                                        type="radio"
                                        name="privacyDefault"
                                        value={mode.id}
                                        checked={settings?.privacyDefault === mode.id}
                                        onChange={(e) => onSettingsUpdated(s => s ? { ...s, privacyDefault: e.target.value as PrivacyDefault } : null)}
                                        className="absolute opacity-0"
                                    />
                                    <mode.icon className={`mb-3 transition-colors ${settings?.privacyDefault === mode.id ? 'text-theme-primary' : 'text-theme-light group-hover:text-theme-secondary'}`} size={20} />
                                    <span className={`text-sm font-black tracking-tight ${settings?.privacyDefault === mode.id ? 'text-theme-primary' : 'text-theme-secondary'}`}>{mode.label}</span>
                                    <span className="text-[10px] font-medium text-theme-light mt-1">{mode.desc}</span>
                                    {settings?.privacyDefault === mode.id && (
                                        <div className="absolute top-3 right-3 text-theme-primary">
                                            <CheckCircle2 size={16} />
                                        </div>
                                    )}
                                </label>
                            ))}
                        </div>
                    </div>
                </Card>
            </div>

            {/* Session Control */}
            <Card header={
                <div className="flex items-center gap-2">
                    <Clock size={18} className="text-theme-primary" />
                    <span className="font-black text-xs uppercase tracking-widest">{t('settings.activeSessionTitle')}</span>
                </div>
            }>
                <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                    <div className="space-y-1">
                        <label className="text-xs font-bold text-theme-secondary">
                            {t('settings.timeoutIntervalLabel')}
                        </label>
                        <p className="text-[11px] text-theme-light/80 font-medium leading-relaxed">
                            {t('settings.timeoutIntervalDescription')}
                        </p>
                    </div>
                    <div className="w-full sm:w-36 shrink-0">
                        <Input
                            type="number"
                            value={settings ? Math.floor((settings.sessionTimeoutSeconds || 0) / 60) : 10}
                            onChange={(e) => {
                                const minutes = parseInt(e.target.value, 10);
                                onSettingsUpdated(s => s ? { ...s, sessionTimeoutSeconds: isNaN(minutes) ? 0 : minutes * 60 } : null);
                            }}
                            min="0"
                            className="font-bold"
                        />
                    </div>
                </div>
            </Card>

            {/* Save Button */}
            <div className="pt-2 flex flex-col items-center gap-4">
                <Button type="submit" disabled={isSaving} className="min-w-[240px] gap-2 rounded-2xl py-4 shadow-lg shadow-theme-primary/20">
                    {isSaving ? <Clock className="animate-spin" size={18} /> : <Save size={18} />}
                    {isSaving ? t('settings.saving') : t('settings.saveButton')}
                </Button>
                {saveError && <AlertBanner variant="error" className="w-full max-w-md">{saveError}</AlertBanner>}
                {saveSuccess && <AlertBanner variant="success" className="w-full max-w-md">{saveSuccess}</AlertBanner>}
            </div>

            {/* Gefahrenzone (Danger Zone) */}
            <div className="p-6 rounded-2xl border-2 border-rose-500/30 bg-rose-500/5 space-y-4">
                <div className="flex items-start gap-3">
                    <AlertTriangle size={20} className="text-rose-600 dark:text-rose-400 mt-0.5 shrink-0" />
                    <div className="space-y-1">
                        <h4 className="text-xs font-black uppercase tracking-wider text-rose-700 dark:text-rose-300">
                            {t('settings.dangerZoneTitle')}
                        </h4>
                        <p className="text-xs text-rose-800/80 dark:text-rose-200/80 leading-relaxed font-medium">
                            {t('settings.dangerZoneDesc')}
                        </p>
                    </div>
                </div>

                {deleteError && (
                    <AlertBanner variant="error">{deleteError}</AlertBanner>
                )}

                <div className="pt-2">
                    <Button
                        type="button"
                        variant="danger"
                        onClick={() => setIsDeleteModalOpen(true)}
                        className="gap-2 text-xs"
                    >
                        <Trash2 size={14} />
                        <span>{t('settings.deleteProfileLocalBtn')}</span>
                    </Button>
                </div>
            </div>

            {/* Confirmation Modal for Profile Purge */}
            <ConfirmationModal
                isOpen={isDeleteModalOpen}
                title={t('settings.deleteProfileConfirmTitle')}
                description={t('settings.deleteProfileConfirmDesc')}
                confirmVariant="danger"
                confirmText={t('common.delete')}
                isProcessing={isDeleting}
                onConfirm={handleDeleteProfile}
                onCancel={() => {
                    setIsDeleteModalOpen(false);
                    setDeleteError('');
                }}
            />

            {/* Guardian Setup Modal */}
            <GuardianSetupModal
                isOpen={isGuardianSetupOpen}
                onClose={() => {
                    setIsGuardianSetupOpen(false);
                    loadProtectionStatus();
                }}
            />

            {/* Emergency Kit Modal */}
            <EmergencyKitModal
                isOpen={isEmergencyKitOpen}
                onClose={() => {
                    setIsEmergencyKitOpen(false);
                    loadProtectionStatus();
                }}
            />

            {/* Confirmation Modal for Revoking Guardians */}
            <ConfirmationModal
                isOpen={isRevokeModalOpen}
                title={t('backup.revokeGuardiansButton')}
                description={t('backup.guardianHintHousehold')}
                confirmVariant="danger"
                confirmText={t('backup.revokeGuardiansButton')}
                isProcessing={isRevokingGuardians}
                onConfirm={handleRevokeGuardians}
                onCancel={() => setIsRevokeModalOpen(false)}
            />
        </form>
    );
};

export default SecurityBackupTab;
