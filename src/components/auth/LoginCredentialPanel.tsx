import React from 'react';
import { useTranslation } from 'react-i18next';
import {
    ArrowRight,
    CheckCircle2,
    Key,
    KeyRound,
    Lock,
    LogIn,
    PlusCircle,
    RefreshCw,
    ShieldAlert,
    Trash2,
    UserCircle,
} from 'lucide-react';
import { ProfilePinStatusDto } from '../../services/authService';
import { ProfileInfo } from '../../types';
import { Button } from '../ui/Button';
import { HelpIcon } from '../ui/HelpIcon';
import { Input } from '../ui/Input';

export interface LoginOnboardingCardsProps {
    onSwitchToCreate: () => void;
    onSwitchToRestoreBackup: () => void;
}

/**
 * Two-column shortcut cards for creating a new profile or opening/restoring an existing one.
 */
export const LoginOnboardingCards: React.FC<LoginOnboardingCardsProps> = ({
    onSwitchToCreate,
    onSwitchToRestoreBackup,
}) => {
    const { t } = useTranslation();

    return (
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-5 pt-2">
            <button
                type="button"
                onClick={onSwitchToCreate}
                className="p-6 bg-slate-50 border border-slate-200 hover:border-theme-primary/40 hover:bg-theme-primary/5 rounded-[28px] text-center group transition-all flex flex-col items-center justify-center gap-3 cursor-pointer shadow-sm hover:shadow-md"
            >
                <div className="w-12 h-12 rounded-full bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-110 transition-transform">
                    <PlusCircle size={26} className="text-theme-primary" />
                </div>
                <div className="space-y-1">
                    <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary group-hover:text-theme-primary">
                        {t('auth.createNewWalletTitle')}
                    </h3>
                    <p className="text-[10px] font-medium text-theme-light">
                        {t('auth.createNewWalletSub')}
                    </p>
                </div>
            </button>
            <button
                type="button"
                onClick={onSwitchToRestoreBackup}
                className="p-6 bg-slate-50 border border-slate-200 hover:border-theme-primary/40 hover:bg-theme-primary/5 rounded-[28px] text-center group transition-all flex flex-col items-center justify-center gap-3 cursor-pointer shadow-sm hover:shadow-md"
            >
                <div className="w-12 h-12 rounded-full bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-110 transition-transform">
                    <RefreshCw size={24} className="text-theme-primary" />
                </div>
                <div className="space-y-1">
                    <h3 className="text-xs font-black uppercase tracking-wider text-theme-secondary group-hover:text-theme-primary">
                        {t('auth.loadExistingWalletTitle')}
                    </h3>
                    <p className="text-[10px] font-medium text-theme-light">
                        {t('auth.loadExistingWalletSub')}
                    </p>
                </div>
            </button>
        </div>
    );
};

export interface LoginCredentialPanelProps {
    profiles: ProfileInfo[];
    selectedProfile: string;
    onSelectProfile: (folderName: string) => void;
    onRequestDelete: () => void;
    authMethod: 'pin' | 'password';
    onSwitchAuthMethod: (method: 'pin' | 'password') => void;
    pinStatus: ProfilePinStatusDto | null;
    pin: string;
    onPinChange: (value: string) => void;
    password: string;
    onPasswordChange: (value: string) => void;
    pinInputRef: React.RefObject<HTMLInputElement | null>;
    passwordInputRef: React.RefObject<HTMLInputElement | null>;
    showHandoverUI: boolean;
    onCancelHandover: () => void;
    onHandover: () => void;
    isLoading: boolean;
    isLoggingIn: boolean;
    onSubmit: () => void;
    onSwitchToCreate: () => void;
    onSwitchToRestoreBackup: () => void;
    onSwitchToReset: () => void;
    feedbackMsg: string;
    feedbackType: 'error' | 'success' | 'info' | null;
}

/**
 * Login form panel handling profile selection, Quick PIN vs master password
 * entry, device-mismatch handover action, and error/status feedback.
 */
export const LoginCredentialPanel: React.FC<LoginCredentialPanelProps> = ({
    profiles,
    selectedProfile,
    onSelectProfile,
    onRequestDelete,
    authMethod,
    onSwitchAuthMethod,
    pinStatus,
    pin,
    onPinChange,
    password,
    onPasswordChange,
    pinInputRef,
    passwordInputRef,
    showHandoverUI,
    onCancelHandover,
    onHandover,
    isLoading,
    isLoggingIn,
    onSubmit,
    onSwitchToCreate,
    onSwitchToRestoreBackup,
    onSwitchToReset,
    feedbackMsg,
    feedbackType,
}) => {
    const { t } = useTranslation();
    const isPinMode = authMethod === 'pin' && Boolean(pinStatus?.enabled) && !showHandoverUI;

    return (
        <form
            className="space-y-6"
            onSubmit={(e) => {
                e.preventDefault();
                onSubmit();
            }}
        >
            <div className="space-y-3">
                <div className="flex items-center gap-1.5">
                    <label
                        htmlFor="profile-select"
                        className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1"
                    >
                        <UserCircle size={12} />
                        <span>{t('profile.selectProfile')}</span>
                    </label>
                    <HelpIcon topic="profile" size={12} />
                </div>
                <div className="flex gap-3">
                    <div className="relative flex-1 group">
                        <select
                            id="profile-select"
                            value={selectedProfile}
                            onChange={(e) => onSelectProfile(e.target.value)}
                            className="w-full bg-white border border-theme-subtle rounded-2xl px-5 py-4 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all group-hover:border-theme-primary/30"
                        >
                            {profiles.map((profile) => (
                                <option key={profile.folderName} value={profile.folderName}>
                                    {profile.profileName}
                                </option>
                            ))}
                        </select>
                        <div className="absolute right-5 top-1/2 -translate-y-1/2 pointer-events-none text-theme-light">
                            <ArrowRight size={16} className="rotate-90" />
                        </div>
                    </div>
                    <button
                        type="button"
                        onClick={onRequestDelete}
                        className="w-14 h-14 rounded-2xl bg-slate-50 border border-slate-200 text-slate-400 hover:text-rose-500 hover:bg-rose-50 hover:border-rose-200 transition-all flex items-center justify-center shadow-sm cursor-pointer"
                        title={t('profile.deleteProfile')}
                    >
                        <Trash2 size={22} />
                    </button>
                </div>
            </div>

            {isPinMode && pinStatus ? (
                <div className="space-y-3">
                    <div className="flex items-center justify-between gap-2">
                        <label
                            htmlFor="pin-input"
                            className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"
                        >
                            <KeyRound size={12} /> {t('auth.quickPinLabel')}
                        </label>
                        <span
                            data-testid="login-pin-scope-badge"
                            className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-sky-50 text-sky-800 border border-sky-200"
                        >
                            {t('auth.quickPinModeBadge')}
                        </span>
                    </div>
                    <Input
                        id="pin-input"
                        type="password"
                        inputMode="numeric"
                        autoComplete="off"
                        maxLength={8}
                        value={pin}
                        onChange={(e) => onPinChange(e.target.value.replace(/\D/g, '').slice(0, 8))}
                        placeholder={t('settings.pinInputPlaceholder')}
                        ref={pinInputRef}
                        data-testid="login-pin-input"
                        className="py-5 px-6 text-lg font-bold tracking-widest"
                    />
                    <div className="flex items-center justify-between text-[11px] text-theme-light pt-0.5">
                        <span>
                            {t('auth.pinAttemptsRemaining', { count: pinStatus.remainingAttempts })}
                        </span>
                        <button
                            type="button"
                            onClick={() => onSwitchAuthMethod('password')}
                            data-testid="switch-to-password-btn"
                            className="font-bold text-theme-primary hover:underline cursor-pointer"
                        >
                            {t('auth.useMasterPasswordInstead')}
                        </button>
                    </div>
                </div>
            ) : (
                <div className="space-y-3">
                    <div className="flex items-center justify-between gap-2">
                        <label
                            htmlFor="password-input"
                            className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center gap-1.5"
                        >
                            <Lock size={12} /> {t('auth.password')}
                        </label>
                        {pinStatus?.enabled && !showHandoverUI && (
                            <button
                                type="button"
                                onClick={() => onSwitchAuthMethod('pin')}
                                data-testid="switch-to-pin-btn"
                                className="text-[11px] font-bold text-theme-primary hover:underline cursor-pointer"
                            >
                                {t('auth.useQuickPinInstead')}
                            </button>
                        )}
                    </div>
                    <Input
                        id="password-input"
                        type="password"
                        value={password}
                        onChange={(e) => onPasswordChange(e.target.value)}
                        placeholder={t('auth.enterAccessPassword')}
                        ref={passwordInputRef}
                        className="py-5 px-6 text-lg font-bold tracking-widest"
                    />
                </div>
            )}

            <div className="space-y-6 pt-4">
                {showHandoverUI ? (
                    <div className="space-y-4 animate-in slide-in-from-bottom-4">
                        <div className="flex items-center justify-between">
                            <span className="text-[10px] font-black text-amber-600 uppercase tracking-widest flex items-center gap-1.5">
                                <ShieldAlert size={12} />
                                <span>{t('profile.performHandover')}</span>
                            </span>
                            <HelpIcon topic="handover" size={14} className="text-amber-600 animate-pulse" />
                        </div>
                        {!password.trim() && (
                            <p className="text-xs font-medium text-amber-800 dark:text-amber-200 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-800 rounded-xl px-3.5 py-2.5">
                                {t('profile.handoverEnterPasswordHint', 'Bitte gib oben dein Profil-Passwort ein, um die Geräte-Übernahme zu bestätigen.')}
                            </p>
                        )}
                        <Button
                            type="button"
                            onClick={onHandover}
                            disabled={isLoggingIn || !password.trim()}
                            className="w-full py-5 rounded-3xl !bg-amber-600 hover:!bg-amber-700 shadow-premium-lg text-lg gap-3"
                        >
                            {isLoggingIn ? <RefreshCw className="animate-spin" /> : <ShieldAlert size={24} />}
                            {t('profile.performHandover')}
                        </Button>
                        <button
                            type="button"
                            onClick={onCancelHandover}
                            disabled={isLoggingIn}
                            className="w-full text-center text-[10px] font-black uppercase tracking-widest text-theme-light hover:text-theme-primary transition-colors cursor-pointer"
                        >
                            {t('profile.cancelProtocol')}
                        </button>
                    </div>
                ) : (
                    <Button
                        type="submit"
                        disabled={
                            isLoading ||
                            isLoggingIn ||
                            profiles.length === 0 ||
                            !selectedProfile ||
                            (isPinMode ? pin.length < 6 : !password)
                        }
                        className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3"
                    >
                        {isLoggingIn ? <RefreshCw className="animate-spin" size={24} /> : <LogIn size={24} />}
                        {isLoggingIn ? t('auth.authorizing') : t('auth.login')}
                    </Button>
                )}

                <div className="relative flex items-center justify-center my-6">
                    <div className="absolute inset-0 flex items-center">
                        <div className="w-full border-t border-theme-light-border" />
                    </div>
                    <span className="relative px-3 bg-white text-[10px] font-black uppercase tracking-widest text-theme-light">
                        {t('common.or') || 'OR'}
                    </span>
                </div>

                <LoginOnboardingCards
                    onSwitchToCreate={onSwitchToCreate}
                    onSwitchToRestoreBackup={onSwitchToRestoreBackup}
                />

                <button
                    type="button"
                    onClick={onSwitchToReset}
                    className="w-full text-center text-[10px] font-black uppercase tracking-[0.2em] text-theme-light hover:text-theme-primary transition-colors flex items-center justify-center gap-2 cursor-pointer mt-4"
                >
                    <Key size={12} /> {t('auth.forgotPassword')}
                </button>
            </div>

            {feedbackMsg && (
                <div
                    className={`p-5 rounded-[32px] border flex items-center gap-4 animate-in slide-in-from-bottom-4 ${
                        feedbackType === 'error'
                            ? 'bg-rose-50 border-rose-100 text-rose-800'
                            : 'bg-emerald-50 border-emerald-100 text-emerald-800'
                    }`}
                >
                    {feedbackType === 'error' ? (
                        <ShieldAlert size={20} className="text-rose-500 shrink-0" />
                    ) : (
                        <CheckCircle2 size={20} className="text-emerald-500 shrink-0" />
                    )}
                    <div>
                        <h4 className="text-[10px] font-black uppercase tracking-widest mb-1">
                            {feedbackType === 'error'
                                ? t('profile.verificationSystem')
                                : t('profile.protocolAlert')}
                        </h4>
                        <p className="text-sm font-bold leading-tight">{feedbackMsg}</p>
                    </div>
                </div>
            )}
        </form>
    );
};
