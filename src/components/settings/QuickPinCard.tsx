// src/components/settings/QuickPinCard.tsx
import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import {
    KeyRound,
    ShieldCheck,
    Lock,
    Store,
    CheckCircle2,
    Trash2,
} from 'lucide-react';
import { Card } from '../ui/Card';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { AlertBanner } from '../ui/AlertBanner';
import { authService, PinScope, ProfilePinStatusDto } from '../../services/authService';
import { useSession } from '../../context/SessionContext';
import { translateError } from '../../utils/errorHelper';

export const QuickPinCard: React.FC = () => {
    const { t } = useTranslation();
    const { folderName, accessMode = 'full', elevateSession } = useSession();

    const [status, setStatus] = useState<ProfilePinStatusDto | null>(null);
    const [isLoading, setIsLoading] = useState(true);

    // Enable / update PIN form
    const [isConfiguring, setIsConfiguring] = useState(false);
    const [scope, setScope] = useState<PinScope>('dailyPay');
    const [pin, setPin] = useState('');
    const [confirmPin, setConfirmPin] = useState('');
    const [masterPassword, setMasterPassword] = useState('');
    const [isSaving, setIsSaving] = useState(false);

    // Disable PIN confirmation
    const [isDisablingOpen, setIsDisablingOpen] = useState(false);
    const [disablePassword, setDisablePassword] = useState('');
    const [isDisabling, setIsDisabling] = useState(false);

    const [errorMsg, setErrorMsg] = useState('');
    const [successMsg, setSuccessMsg] = useState('');

    const refreshStatus = useCallback(async () => {
        if (!folderName) {
            setIsLoading(false);
            return;
        }
        try {
            const dto = await authService.getProfilePinStatus?.(folderName);
            if (dto) {
                setStatus(dto);
                if (dto.scope) {
                    setScope(dto.scope);
                }
            }
        } catch {
            // Silently ignore if backend command not yet available in mock
        } finally {
            setIsLoading(false);
        }
    }, [folderName]);

    useEffect(() => {
        refreshStatus();
    }, [refreshStatus]);

    const sanitizePinInput = (val: string) => val.replace(/\D/g, '').slice(0, 8);

    const handleEnablePin = async (e: React.FormEvent) => {
        e.preventDefault();
        setErrorMsg('');
        setSuccessMsg('');

        if (!/^\d{6,8}$/.test(pin)) {
            setErrorMsg(t('settings.pinValidationDigits'));
            return;
        }
        if (pin !== confirmPin) {
            setErrorMsg(t('settings.pinValidationMismatch'));
            return;
        }
        if (!masterPassword) {
            setErrorMsg(t('settings.pinMasterPasswordRequired'));
            return;
        }

        setIsSaving(true);
        try {
            const updated = await authService.enableProfilePin(
                masterPassword,
                pin,
                scope,
                folderName || undefined
            );
            setStatus(updated);
            if (accessMode !== 'full' && elevateSession) {
                await elevateSession(masterPassword).catch(() => undefined);
            }
            setIsConfiguring(false);
            setPin('');
            setConfirmPin('');
            setMasterPassword('');
            setSuccessMsg(t('settings.pinEnabledSuccess'));
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsSaving(false);
        }
    };

    const handleDisablePin = async (e: React.FormEvent) => {
        e.preventDefault();
        setErrorMsg('');
        setSuccessMsg('');

        if (!disablePassword) {
            setErrorMsg(t('settings.pinMasterPasswordRequired'));
            return;
        }

        setIsDisabling(true);
        try {
            await authService.disableProfilePin(disablePassword, folderName || undefined);
            setStatus({
                enabled: false,
                scope: null,
                failedAttempts: 0,
                maxAttempts: status?.maxAttempts ?? 5,
                remainingAttempts: status?.maxAttempts ?? 5,
            });
            setIsDisablingOpen(false);
            setDisablePassword('');
            setSuccessMsg(t('settings.pinDisabledSuccess'));
        } catch (err) {
            setErrorMsg(translateError(err, t));
        } finally {
            setIsDisabling(false);
        }
    };

    return (
        <Card
            header={
                <div className="flex items-center justify-between gap-2 w-full">
                    <div className="flex items-center gap-2">
                        <KeyRound size={18} className="text-theme-primary" />
                        <span className="font-black text-xs uppercase tracking-widest">
                            {t('settings.quickPinTitle')}
                        </span>
                    </div>
                    {!isLoading && (
                        <span
                            data-testid="quick-pin-status-badge"
                            className={`px-2.5 py-0.5 rounded-full text-[10px] font-bold uppercase tracking-wider border ${
                                status?.enabled
                                    ? 'bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-950/30 dark:text-emerald-300 dark:border-emerald-800/50'
                                    : 'bg-gray-100 text-gray-600 border-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:border-gray-700'
                            }`}
                        >
                            {status?.enabled
                                ? status.scope === 'receiveOnly'
                                    ? t('settings.pinBadgeReceiveOnly')
                                    : t('settings.pinBadgeDailyPay')
                                : t('settings.pinBadgeDisabled')}
                        </span>
                    )}
                </div>
            }
        >
            <div className="space-y-4">
                <p className="text-sm text-theme-secondary leading-relaxed">
                    {t('settings.quickPinDescription')}
                </p>

                {errorMsg && <AlertBanner variant="error">{errorMsg}</AlertBanner>}
                {successMsg && (
                    <AlertBanner variant="success">
                        <span className="flex items-center gap-2">
                            <CheckCircle2 size={16} />
                            {successMsg}
                        </span>
                    </AlertBanner>
                )}

                {!isConfiguring && !isDisablingOpen && (
                    <div className="flex flex-wrap items-center gap-3 pt-1">
                        <Button
                            type="button"
                            variant={status?.enabled ? 'secondary' : 'primary'}
                            size="sm"
                            onClick={() => {
                                setErrorMsg('');
                                setSuccessMsg('');
                                setIsConfiguring(true);
                            }}
                            data-testid="quick-pin-configure-btn"
                            className="gap-2"
                        >
                            <KeyRound size={15} />
                            <span>
                                {status?.enabled
                                    ? t('settings.pinChangeButton')
                                    : t('settings.pinSetupButton')}
                            </span>
                        </Button>

                        {status?.enabled && (
                            <Button
                                type="button"
                                variant="danger"
                                size="sm"
                                onClick={() => {
                                    setErrorMsg('');
                                    setSuccessMsg('');
                                    setIsDisablingOpen(true);
                                }}
                                data-testid="quick-pin-disable-open-btn"
                                className="gap-2"
                            >
                                <Trash2 size={15} />
                                <span>{t('settings.pinDisableButton')}</span>
                            </Button>
                        )}
                    </div>
                )}

                {isConfiguring && (
                    <form onSubmit={handleEnablePin} className="space-y-4 pt-2 border-t border-theme-subtle">
                        <div className="space-y-2">
                            <label className="text-xs font-bold text-theme-secondary uppercase tracking-wider block">
                                {t('settings.pinScopeLabel')}
                            </label>
                            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                                <button
                                    type="button"
                                    onClick={() => setScope('dailyPay')}
                                    data-testid="pin-scope-daily-pay"
                                    className={`text-left p-3.5 rounded-xl border transition-all cursor-pointer ${
                                        scope === 'dailyPay'
                                            ? 'border-theme-primary bg-theme-primary/5 ring-2 ring-theme-primary/20'
                                            : 'border-theme-subtle hover:border-theme-primary/40'
                                    }`}
                                >
                                    <div className="flex items-center gap-2 font-bold text-sm text-theme-secondary">
                                        <ShieldCheck size={16} className="text-theme-primary shrink-0" />
                                        <span>{t('settings.pinScopeDailyPayTitle')}</span>
                                    </div>
                                    <p className="text-xs text-theme-light mt-1">
                                        {t('settings.pinScopeDailyPayDesc')}
                                    </p>
                                </button>

                                <button
                                    type="button"
                                    onClick={() => setScope('receiveOnly')}
                                    data-testid="pin-scope-receive-only"
                                    className={`text-left p-3.5 rounded-xl border transition-all cursor-pointer ${
                                        scope === 'receiveOnly'
                                            ? 'border-amber-500 bg-amber-50/70 dark:bg-amber-950/30 ring-2 ring-amber-500/20'
                                            : 'border-theme-subtle hover:border-amber-400/50'
                                    }`}
                                >
                                    <div className="flex items-center gap-2 font-bold text-sm text-theme-secondary">
                                        <Store size={16} className="text-amber-600 shrink-0" />
                                        <span>{t('settings.pinScopeReceiveOnlyTitle')}</span>
                                    </div>
                                    <p className="text-xs text-theme-light mt-1">
                                        {t('settings.pinScopeReceiveOnlyDesc')}
                                    </p>
                                </button>
                            </div>
                        </div>

                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                            <div>
                                <label htmlFor="quick-pin-input" className="text-xs font-bold text-theme-secondary block mb-1">
                                    {t('settings.pinInputLabel')}
                                </label>
                                <Input
                                    id="quick-pin-input"
                                    type="password"
                                    inputMode="numeric"
                                    autoComplete="off"
                                    maxLength={8}
                                    value={pin}
                                    onChange={(e) => setPin(sanitizePinInput(e.target.value))}
                                    placeholder={t('settings.pinInputPlaceholder')}
                                    data-testid="quick-pin-input"
                                />
                            </div>
                            <div>
                                <label htmlFor="quick-pin-confirm-input" className="text-xs font-bold text-theme-secondary block mb-1">
                                    {t('settings.pinConfirmLabel')}
                                </label>
                                <Input
                                    id="quick-pin-confirm-input"
                                    type="password"
                                    inputMode="numeric"
                                    autoComplete="off"
                                    maxLength={8}
                                    value={confirmPin}
                                    onChange={(e) => setConfirmPin(sanitizePinInput(e.target.value))}
                                    placeholder={t('settings.pinConfirmPlaceholder')}
                                    data-testid="quick-pin-confirm-input"
                                />
                            </div>
                        </div>

                        <div>
                            <label htmlFor="quick-pin-master-pw" className="text-xs font-bold text-theme-secondary flex items-center gap-1.5 mb-1">
                                <Lock size={12} />
                                {t('settings.pinMasterPasswordLabel')}
                            </label>
                            <Input
                                id="quick-pin-master-pw"
                                type="password"
                                value={masterPassword}
                                onChange={(e) => setMasterPassword(e.target.value)}
                                placeholder={t('auth.enterAccessPassword')}
                                data-testid="quick-pin-master-password-input"
                            />
                        </div>

                        <div className="flex flex-wrap justify-end gap-2 pt-1">
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={() => {
                                    setIsConfiguring(false);
                                    setPin('');
                                    setConfirmPin('');
                                    setMasterPassword('');
                                    setErrorMsg('');
                                }}
                                disabled={isSaving}
                            >
                                {t('common.cancel')}
                            </Button>
                            <Button
                                type="submit"
                                variant="primary"
                                size="sm"
                                disabled={isSaving || pin.length < 6 || confirmPin.length < 6 || !masterPassword}
                                data-testid="quick-pin-save-btn"
                            >
                                {isSaving ? t('common.saving') : t('settings.pinSaveButton')}
                            </Button>
                        </div>
                    </form>
                )}

                {isDisablingOpen && (
                    <form onSubmit={handleDisablePin} className="space-y-3 pt-2 border-t border-theme-subtle">
                        <p className="text-xs text-theme-light">
                            {t('settings.pinDisableDesc')}
                        </p>
                        <div>
                            <label htmlFor="quick-pin-disable-pw" className="text-xs font-bold text-theme-secondary block mb-1">
                                {t('settings.pinMasterPasswordLabel')}
                            </label>
                            <Input
                                id="quick-pin-disable-pw"
                                type="password"
                                value={disablePassword}
                                onChange={(e) => setDisablePassword(e.target.value)}
                                placeholder={t('auth.enterAccessPassword')}
                                data-testid="quick-pin-disable-password-input"
                            />
                        </div>
                        <div className="flex flex-wrap justify-end gap-2">
                            <Button
                                type="button"
                                variant="secondary"
                                size="sm"
                                onClick={() => {
                                    setIsDisablingOpen(false);
                                    setDisablePassword('');
                                    setErrorMsg('');
                                }}
                                disabled={isDisabling}
                            >
                                {t('common.cancel')}
                            </Button>
                            <Button
                                type="submit"
                                variant="danger"
                                size="sm"
                                disabled={isDisabling || !disablePassword}
                                data-testid="quick-pin-disable-confirm-btn"
                            >
                                {t('settings.pinDisableButton')}
                            </Button>
                        </div>
                    </form>
                )}
            </div>
        </Card>
    );
};
