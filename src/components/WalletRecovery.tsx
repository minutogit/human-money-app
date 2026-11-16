// src/components/WalletRecovery.tsx
import { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import logo from '../assets/logo.png';
import { profileService } from '../services/profileService';
import { authService } from '../services/authService';
import { recoveryService } from '../services/recoveryService';
import { AuthLayout } from './AuthLayout';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { Card } from './ui/Card';
import { AlertBanner } from './ui/AlertBanner';
import { SeedPhraseInput, SeedPhraseValue } from './ui/SeedPhraseInput';
import { SocialRecoveryFlow } from './SocialRecoveryFlow';
import { ActiveRecoverySessionDto, ProfileInfo } from '../types';
import { translateError } from '../utils/errorHelper';
import {
    Users,
    KeyRound,
    Lock,
    Fingerprint,
    ArrowLeft,
    ArrowRight,
    RefreshCw,
    ShieldAlert,
    User,
    Info,
} from 'lucide-react';

interface WalletRecoveryProps {
    onRecoverySuccess: (folderName?: string) => void;
    onSwitchToLogin: () => void;
    initialPayload?: string;
    defaultMethod?: 'choose' | 'social' | 'words';
}

const EMPTY_SEED: SeedPhraseValue = {
    mnemonic: '',
    wordCount: 12,
    language: 'english',
    passphrase: '',
    userPrefix: '',
    isValid: false,
    detectedKit: null,
};

export function WalletRecovery({
    onRecoverySuccess,
    onSwitchToLogin,
    initialPayload,
    defaultMethod,
}: WalletRecoveryProps) {
    const { t } = useTranslation();

    const [method, setMethod] = useState<'choose' | 'social' | 'words'>(() => {
        if (defaultMethod) return defaultMethod;
        if (initialPayload?.includes('humoco://recovery-reply')) return 'social';
        if (initialPayload?.includes('humoco://kit')) return 'words';
        return 'choose';
    });

    const [profiles, setProfiles] = useState<ProfileInfo[]>([]);
    const [selectedProfile, setSelectedProfile] = useState<string>('');
    const [activeSocialSession, setActiveSocialSession] =
        useState<ActiveRecoverySessionDto | null>(null);
    const [seedState, setSeedState] = useState<SeedPhraseValue>(EMPTY_SEED);

    const [newPassword, setNewPassword] = useState('');
    const [confirmPassword, setConfirmPassword] = useState('');
    const [feedbackMsg, setFeedbackMsg] = useState('');
    const [feedbackType, setFeedbackType] = useState<'error' | 'success' | 'info' | null>(null);
    const [isLoading, setIsLoading] = useState(false);

    useEffect(() => {
        if (initialPayload?.includes('humoco://recovery-reply')) {
            setMethod('social');
        } else if (initialPayload?.includes('humoco://kit')) {
            setMethod('words');
        }
    }, [initialPayload]);

    useEffect(() => {
        if (method !== 'choose' && method !== 'social') return;
        let active = true;
        recoveryService
            .getActiveRecoverySession?.()
            ?.then((session) => {
                if (!active) return;
                if (session && session.sessionMode === 'reset_local_password') {
                    setActiveSocialSession(session);
                    if (session.targetFolderName) {
                        setSelectedProfile(session.targetFolderName);
                    }
                } else {
                    setActiveSocialSession(null);
                }
            })
            ?.catch(() => {});
        return () => {
            active = false;
        };
    }, [method]);

    useEffect(() => {
        let active = true;
        logger.info('WalletRecovery component displayed');
        setIsLoading(true);
        authService
            .listProfiles()
            .then((available) => {
                if (!active) return;
                setProfiles(available);
                if (available.length === 1) {
                    setSelectedProfile((prev) => prev || available[0].folderName);
                } else if (available.length === 0) {
                    setFeedbackMsg(t('auth.noProfilesToRecover'));
                    setFeedbackType('error');
                }
            })
            .catch((e) => {
                if (!active) return;
                setFeedbackMsg(`${t('profile.errorPrefix')}: ${translateError(e, t)}`);
                setFeedbackType('error');
            })
            .finally(() => {
                if (active) setIsLoading(false);
            });
        return () => {
            active = false;
        };
    }, [t]);

    const handleSeedChange = useCallback((next: SeedPhraseValue) => {
        setSeedState(next);
    }, []);

    async function handleWordsRecovery(e: React.FormEvent) {
        e.preventDefault();
        if (!selectedProfile) {
            setFeedbackMsg(t('auth.selectProfileToRecover'));
            setFeedbackType('error');
            return;
        }
        if (!seedState.isValid) {
            setFeedbackMsg(t('auth.seedPhraseInvalid'));
            setFeedbackType('error');
            return;
        }
        if (newPassword !== confirmPassword) {
            setFeedbackMsg(t('auth.passwordsDontMatch'));
            setFeedbackType('error');
            return;
        }
        if (newPassword.length < 8) {
            setFeedbackMsg(t('auth.passwordMinLength'));
            setFeedbackType('error');
            return;
        }

        setIsLoading(true);
        setFeedbackMsg(t('auth.recoveringWallet'));
        setFeedbackType('info');
        try {
            const localInstanceId = await authService.getLocalInstanceId();
            await profileService.recoverWallet({
                folderName: selectedProfile,
                mnemonic: seedState.mnemonic,
                passphrase: seedState.passphrase || undefined,
                newPassword,
                localInstanceId,
                language: seedState.language,
            });
            onRecoverySuccess(selectedProfile);
        } catch (err) {
            setFeedbackMsg(`${t('auth.errorRecoveringWallet')}: ${translateError(err, t)}`);
            setFeedbackType('error');
        } finally {
            setIsLoading(false);
        }
    }

    return (
        <AuthLayout maxWidth="max-w-3xl">
            <div className="flex items-center justify-center gap-3 sm:gap-6">
                <img
                    src={logo}
                    alt="Human Money Logo"
                    className="w-12 h-12 sm:w-20 sm:h-20 object-contain drop-shadow-sm"
                />
                <div className="text-left space-y-0 sm:space-y-0.5">
                    <h1 className="text-2xl sm:text-4xl font-black text-theme-primary tracking-tighter leading-none">
                        HUMAN MONEY
                    </h1>
                    <p className="text-[9px] sm:text-[10px] font-black uppercase tracking-[0.4em] text-theme-light">
                        {t('auth.recoverWallet')}
                    </p>
                </div>
            </div>

            {/* Profile Selector Card */}
            <Card
                header={
                    <div className="flex items-center gap-2">
                        <User size={14} />
                        <label
                            htmlFor="profile-select"
                            className="font-black text-[10px] uppercase tracking-widest cursor-pointer"
                        >
                            {t('profile.selectProfile')}
                        </label>
                    </div>
                }
            >
                {profiles.length > 0 ? (
                    <select
                        id="profile-select"
                        value={selectedProfile}
                        onChange={(e) => {
                            setSelectedProfile(e.target.value);
                            if (feedbackType === 'error') {
                                setFeedbackMsg('');
                                setFeedbackType(null);
                            }
                        }}
                        disabled={Boolean(activeSocialSession?.targetFolderName && method === 'social')}
                        data-testid="profile-select"
                        className="w-full bg-white border border-theme-subtle rounded-2xl px-4 py-3.5 text-sm font-bold text-theme-secondary focus:ring-2 focus:ring-theme-primary/10 outline-none shadow-inner-soft appearance-none transition-all disabled:opacity-70"
                    >
                        {profiles.length > 1 && (
                            <option value="">
                                {t('auth.selectProfileToRecover')}
                            </option>
                        )}
                        {profiles.map((profile) => (
                            <option key={profile.folderName} value={profile.folderName}>
                                {profile.profileName}
                            </option>
                        ))}
                    </select>
                ) : (
                    <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3">
                        <ShieldAlert size={20} className="text-rose-500" />
                        <p className="text-xs font-bold text-rose-800">
                            {t('auth.noProfilesToRecover')}
                        </p>
                    </div>
                )}
            </Card>

            {/* Top Choice: 2 Equal Cards */}
            {method === 'choose' && (
                <div className="space-y-5 animate-in fade-in duration-300">
                    {feedbackMsg && (
                        <AlertBanner
                            data-testid="feedback-message"
                            variant={
                                feedbackType === 'error'
                                    ? 'error'
                                    : feedbackType === 'success'
                                      ? 'success'
                                      : 'info'
                            }
                        >
                            {feedbackMsg}
                        </AlertBanner>
                    )}

                    {activeSocialSession && (
                        <AlertBanner
                            variant="info"
                            data-testid="wallet-recovery-resume-banner"
                            action={
                                <Button
                                    type="button"
                                    variant="primary"
                                    size="xs"
                                    data-testid="wallet-recovery-resume-btn"
                                    onClick={() => setMethod('social')}
                                >
                                    {t('backup.resumeSessionBtn')}
                                </Button>
                            }
                        >
                            {t('backup.resumeSessionDesc', {
                                code: activeSocialSession.sasCode,
                                collected: activeSocialSession.collectedCount,
                                required: activeSocialSession.thresholdM ?? '?',
                            })}
                        </AlertBanner>
                    )}

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                        <button
                            type="button"
                            onClick={() => {
                                if (!selectedProfile) {
                                    setFeedbackMsg(t('auth.selectProfileToRecover'));
                                    setFeedbackType('error');
                                    return;
                                }
                                setMethod('social');
                            }}
                            data-testid="wallet-recovery-choose-social"
                            className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between cursor-pointer"
                        >
                            <div className="space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-theme-primary/10 text-theme-primary flex items-center justify-center group-hover:scale-105 transition-transform">
                                    <Users size={24} />
                                </div>
                                <h3 className="font-black text-base text-theme-secondary tracking-tight">
                                    {t('backup.restorePrimaryContacts')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('backup.resetPasswordContactsDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1.5 text-xs font-bold text-theme-primary pt-4">
                                <span>{t('common.select')}</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>

                        <button
                            type="button"
                            onClick={() => {
                                if (!selectedProfile) {
                                    setFeedbackMsg(t('auth.selectProfileToRecover'));
                                    setFeedbackType('error');
                                    return;
                                }
                                setMethod('words');
                            }}
                            data-testid="wallet-recovery-choose-words"
                            className="p-6 bg-bg-surface hover:bg-theme-subtle/30 border-2 border-theme-subtle hover:border-theme-primary rounded-3xl text-left transition-all group shadow-sm flex flex-col justify-between cursor-pointer"
                        >
                            <div className="space-y-3">
                                <div className="w-12 h-12 rounded-2xl bg-amber-500/10 text-amber-600 flex items-center justify-center group-hover:scale-105 transition-transform">
                                    <KeyRound size={24} />
                                </div>
                                <h3 className="font-black text-base text-theme-secondary tracking-tight">
                                    {t('backup.restoreCardWordsTitle')}
                                </h3>
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {t('auth.resetPasswordWithSeedDesc')}
                                </p>
                            </div>
                            <div className="flex items-center gap-1.5 text-xs font-bold text-amber-600 pt-4">
                                <span>{t('common.select')}</span>
                                <ArrowRight size={14} />
                            </div>
                        </button>
                    </div>

                    <div className="flex justify-center pt-1">
                        <Button type="button" variant="secondary" onClick={onSwitchToLogin} className="gap-2">
                            <ArrowLeft size={16} />
                            {t('auth.backToLogin')}
                        </Button>
                    </div>
                </div>
            )}

            {/* Branch A: Social Recovery */}
            {method === 'social' && (
                <div className="space-y-4">
                    <SocialRecoveryFlow
                        mode="reset_local_password"
                        targetFolderName={selectedProfile}
                        existingSession={
                            activeSocialSession &&
                            (!activeSocialSession.targetFolderName ||
                                activeSocialSession.targetFolderName === selectedProfile)
                                ? activeSocialSession
                                : null
                        }
                        initialPayload={initialPayload}
                        onBack={() => setMethod('choose')}
                        onSuccess={() => onRecoverySuccess(selectedProfile)}
                    />
                    <div className="flex justify-center pt-1">
                        <Button type="button" variant="outline" size="sm" onClick={onSwitchToLogin}>
                            {t('auth.backToLogin')}
                        </Button>
                    </div>
                </div>
            )}

            {/* Branch B: Recovery Words / Emergency Sheet */}
            {method === 'words' && (
                <form onSubmit={handleWordsRecovery} className="space-y-6 animate-in fade-in duration-300">
                    <div className="p-4 bg-slate-50 border border-slate-200 rounded-2xl text-xs text-theme-light flex items-start justify-between gap-3">
                        <div className="flex items-start gap-2.5">
                            <Info size={16} className="text-theme-primary shrink-0 mt-0.5" />
                            <span>{t('auth.resetPasswordWithSeedDesc')}</span>
                        </div>
                        <button
                            type="button"
                            onClick={() => setMethod('choose')}
                            className="text-xs font-bold text-theme-primary hover:underline shrink-0 cursor-pointer"
                        >
                            {t('common.back')}
                        </button>
                    </div>

                    <SeedPhraseInput
                        allowKitUri
                        showUserPrefix={false}
                        initialPayload={initialPayload}
                        onChange={handleSeedChange}
                        idPrefix="recovery-word"
                    />

                    <Card
                        header={
                            <div className="flex items-center gap-2">
                                <Lock size={14} />
                                <span className="font-black text-[10px] uppercase tracking-widest">
                                    {t('auth.reEncryptionCredentials')}
                                </span>
                            </div>
                        }
                    >
                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">
                                    {t('auth.newWalletPassword')}
                                </label>
                                <Input
                                    type="password"
                                    showToggle
                                    data-testid="recovery-new-password-input"
                                    value={newPassword}
                                    onChange={(e) => {
                                        setNewPassword(e.target.value);
                                        if (feedbackType === 'error') {
                                            setFeedbackMsg('');
                                            setFeedbackType(null);
                                        }
                                    }}
                                    placeholder={t('auth.passwordMinChars')}
                                    autoComplete="new-password"
                                />
                            </div>
                            <div className="space-y-2">
                                <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">
                                    {t('auth.confirmPassword')}
                                </label>
                                <Input
                                    type="password"
                                    showToggle
                                    data-testid="recovery-confirm-password-input"
                                    value={confirmPassword}
                                    onChange={(e) => setConfirmPassword(e.target.value)}
                                    placeholder={t('auth.repeatPassword')}
                                    autoComplete="new-password"
                                />
                            </div>
                        </div>
                    </Card>

                    {feedbackMsg && (
                        <AlertBanner
                            data-testid="feedback-message"
                            variant={
                                feedbackType === 'error'
                                    ? 'error'
                                    : feedbackType === 'success'
                                      ? 'success'
                                      : 'info'
                            }
                        >
                            {feedbackMsg}
                        </AlertBanner>
                    )}

                    <div className="flex flex-col items-center gap-3 pt-2">
                        <Button
                            type="submit"
                            data-testid="recovery-submit-btn"
                            disabled={isLoading || !seedState.isValid || profiles.length === 0}
                            className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3"
                        >
                            {isLoading ? (
                                <RefreshCw className="animate-spin" size={24} />
                            ) : (
                                <Fingerprint size={24} />
                            )}
                            {isLoading ? t('auth.loadingWallet') : t('auth.recoverWalletAndLogin')}
                        </Button>
                        <Button
                            type="button"
                            variant="secondary"
                            onClick={onSwitchToLogin}
                            className="w-full py-4 rounded-2xl shadow-sm gap-2"
                        >
                            <ArrowLeft size={18} /> {t('auth.backToLogin')}
                        </Button>
                    </div>
                </form>
            )}
        </AuthLayout>
    );
}
