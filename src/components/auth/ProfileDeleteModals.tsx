import React from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Lock, ShieldAlert, Trash2, UserX } from 'lucide-react';
import { Button } from '../ui/Button';
import { HelpIcon } from '../ui/HelpIcon';
import { Input } from '../ui/Input';
import { ModalShell } from '../ui/ModalShell';

export interface QuickDeleteProfileModalProps {
    isOpen: boolean;
    password: string;
    error: string;
    isDeleting: boolean;
    onPasswordChange: (value: string) => void;
    onClearError: () => void;
    onCancel: () => void;
    onConfirm: () => void | Promise<void>;
}

/**
 * Single-step local profile removal dialog used from ForkLockOverlay handover state.
 */
export const QuickDeleteProfileModal: React.FC<QuickDeleteProfileModalProps> = ({
    isOpen,
    password,
    error,
    isDeleting,
    onPasswordChange,
    onClearError,
    onCancel,
    onConfirm,
}) => {
    const { t } = useTranslation();

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={isDeleting ? undefined : onCancel}
            closeOnBackdrop={!isDeleting}
            closeOnEscape={!isDeleting}
            hideCloseButton
            maxWidth="sm"
            zIndexClass="z-[110]"
            title={
                <h3 className="text-lg font-bold text-gray-900 dark:text-white flex items-center gap-2">
                    <Trash2 size={18} className="text-rose-600" />
                    {t('auth.handoverDeleteBtn', 'Remove Local Profile')}
                </h3>
            }
            description={t(
                'profile.restoreWarning',
                'Deleting this profile removes local voucher data from this device.'
            )}
        >
            <form
                onSubmit={(e) => {
                    e.preventDefault();
                    void onConfirm();
                }}
                className="space-y-3"
            >
                <Input
                    type="password"
                    placeholder={t('auth.enterAccessPassword', 'Password')}
                    value={password}
                    onChange={(e) => {
                        onPasswordChange(e.target.value);
                        if (error) onClearError();
                    }}
                    autoFocus
                />
                {error && (
                    <p className="text-xs text-rose-600 dark:text-rose-400 font-medium">
                        {error}
                    </p>
                )}
                <div className="flex gap-2 pt-2">
                    <Button
                        type="button"
                        variant="secondary"
                        className="flex-1"
                        onClick={onCancel}
                        disabled={isDeleting}
                    >
                        {t('common.cancel', 'Cancel')}
                    </Button>
                    <Button
                        type="submit"
                        className="flex-1 bg-rose-600 hover:bg-rose-700 text-white"
                        disabled={isDeleting || !password}
                    >
                        {isDeleting
                            ? t('profile.deleting', 'Deleting...')
                            : t('profile.deleteProfile', 'Delete Profile')}
                    </Button>
                </div>
            </form>
        </ModalShell>
    );
};

export interface LoginDeleteModalsProps {
    activeProfileName: string;
    showPasswordPrompt: boolean;
    showConfirmModal: boolean;
    deletePassword: string;
    deleteUserId: string;
    deleteError: string;
    isVerifying: boolean;
    isDeleting: boolean;
    onPasswordChange: (value: string) => void;
    onClearError: () => void;
    onVerifyPassword: () => void | Promise<void>;
    onConfirmDelete: () => void | Promise<void>;
    onCancelPrompt: () => void;
    onCancelConfirm: () => void;
}

/**
 * Two-step verified profile deletion modals used on the Login screen:
 * 1) Password verification to reveal the bound identity,
 * 2) Destructive confirmation dialog.
 */
export const LoginDeleteModals: React.FC<LoginDeleteModalsProps> = ({
    activeProfileName,
    showPasswordPrompt,
    showConfirmModal,
    deletePassword,
    deleteUserId,
    deleteError,
    isVerifying,
    isDeleting,
    onPasswordChange,
    onClearError,
    onVerifyPassword,
    onConfirmDelete,
    onCancelPrompt,
    onCancelConfirm,
}) => {
    const { t } = useTranslation();

    return (
        <>
            <ModalShell
                isOpen={showPasswordPrompt}
                onClose={isVerifying ? undefined : onCancelPrompt}
                closeOnBackdrop={!isVerifying}
                closeOnEscape={!isVerifying}
                hideCloseButton
                maxWidth="md"
                zIndexClass="z-50"
            >
                <div className="space-y-6 py-2">
                    <div className="text-center space-y-4">
                        <div className="mx-auto w-16 h-16 bg-slate-100 text-slate-400 rounded-3xl flex items-center justify-center">
                            <Lock size={32} />
                        </div>
                        <div className="space-y-1">
                            <h2 className="text-2xl font-black text-slate-900 tracking-tight uppercase">
                                {t('profile.deleteProfile')}
                            </h2>
                            <p className="text-[10px] font-black uppercase tracking-widest text-slate-400">
                                {t('profile.authorizeDeletionFor', { name: activeProfileName })}
                            </p>
                        </div>
                    </div>

                    <form
                        className="space-y-6"
                        onSubmit={(e) => {
                            e.preventDefault();
                            void onVerifyPassword();
                        }}
                    >
                        <div className="space-y-2">
                            <label
                                htmlFor="delete-password-input"
                                className="text-[10px] font-black text-slate-400 uppercase tracking-widest"
                            >
                                {t('profile.walletPassword')}
                            </label>
                            <Input
                                id="delete-password-input"
                                type="password"
                                value={deletePassword}
                                onChange={(e) => {
                                    onPasswordChange(e.target.value);
                                    if (deleteError) onClearError();
                                }}
                                placeholder={t('common.required')}
                                autoFocus
                            />
                        </div>

                        {deleteError && (
                            <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3 animate-in shake duration-500">
                                <AlertTriangle className="text-rose-500 shrink-0" size={18} />
                                <p className="text-xs font-bold text-rose-800 leading-tight">{deleteError}</p>
                            </div>
                        )}

                        <div className="flex gap-4">
                            <Button
                                type="button"
                                variant="secondary"
                                onClick={onCancelPrompt}
                                disabled={isVerifying}
                                className="flex-1 rounded-2xl"
                            >
                                {t('common.cancel')}
                            </Button>
                            <Button
                                type="submit"
                                disabled={isVerifying || !deletePassword}
                                className="flex-1 rounded-3xl shadow-md"
                            >
                                {isVerifying ? t('profile.verifying') : t('profile.verify')}
                            </Button>
                        </div>
                    </form>
                </div>
            </ModalShell>

            <ModalShell
                isOpen={showConfirmModal}
                onClose={isDeleting ? undefined : onCancelConfirm}
                closeOnBackdrop={!isDeleting}
                closeOnEscape={!isDeleting}
                hideCloseButton
                maxWidth="xl"
                zIndexClass="z-50"
                className="border-rose-200"
            >
                <div className="space-y-6 py-2">
                    <div className="text-center space-y-4">
                        <div className="mx-auto w-20 h-20 bg-rose-500 text-white rounded-[32px] flex items-center justify-center shadow-premium-lg">
                            <UserX size={40} />
                        </div>
                        <div className="space-y-1">
                            <h2 className="text-3xl font-black text-rose-900 tracking-tighter uppercase flex items-center justify-center gap-2">
                                <span>{t('profile.deleteProfile')}</span>
                                <HelpIcon topic="profile" size={20} />
                            </h2>
                            <p className="text-[10px] font-black uppercase tracking-[0.2em] text-rose-500">
                                {t('profile.identity')} {activeProfileName}
                            </p>
                        </div>
                    </div>

                    <div className="p-6 bg-slate-50 border border-slate-200 rounded-[32px] space-y-3">
                        <p className="text-[10px] font-black text-slate-400 uppercase tracking-widest">
                            {t('profile.userId')}
                        </p>
                        <p className="text-xs font-mono break-all text-slate-800 bg-white p-3 rounded-2xl border border-slate-100">
                            {deleteUserId}
                        </p>
                    </div>

                    <div className="p-6 bg-rose-50 border border-rose-100 rounded-[32px] space-y-4">
                        <div className="flex items-center gap-2 text-rose-600">
                            <ShieldAlert size={18} />
                            <h3 className="text-xs font-black uppercase tracking-widest">
                                {t('profile.restorationAdvisory')}
                            </h3>
                        </div>
                        <p className="text-sm font-medium text-rose-900 leading-relaxed">
                            {t('profile.restoreWarning')}
                        </p>
                    </div>

                    {deleteError && (
                        <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3 animate-in shake duration-500">
                            <AlertTriangle className="text-rose-500 shrink-0" size={18} />
                            <p className="text-xs font-bold text-rose-800 leading-tight">{deleteError}</p>
                        </div>
                    )}

                    <div className="flex gap-4">
                        <Button
                            type="button"
                            variant="secondary"
                            onClick={onCancelConfirm}
                            className="flex-1 rounded-2xl"
                        >
                            {t('common.abort')}
                        </Button>
                        <Button
                            type="button"
                            onClick={() => void onConfirmDelete()}
                            disabled={isDeleting}
                            className="flex-1 rounded-3xl !bg-rose-600 hover:!bg-rose-700 shadow-premium-lg"
                        >
                            {isDeleting ? t('profile.deleting') : t('profile.deleteProfile')}
                        </Button>
                    </div>
                </div>
            </ModalShell>
        </>
    );
};
