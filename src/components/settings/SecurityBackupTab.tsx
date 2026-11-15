// src/components/settings/SecurityBackupTab.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    Shield,
    Fingerprint,
    Eye,
    EyeOff,
    CheckCircle2,
    Clock,
} from 'lucide-react';
import { AppSettings, PrivacyDefault } from '../../types';
import { BackupSettingsCard } from '../BackupSettingsCard';
import { IdentityRecoveryCard } from './IdentityRecoveryCard';
import { DangerZoneCard } from './DangerZoneCard';
import { Card } from '../ui/Card';
import { Input } from '../ui/Input';
import { HelpIcon } from '../ui/HelpIcon';

export interface SecurityBackupTabProps {
    settings: AppSettings | null;
    onPersistSettings: (updater: AppSettings | ((prev: AppSettings | null) => AppSettings | null)) => void;
    onSettingsSynced: (newSettings: AppSettings) => void;
}

export const SecurityBackupTab: React.FC<SecurityBackupTabProps> = ({
    settings,
    onPersistSettings,
    onSettingsSynced,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Encrypted Profile Backups */}
            <div id="settings-backup-section">
                <BackupSettingsCard
                    settings={settings}
                    onSettingsUpdated={onSettingsSynced}
                />
            </div>

            {/* Identity Recovery Section */}
            <IdentityRecoveryCard />

            {/* Privacy Protocols */}
            <div id="settings-security-section">
                <Card
                    header={
                        <div className="flex items-center gap-2">
                            <Shield size={18} className="text-theme-primary" />
                            <span className="font-black text-xs uppercase tracking-widest">
                                {t('settings.privacyProtocolsTitle')}
                            </span>
                            <HelpIcon topic="settingsPrivacy" size={15} />
                        </div>
                    }
                >
                    <div className="space-y-4">
                        <p className="text-xs text-theme-light font-medium mb-4">
                            {t('settings.privacyProtocolsDescription')}
                        </p>
                        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                            {[
                                {
                                    id: 'ask',
                                    label: t('settings.privacyModeInteractive'),
                                    icon: Fingerprint,
                                    desc: t('settings.privacyModeInteractiveDesc'),
                                },
                                {
                                    id: 'stealth',
                                    label: t('settings.privacyModeStealth'),
                                    icon: EyeOff,
                                    desc: t('settings.privacyModeStealthDesc'),
                                },
                                {
                                    id: 'public',
                                    label: t('settings.privacyModePublic'),
                                    icon: Eye,
                                    desc: t('settings.privacyModePublicDesc'),
                                },
                            ].map((mode) => (
                                <label
                                    key={mode.id}
                                    className={`relative flex flex-col p-4 rounded-2xl border-2 transition-all cursor-pointer group ${
                                        settings?.privacyDefault === mode.id
                                            ? 'bg-theme-primary/5 border-theme-primary shadow-sm'
                                            : 'bg-white border-theme-subtle hover:border-theme-primary/30'
                                    }`}
                                >
                                    <input
                                        type="radio"
                                        name="privacyDefault"
                                        value={mode.id}
                                        checked={settings?.privacyDefault === mode.id}
                                        onChange={(e) =>
                                            onPersistSettings((s) =>
                                                s
                                                    ? { ...s, privacyDefault: e.target.value as PrivacyDefault }
                                                    : null
                                            )
                                        }
                                        className="absolute opacity-0"
                                    />
                                    <mode.icon
                                        className={`mb-3 transition-colors ${
                                            settings?.privacyDefault === mode.id
                                                ? 'text-theme-primary'
                                                : 'text-theme-light group-hover:text-theme-secondary'
                                        }`}
                                        size={20}
                                    />
                                    <span
                                        className={`text-sm font-black tracking-tight ${
                                            settings?.privacyDefault === mode.id
                                                ? 'text-theme-primary'
                                                : 'text-theme-secondary'
                                        }`}
                                    >
                                        {mode.label}
                                    </span>
                                    <span className="text-[10px] font-medium text-theme-light mt-1">
                                        {mode.desc}
                                    </span>
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
            <Card
                header={
                    <div className="flex items-center gap-2">
                        <Clock size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">
                            {t('settings.activeSessionTitle')}
                        </span>
                    </div>
                }
            >
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
                                onPersistSettings((s) =>
                                    s ? { ...s, sessionTimeoutSeconds: isNaN(minutes) ? 0 : minutes * 60 } : null
                                );
                            }}
                            min="0"
                            className="font-bold"
                        />
                    </div>
                </div>
            </Card>

            {/* Danger Zone */}
            <div id="settings-danger-section">
                <DangerZoneCard />
            </div>
        </div>
    );
};

export default SecurityBackupTab;
