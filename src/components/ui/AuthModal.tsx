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
}

export function AuthModal({ isOpen, onConfirm, onCancel, title, description, errorMsg }: AuthModalProps) {
    const { t } = useTranslation();
    const [password, setPassword] = useState('');

    // Reset password field when modal opens
    useEffect(() => {
        if (isOpen) setPassword('');
    }, [isOpen]);

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        onConfirm(password);
    };

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={onCancel}
            closeOnBackdrop={false}
            hideCloseButton={true}
            maxWidth="md"
            zIndexClass="z-50"
            title={title || t('auth.authenticationRequired')}
            description={description || t('auth.enterPasswordToContinue')}
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
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        placeholder={t('auth.walletPasswordPlaceholder')}
                        autoFocus
                    />
                    <div className="flex justify-end gap-3 pt-2">
                        <Button type="button" variant="secondary" onClick={onCancel}>{t('common.cancel')}</Button>
                        <Button type="submit" disabled={!password}>{t('common.confirm')}</Button>
                    </div>
                </form>
            </div>
        </ModalShell>
    );
}
