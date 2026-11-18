import React, { FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import {
    AlertTriangle,
    ArrowRight,
    CheckCircle2,
    Fingerprint,
    HelpCircle,
    Info,
    Lock,
    RefreshCw,
    ShieldCheck,
    User,
} from 'lucide-react';
import { Button } from '../ui/Button';
import { Card } from '../ui/Card';
import { HelpIcon } from '../ui/HelpIcon';
import { Input } from '../ui/Input';
import { PasswordPairFields } from '../ui/PasswordPairFields';
import { PrefixInfoModal } from '../ui/PrefixInfoModal';

export interface CreateOrRestoreChoiceHeaderProps {
    profilesCount: number | null;
    onSwitchToRestore?: (method?: 'social' | 'words_or_sheet' | 'file_or_folder') => void;
}

/**
 * Onboarding switcher shown when no local profiles exist yet.
 */
export const CreateOrRestoreChoiceHeader: React.FC<CreateOrRestoreChoiceHeaderProps> = ({
    profilesCount,
    onSwitchToRestore,
}) => {
    const { t } = useTranslation();

    if (profilesCount !== 0 || !onSwitchToRestore) {
        return null;
    }

    return (
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 mb-2">
            <div className="p-4 rounded-2xl bg-theme-primary/10 border-2 border-theme-primary text-theme-primary font-bold text-xs flex items-center justify-between shadow-sm">
                <span>{t('auth.createNewProfileOption')}</span>
                <CheckCircle2 size={16} />
            </div>
            <button
                type="button"
                onClick={() => onSwitchToRestore()}
                className="p-4 rounded-2xl bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary/50 text-theme-secondary font-bold text-xs text-left transition-all flex items-center justify-between cursor-pointer"
            >
                <span>{t('auth.restoreExistingProfileOption')}</span>
                <ArrowRight size={14} className="text-theme-light" />
            </button>
        </div>
    );
};

export interface ProfileCredentialsCardProps {
    profileName: string;
    password: string;
    confirmPassword: string;
    onProfileNameChange: (value: string) => void;
    onPasswordChange: (value: string) => void;
    onConfirmPasswordChange: (value: string) => void;
    passwordRef?: React.Ref<HTMLInputElement>;
}

/**
 * Shared profile name and access password pair card used across both standard
 * quick start and expert onboarding step 3.
 */
export const ProfileCredentialsCard: React.FC<ProfileCredentialsCardProps> = ({
    profileName,
    password,
    confirmPassword,
    onProfileNameChange,
    onPasswordChange,
    onConfirmPasswordChange,
    passwordRef,
}) => {
    const { t } = useTranslation();

    return (
        <Card
            header={
                <div className="flex items-center gap-2">
                    <Lock size={14} />
                    <span className="font-black text-[10px] uppercase tracking-widest">
                        {t('profile.profileDetails')}
                    </span>
                </div>
            }
        >
            <div className="space-y-4">
                <div className="space-y-2">
                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5">
                        <User size={10} /> {t('profile.profileLabel')}
                    </label>
                    <Input
                        value={profileName}
                        onChange={(e) => onProfileNameChange(e.target.value)}
                        placeholder={t('profile.profileNamePlaceholder')}
                        required
                    />
                </div>
                <PasswordPairFields
                    password={password}
                    confirmPassword={confirmPassword}
                    onPasswordChange={onPasswordChange}
                    onConfirmPasswordChange={onConfirmPasswordChange}
                    passwordLabel={t('profile.accessPassword')}
                    confirmLabel={t('profile.confirmPassword')}
                    passwordPlaceholder="••••••••"
                    confirmPlaceholder="••••••••"
                    passwordRef={passwordRef}
                    minLengthHint={null}
                    showToggle={false}
                    required
                />
            </div>
        </Card>
    );
};

export interface StandardQuickCreateStepProps {
    profilesCount: number | null;
    profileName: string;
    password: string;
    confirmPassword: string;
    isLoading: boolean;
    onProfileNameChange: (value: string) => void;
    onPasswordChange: (value: string) => void;
    onConfirmPasswordChange: (value: string) => void;
    onSubmit: (e: FormEvent) => void;
    onEnterExpertMode: () => void;
    onSwitchToLogin?: () => void;
    onSwitchToRestore?: (method?: 'social' | 'words_or_sheet' | 'file_or_folder') => void;
}

/**
 * Standard beginner-friendly quick profile creation view.
 */
export const StandardQuickCreateStep: React.FC<StandardQuickCreateStepProps> = ({
    profilesCount,
    profileName,
    password,
    confirmPassword,
    isLoading,
    onProfileNameChange,
    onPasswordChange,
    onConfirmPasswordChange,
    onSubmit,
    onEnterExpertMode,
    onSwitchToLogin,
    onSwitchToRestore,
}) => {
    const { t } = useTranslation();

    return (
        <div className="space-y-6 sm:space-y-8 animate-in fade-in duration-500">
            <CreateOrRestoreChoiceHeader
                profilesCount={profilesCount}
                onSwitchToRestore={onSwitchToRestore}
            />

            <div className="text-center space-y-2">
                <h2 className="text-2xl font-black text-theme-primary tracking-tight">
                    {t('auth.standardCreationTitle')}
                </h2>
                <p className="text-sm font-medium text-theme-light">
                    {t('auth.standardCreationSubtitle')}
                </p>
            </div>

            <form onSubmit={onSubmit} className="space-y-6">
                <ProfileCredentialsCard
                    profileName={profileName}
                    password={password}
                    confirmPassword={confirmPassword}
                    onProfileNameChange={onProfileNameChange}
                    onPasswordChange={onPasswordChange}
                    onConfirmPasswordChange={onConfirmPasswordChange}
                />

                <div
                    data-testid="simple-mode-notice"
                    className="p-3.5 rounded-2xl bg-theme-primary/5 border border-theme-primary/15 text-xs text-theme-secondary leading-relaxed flex items-start gap-2.5"
                >
                    <Info size={16} className="text-theme-primary shrink-0 mt-0.5" />
                    <span>{t('profile.simpleModeNotice')}</span>
                </div>

                <Button
                    type="submit"
                    disabled={isLoading || !password || password !== confirmPassword}
                    data-testid="onboarding-finish-button"
                    className="w-full py-4 rounded-3xl shadow-premium-lg text-base gap-3"
                >
                    {isLoading ? (
                        <RefreshCw className="animate-spin" size={20} />
                    ) : (
                        <Fingerprint size={20} />
                    )}
                    {isLoading ? t('profile.creating') : t('profile.create')}
                </Button>
            </form>

            <div className="pt-4 border-t border-theme-primary/10 text-center space-y-3">
                <button
                    type="button"
                    onClick={onEnterExpertMode}
                    className="text-xs font-bold text-theme-light hover:text-theme-primary transition-colors underline cursor-pointer"
                >
                    {t('auth.expertOptionsLink')}
                </button>

                {onSwitchToLogin && (
                    <div>
                        <button
                            type="button"
                            onClick={onSwitchToLogin}
                            className="text-xs text-theme-light hover:text-theme-secondary transition-colors cursor-pointer"
                        >
                            {t('auth.backToLogin')}
                        </button>
                    </div>
                )}
            </div>
        </div>
    );
};

export interface ExpertDeviceSettingsCardProps {
    isSubAccount: boolean;
    userPrefix: string;
    retainMnemonic: boolean;
    showPrefixInfo: boolean;
    onSubAccountChange: (checked: boolean) => void;
    onUserPrefixChange: (value: string) => void;
    onRetainMnemonicChange: (retain: boolean) => void;
    onOpenPrefixInfo: () => void;
    onClosePrefixInfo: () => void;
}

/**
 * Expert onboarding step 3 device settings card (sub-account prefix & mnemonic retention).
 */
export const ExpertDeviceSettingsCard: React.FC<ExpertDeviceSettingsCardProps> = ({
    isSubAccount,
    userPrefix,
    retainMnemonic,
    showPrefixInfo,
    onSubAccountChange,
    onUserPrefixChange,
    onRetainMnemonicChange,
    onOpenPrefixInfo,
    onClosePrefixInfo,
}) => {
    const { t } = useTranslation();

    return (
        <>
            <Card
                header={
                    <div className="flex items-center gap-2">
                        <ShieldCheck size={14} />
                        <span className="font-black text-[10px] uppercase tracking-widest">
                            {t('profile.deviceSettings')}
                        </span>
                    </div>
                }
            >
                <div className="space-y-4">
                    <div className="flex items-start gap-2.5 py-1">
                        <input
                            id="checkbox-sub-account"
                            type="checkbox"
                            checked={isSubAccount}
                            onChange={(e) => onSubAccountChange(e.target.checked)}
                            className="mt-1 h-4.5 w-4.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary/30 cursor-pointer"
                        />
                        <div className="flex flex-col">
                            <div className="flex items-center gap-1.5">
                                <label
                                    htmlFor="checkbox-sub-account"
                                    className="text-xs font-bold text-theme-secondary cursor-pointer select-none"
                                >
                                    {t('profile.subAccountOption')}
                                </label>
                                <HelpIcon topic="subaccount" size={12} />
                            </div>
                            <p className="text-[10px] font-medium text-theme-light leading-normal mt-0.5">
                                {t('profile.subAccountDescription')}
                            </p>
                        </div>
                    </div>

                    {isSubAccount && (
                        <>
                            <div className="space-y-2 pt-2 border-t border-theme-primary/5 animate-in fade-in duration-300">
                                <div className="flex items-center justify-between">
                                    <label className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5">
                                        {t('profile.subAccountName')}
                                    </label>
                                    <button
                                        type="button"
                                        onClick={onOpenPrefixInfo}
                                        className="text-[9px] font-black uppercase tracking-widest text-theme-primary hover:bg-theme-primary/10 transition-all flex items-center gap-1.5 bg-theme-primary/5 px-2.5 py-1 rounded-full border border-theme-primary/20"
                                    >
                                        <HelpCircle size={12} />
                                        <span>{t('profile.readInstructions')}</span>
                                    </button>
                                </div>
                                <Input
                                    value={userPrefix}
                                    onChange={(e) => onUserPrefixChange(e.target.value)}
                                    placeholder={t('profile.prefixPlaceholder')}
                                />
                            </div>

                            <div className="space-y-1.5 p-3.5 bg-theme-subtle/20 border border-theme-subtle rounded-2xl animate-in fade-in duration-300">
                                <p className="text-[10px] text-theme-secondary leading-relaxed">
                                    {t('profile.prefixDescription')}
                                </p>
                                <p className="text-[9px] font-black text-rose-600 flex items-start gap-1.5 leading-tight italic">
                                    <AlertTriangle size={10} className="shrink-0 mt-0.5" />
                                    {t('profile.criticalPrefix')}
                                </p>
                            </div>
                        </>
                    )}

                    <div className="flex items-start gap-2.5 py-1 pt-2 border-t border-theme-primary/5">
                        <input
                            id="checkbox-retain-mnemonic"
                            type="checkbox"
                            checked={!retainMnemonic}
                            onChange={(e) => onRetainMnemonicChange(!e.target.checked)}
                            className="mt-1 h-4.5 w-4.5 rounded border-theme-subtle text-theme-primary focus:ring-theme-primary/30 cursor-pointer"
                        />
                        <div className="flex flex-col">
                            <label
                                htmlFor="checkbox-retain-mnemonic"
                                className="text-xs font-bold text-theme-secondary cursor-pointer select-none"
                            >
                                {t('auth.doNotRetainMnemonic')}
                            </label>
                            <p className="text-[10px] font-medium text-theme-light leading-normal mt-0.5">
                                {t('auth.doNotRetainMnemonicHint')}
                            </p>
                        </div>
                    </div>
                </div>
            </Card>

            <PrefixInfoModal isOpen={showPrefixInfo} onClose={onClosePrefixInfo} />
        </>
    );
};
