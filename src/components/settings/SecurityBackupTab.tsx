// src/components/settings/SecurityBackupTab.tsx
import React, { useState, FormEvent } from 'react';
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
    Trash2
} from 'lucide-react';
import { AppSettings, PrivacyDefault } from '../../types';
import { BackupSettingsCard } from '../BackupSettingsCard';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { HelpIcon } from '../ui/HelpIcon';
import { AlertBanner } from '../ui/AlertBanner';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { useSession } from '../../context/SessionContext';
import { useOptionalNavigation } from '../../context/NavigationContext';
import { authService } from '../../services/authService';
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
        </form>
    );
};

export default SecurityBackupTab;
