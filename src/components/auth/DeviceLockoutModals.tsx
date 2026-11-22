import React, { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
    AlertTriangle,
    ArrowRight,
    ChevronDown,
    ChevronUp,
    HelpCircle,
    Info,
    Key,
    Lock,
    LogOut,
    RefreshCw,
    ShieldAlert,
    ShieldCheck,
    Smartphone,
    Trash2,
} from 'lucide-react';
import { Advanced } from '../ui/Advanced';
import { Button } from '../ui/Button';
import { Card } from '../ui/Card';
import { HelpModal } from '../ui/HelpModal';
import { Input } from '../ui/Input';

export type DeviceLockoutKind = 'handover' | 'clone' | 'fork';

export interface DeviceLockoutModalProps {
    kind: DeviceLockoutKind;
    isOpen: boolean;
    profileName?: string;
    targetEpoch?: number;
    reclaimPassword?: string;
    onReclaimPasswordChange?: (value: string) => void;
    onReclaim?: () => void | Promise<void>;
    isReclaiming?: boolean;
    reclaimError?: string;
    onClearReclaimError?: () => void;
    onSwitchOrLogout: () => void;
    onStartRecovery?: () => void;
    onRequestDeleteLocal?: () => void;
    secondaryCancelLabel?: string;
    onSecondaryCancel?: () => void;
    profileSelector?: React.ReactNode;
    zIndexClass?: string;
}

/**
 * Shared security lockout overlay used by both Login and ForkLockOverlay for
 * handover deactivation, concurrent clone conflicts, and history fork lockouts.
 */
export const DeviceLockoutModal: React.FC<DeviceLockoutModalProps> = ({
    kind,
    isOpen,
    profileName,
    targetEpoch,
    reclaimPassword = '',
    onReclaimPasswordChange,
    onReclaim,
    isReclaiming = false,
    reclaimError = '',
    onClearReclaimError,
    onSwitchOrLogout,
    onStartRecovery,
    onRequestDeleteLocal,
    secondaryCancelLabel,
    onSecondaryCancel,
    profileSelector,
    zIndexClass = 'z-[100]',
}) => {
    const { t } = useTranslation();
    const [showMultiDeviceHelp, setShowMultiDeviceHelp] = useState(false);
    const [showReclaim, setShowReclaim] = useState(false);

    useEffect(() => {
        if (!isOpen) {
            setShowReclaim(false);
            setShowMultiDeviceHelp(false);
        }
    }, [isOpen]);

    if (!isOpen) {
        return null;
    }

    const helpButton = (
        <div className="pt-1 text-center">
            <button
                type="button"
                data-testid="forklock-help-button"
                onClick={() => setShowMultiDeviceHelp(true)}
                className="inline-flex items-center gap-1.5 text-xs font-semibold text-theme-primary hover:underline cursor-pointer"
            >
                <HelpCircle size={14} />
                {t('auth.multiDeviceHelpLink')}
            </button>
        </div>
    );

    const helpModal = (
        <HelpModal
            isOpen={showMultiDeviceHelp}
            onClose={() => setShowMultiDeviceHelp(false)}
            topic="multiDevice"
        />
    );

    if (kind === 'handover') {
        const handleToggleReclaim = () => {
            if (showReclaim) {
                onReclaimPasswordChange?.('');
                onClearReclaimError?.();
            }
            setShowReclaim((prev) => !prev);
        };

        return (
            <div
                className={`fixed inset-0 ${zIndexClass} flex items-center justify-center bg-black/70 backdrop-blur-sm p-4 animate-in fade-in duration-200`}
            >
                <div className="max-w-md w-full bg-white dark:bg-gray-800 rounded-2xl shadow-2xl overflow-hidden border border-amber-500/30">
                    <div className="bg-amber-500 p-6 flex justify-center items-center text-white">
                        <Smartphone className="h-14 w-14" />
                    </div>

                    <div className="p-6 sm:p-8 space-y-6">
                        <div className="text-center space-y-2">
                            <h2 className="text-xl sm:text-2xl font-bold text-gray-900 dark:text-white">
                                {t('auth.handoverTitle')}
                            </h2>
                            {profileName && (
                                <p className="text-xs font-semibold uppercase tracking-wider text-amber-600">
                                    {profileName}
                                </p>
                            )}
                            <p className="text-sm text-gray-600 dark:text-gray-300 leading-relaxed">
                                {t('auth.handoverSimpleDesc')}
                            </p>
                            <Advanced>
                                <p
                                    data-testid="forklock-epoch-detail"
                                    className="text-xs font-mono text-gray-500 dark:text-gray-400"
                                >
                                    {t('auth.handoverEpochDetail', {
                                        targetEpoch: targetEpoch !== undefined ? targetEpoch : '?',
                                    })}
                                </p>
                            </Advanced>
                            {helpButton}
                        </div>
                        {helpModal}

                        {profileSelector}

                        <div className="space-y-3">
                            <Button
                                type="button"
                                variant="primary"
                                size="lg"
                                className="w-full bg-amber-600 hover:bg-amber-700 text-white flex items-center justify-center gap-2 py-3"
                                onClick={onSwitchOrLogout}
                            >
                                <LogOut size={18} />
                                {t('auth.handoverLogoutBtn', 'Switch Profile / Log Out')}
                            </Button>

                            {onRequestDeleteLocal && (
                                <Button
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="w-full text-rose-600 dark:text-rose-400 border border-rose-200 dark:border-rose-900/40 hover:bg-rose-50 dark:hover:bg-rose-950/20 flex items-center justify-center gap-2"
                                    onClick={onRequestDeleteLocal}
                                >
                                    <Trash2 size={16} />
                                    {t('auth.handoverDeleteBtn', 'Remove Local Profile from This Device')}
                                </Button>
                            )}
                        </div>

                        {onReclaim && (
                            <div className="pt-2 border-t border-gray-100 dark:border-gray-700">
                                <button
                                    type="button"
                                    onClick={handleToggleReclaim}
                                    className="w-full flex items-center justify-between text-xs font-semibold text-gray-500 dark:text-gray-400 hover:text-amber-600 transition-colors py-2 cursor-pointer"
                                >
                                    <span className="flex items-center gap-1.5">
                                        <Key size={14} />
                                        {t('auth.handoverReclaimTitle', 'Reactivate Wallet on This Device')}
                                    </span>
                                    {showReclaim ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                                </button>

                                {showReclaim && (
                                    <form
                                        onSubmit={(e) => {
                                            e.preventDefault();
                                            void onReclaim();
                                        }}
                                        className="mt-3 space-y-3 text-left animate-in slide-in-from-top-2 duration-150"
                                    >
                                        <p className="text-xs text-gray-500 dark:text-gray-400">
                                            {t(
                                                'auth.handoverReclaimDesc',
                                                'To reactivate this device, enter your password. This transfers the active lock to this device and deactivates the other device.'
                                            )}
                                        </p>
                                        <Input
                                            type="password"
                                            placeholder={t('auth.enterAccessPassword', 'Password')}
                                            value={reclaimPassword}
                                            onChange={(e) => {
                                                onReclaimPasswordChange?.(e.target.value);
                                                if (reclaimError) onClearReclaimError?.();
                                            }}
                                            className="text-sm py-2"
                                            autoFocus
                                        />
                                        {reclaimError && (
                                            <div className="p-2.5 bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-900/40 rounded-lg text-xs text-rose-600 dark:text-rose-400 flex items-center gap-2">
                                                <AlertTriangle size={14} className="shrink-0" />
                                                <span>{reclaimError}</span>
                                            </div>
                                        )}
                                        <Button
                                            type="submit"
                                            disabled={isReclaiming || !reclaimPassword}
                                            className="w-full bg-amber-600 hover:bg-amber-700 text-white text-xs py-2.5 flex items-center justify-center gap-2"
                                        >
                                            {isReclaiming ? (
                                                <RefreshCw size={14} className="animate-spin" />
                                            ) : (
                                                <Lock size={14} />
                                            )}
                                            {t('auth.handoverReclaimBtn', 'Reclaim Wallet on This Device')}
                                        </Button>
                                    </form>
                                )}
                            </div>
                        )}
                    </div>
                </div>
            </div>
        );
    }

    const isClone = kind === 'clone';
    const secondaryAction = onSecondaryCancel ?? onSwitchOrLogout;
    const secondaryLabel = secondaryCancelLabel ?? t('auth.handoverLogoutBtn');

    return (
        <div
            className={`fixed inset-0 ${zIndexClass} flex items-center justify-center bg-black/80 backdrop-blur-sm p-4 animate-in fade-in duration-200`}
        >
            <div
                className={`max-w-md w-full bg-white dark:bg-gray-800 rounded-2xl shadow-2xl overflow-hidden border ${
                    isClone ? 'border-rose-500/40' : 'border-amber-500/30'
                }`}
            >
                <div className={`${isClone ? 'bg-rose-600' : 'bg-amber-500'} p-6 flex justify-center items-center text-white`}>
                    {isClone ? (
                        <ShieldAlert className="h-16 w-16 animate-pulse" />
                    ) : (
                        <AlertTriangle className="h-16 w-16" />
                    )}
                </div>

                <div className="p-8 text-center space-y-5">
                    <div className="space-y-2">
                        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">
                            {isClone ? t('auth.concurrentCloneTitle') : t('auth.forkConflictTitle')}
                        </h2>
                        {profileName && (
                            <p className="text-xs font-semibold uppercase tracking-wider text-rose-600">
                                {profileName}
                            </p>
                        )}
                    </div>

                    <p className="text-gray-600 dark:text-gray-300 leading-relaxed text-sm">
                        {isClone ? t('auth.concurrentCloneDesc') : t('auth.forkDetected')}
                    </p>

                    {helpButton}
                    {helpModal}

                    {!isClone && (
                        <div className="bg-amber-50 dark:bg-amber-900/20 rounded-lg p-4 text-left border border-amber-100 dark:border-amber-900/30">
                            <p className="text-sm text-amber-800 dark:text-amber-300 font-medium flex items-center gap-2">
                                <Info className="h-4 w-4 shrink-0" />
                                {t('auth.forkNextStepTitle')}
                            </p>
                            <p className="text-sm text-amber-700 dark:text-amber-400 mt-1">
                                {t('auth.forkRecoveryRequired')}
                            </p>
                        </div>
                    )}

                    <div className="space-y-3 pt-1">
                        {onStartRecovery && (
                            <Button
                                type="button"
                                variant="primary"
                                size="lg"
                                className="w-full bg-amber-600 hover:bg-amber-700 text-white border-none py-4 text-base"
                                onClick={onStartRecovery}
                            >
                                {t('auth.recoverWalletSeed')}
                            </Button>
                        )}

                        <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            className="w-full"
                            onClick={secondaryAction}
                        >
                            {secondaryLabel}
                        </Button>
                    </div>

                    {!isClone && (
                        <p className="text-xs text-theme-subtle">
                            {t('auth.functionsSuspended')}
                        </p>
                    )}
                </div>
            </div>
        </div>
    );
};

export interface PostHandoverSuccessModalProps {
    isOpen: boolean;
    userId: string;
    onAcknowledge: () => void;
}

/**
 * Confirmation modal shown in Login after successfully transferring active device
 * lock ownership onto this device.
 */
export const PostHandoverSuccessModal: React.FC<PostHandoverSuccessModalProps> = ({
    isOpen,
    userId,
    onAcknowledge,
}) => {
    const { t } = useTranslation();

    if (!isOpen) {
        return null;
    }

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-white/40 backdrop-blur-md p-4 animate-in fade-in duration-300">
            <Card className="w-full max-w-xl border-theme-primary shadow-2xl p-10 space-y-8 animate-in zoom-in duration-300">
                <div className="text-center space-y-4">
                    <div className="mx-auto w-20 h-20 bg-theme-primary text-white rounded-[32px] flex items-center justify-center shadow-premium-lg">
                        <ShieldCheck size={40} />
                    </div>
                    <div className="space-y-1">
                        <h2 className="text-3xl font-black text-theme-primary tracking-tighter uppercase">
                            {t('profile.handoverComplete')}
                        </h2>
                        <p className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light">
                            {t('profile.handoverComplete')}
                        </p>
                    </div>
                </div>

                <div className="p-6 bg-slate-50 border border-slate-200 rounded-[32px] space-y-3">
                    <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">
                        {t('profile.boundIdentity')}
                    </p>
                    <p className="text-xs font-mono break-all text-theme-primary bg-white p-3 rounded-2xl border border-slate-100">
                        {userId}
                    </p>
                </div>

                <div className="p-6 bg-amber-50 border border-amber-100 rounded-[32px] space-y-4">
                    <div className="flex items-center gap-2 text-amber-600">
                        <AlertTriangle size={18} />
                        <h3 className="text-xs font-black uppercase tracking-widest">
                            {t('profile.doubleSpendPrevention')}
                        </h3>
                    </div>
                    <p className="text-sm font-medium text-amber-900 leading-relaxed">
                        {t('profile.doubleSpendWarning')}
                    </p>
                </div>

                <Button
                    type="button"
                    onClick={onAcknowledge}
                    className="w-full py-5 rounded-3xl shadow-premium-lg text-lg gap-3"
                >
                    {t('profile.acknowledgeAccess')} <ArrowRight size={20} />
                </Button>
            </Card>
        </div>
    );
};
