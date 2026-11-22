import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { Input } from './Input';
import { ModalShell } from './ModalShell';

interface AuthModalProps {
    isOpen: boolean;
    onConfirm: (password: string) => void;
    onCancel: () => void;
    title?: string;
    description?: string;
    errorMsg?: string;
    credentialKind?: 'password' | 'pin';
    canSwitchToPassword?: boolean;
    onSwitchToPassword?: () => void;
    isSubmitting?: boolean;
}

export function AuthModal({
    isOpen,
    onConfirm,
    onCancel,
    title,
    description,
    errorMsg,
    credentialKind = 'password',
    canSwitchToPassword = false,
    onSwitchToPassword,
    isSubmitting = false,
}: AuthModalProps) {
    const { t } = useTranslation();
    const [password, setPassword] = useState('');
    const isPin = credentialKind === 'pin';

    // Reset credential field when modal opens/closes or switches between PIN and password
    useEffect(() => {
        setPassword('');
    }, [isOpen, credentialKind]);

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        if (isSubmitting) return;
        onConfirm(password);
    };

    const resolvedTitle = title || (isPin ? t('auth.pinReauthTitle') : t('auth.authenticationRequired'));
    const resolvedDescription = description || (isPin ? t('auth.pinReauthDesc') : t('auth.enterPasswordToContinue'));

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onCancel}
            closeOnBackdrop={false}
            closeOnEscape={!isSubmitting}
            hideCloseButton={true}
            maxWidth="md"
            zIndexClass="z-50"
            title={resolvedTitle}
            description={resolvedDescription}
        >
            <div className="space-y-4">
                {errorMsg && (
                    <div className="rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-xs font-semibold text-rose-700">
                        {errorMsg}
                    </div>
                )}

                <form onSubmit={handleSubmit} className="space-y-4">
                    <Input
                        type="password"
                        inputMode={isPin ? 'numeric' : undefined}
                        maxLength={isPin ? 8 : undefined}
                        value={password}
                        disabled={isSubmitting}
                        onChange={(e) => {
                            const val = e.target.value;
                            setPassword(isPin ? val.replace(/\D/g, '').slice(0, 8) : val);
                        }}
                        placeholder={isPin ? t('auth.enterQuickPinPlaceholder') : t('auth.walletPasswordPlaceholder')}
                        autoFocus
                    />
                    <div className="flex items-center justify-between gap-3 pt-2">
                        <div>
                            {isPin && canSwitchToPassword && onSwitchToPassword && (
                                <button
                                    type="button"
                                    data-testid="auth-modal-switch-password"
                                    disabled={isSubmitting}
                                    onClick={onSwitchToPassword}
                                    className="text-xs font-semibold text-theme-primary hover:underline cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                                >
                                    {t('auth.useMasterPasswordInstead')}
                                </button>
                            )}
                        </div>
                        <div className="flex justify-end gap-3">
                            <Button type="button" variant="secondary" disabled={isSubmitting} onClick={onCancel}>{t('common.cancel')}</Button>
                            <Button type="submit" disabled={isSubmitting || (isPin ? password.length < 6 : !password)}>{t('common.confirm')}</Button>
                        </div>
                    </div>
                </form>
            </div>
        </ModalShell>
    );
}
