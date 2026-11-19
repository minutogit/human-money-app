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
import { AppConfigDto, PrivacyDefault, SecurityPolicyPatch } from '../../types';
import { BackupSettingsCard } from './BackupSettingsCard';
import { QuickPinCard } from './QuickPinCard';
import { IdentityRecoveryCard } from './IdentityRecoveryCard';
import { DangerZoneCard } from './DangerZoneCard';
import { SessionTimeoutControl } from './ViewAndLockCard';
import { Card } from '../ui/Card';
import { HelpIcon } from '../ui/HelpIcon';

export interface SecurityBackupTabProps {
    config: AppConfigDto | null;
    onPatchSecurityPolicy: (patch: SecurityPolicyPatch) => Promise<boolean | void> | void;
    onConfigUpdated: (newConfig: AppConfigDto) => void;
}

export const SecurityBackupTab: React.FC<SecurityBackupTabProps> = ({
    config,
    onPatchSecurityPolicy,
    onConfigUpdated,
}) => {
    const { t } = useTranslation();
    const privacyDefault = config?.securityPolicy.privacyDefault;

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Encrypted Profile Backups */}
            <div id="settings-backup-section">
                <BackupSettingsCard
                    config={config}
                    onConfigUpdated={onConfigUpdated}
                />
            </div>

            {/* Quick PIN Authentication */}
            <div id="settings-pin-section">
                <QuickPinCard />
            </div>

            {/* Identity Recovery Section */}
            <div id="settings-security-section">
                <IdentityRecoveryCard />
            </div>

            {/* Privacy Protocols */}
            <div id="settings-privacy-section">
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
                                        privacyDefault === mode.id
                                            ? 'bg-theme-primary/5 border-theme-primary shadow-sm'
                                            : 'bg-white border-theme-subtle hover:border-theme-primary/30'
                                    }`}
                                >
                                    <input
                                        type="radio"
                                        name="privacyDefault"
                                        value={mode.id}
                                        checked={privacyDefault === mode.id}
                                        onChange={(e) =>
                                            void onPatchSecurityPolicy({
                                                privacyDefault: e.target.value as PrivacyDefault,
                                            })
                                        }
                                        className="absolute opacity-0"
                                    />
                                    <mode.icon
                                        className={`mb-3 transition-colors ${
                                            privacyDefault === mode.id
                                                ? 'text-theme-primary'
                                                : 'text-theme-light group-hover:text-theme-secondary'
                                        }`}
                                        size={20}
                                    />
                                    <span
                                        className={`text-sm font-black tracking-tight ${
                                            privacyDefault === mode.id
                                                ? 'text-theme-primary'
                                                : 'text-theme-secondary'
                                        }`}
                                    >
                                        {mode.label}
                                    </span>
                                    <span className="text-[10px] font-medium text-theme-light mt-1">
                                        {mode.desc}
                                    </span>
                                    {privacyDefault === mode.id && (
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
                <SessionTimeoutControl
                    inputId="securityBackupTimeoutInput"
                    sessionTimeoutSeconds={config?.securityPolicy.sessionTimeoutSeconds ?? 600}
                    onSaveTimeout={(seconds) =>
                        onPatchSecurityPolicy({ sessionTimeoutSeconds: seconds })
                    }
                />
            </Card>

            {/* Danger Zone */}
            <div id="settings-danger-section">
                <DangerZoneCard />
            </div>
        </div>
    );
};

export default SecurityBackupTab;
