import React, { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { Input } from './Input';

export interface PasswordPairFieldsProps {
    /** Current value of the primary password input. */
    password: string;
    /** Current value of the confirmation password input. */
    confirmPassword: string;
    /** Change handler for the primary password input. */
    onPasswordChange: (value: string) => void;
    /** Change handler for the confirmation password input. */
    onConfirmPasswordChange: (value: string) => void;
    /** Label for the primary password input. Defaults to `t('auth.password')`. */
    passwordLabel?: React.ReactNode;
    /** Label for the confirmation password input. Defaults to `t('auth.confirmPassword')`. */
    confirmLabel?: React.ReactNode;
    /** Placeholder for the primary password input. Defaults to `t('auth.passwordPlaceholder')`. */
    passwordPlaceholder?: string;
    /** Placeholder for the confirmation password input. Defaults to `t('auth.repeatPassword')`. */
    confirmPlaceholder?: string;
    /** Show eye visibility toggles on both inputs. Defaults to true. */
    showToggle?: boolean;
    /** Minimum password length attribute and length-check threshold. */
    minLength?: number;
    /** Optional hint slot rendered below the primary password input. */
    minLengthHint?: React.ReactNode;
    /** Optional custom mismatch error message. Defaults to `t('auth.passwordsDontMatch')`. */
    mismatchMessage?: React.ReactNode;
    /** Explicit override for showing mismatch feedback. Defaults to `confirmPassword.length > 0 && password !== confirmPassword`. */
    showMismatch?: boolean;
    /** Two-column grid on sm+ screens or vertical stack. Defaults to 'grid'. */
    layout?: 'grid' | 'stack';
    /** Mark inputs as required. */
    required?: boolean;
    /** Disable both inputs. */
    disabled?: boolean;
    /** Autocomplete hint forwarded to both inputs. Defaults to 'new-password'. */
    autoComplete?: string;
    /** Optional ID for the primary password input. */
    passwordId?: string;
    /** Optional ID for the confirmation password input. */
    confirmId?: string;
    /** Optional test ID for the primary password input. */
    passwordTestId?: string;
    /** Optional test ID for the confirmation password input. */
    confirmTestId?: string;
    /** Optional ref forwarded to the primary password input. */
    passwordRef?: React.Ref<HTMLInputElement>;
    /** Additional CSS classes for the outer wrapper. */
    className?: string;
}

/**
 * Controlled password and confirmation input pair with optional visibility toggles,
 * mismatch feedback, and a minimum-length hint slot.
 */
export const PasswordPairFields: React.FC<PasswordPairFieldsProps> = ({
    password,
    confirmPassword,
    onPasswordChange,
    onConfirmPasswordChange,
    passwordLabel,
    confirmLabel,
    passwordPlaceholder,
    confirmPlaceholder,
    showToggle = true,
    minLength,
    minLengthHint,
    mismatchMessage,
    showMismatch,
    layout = 'grid',
    required = false,
    disabled = false,
    autoComplete = 'new-password',
    passwordId,
    confirmId,
    passwordTestId,
    confirmTestId,
    passwordRef,
    className = '',
}) => {
    const { t } = useTranslation();
    const generatedId = useId();

    const resolvedPasswordId = passwordId ?? `${generatedId}-password`;
    const resolvedConfirmId = confirmId ?? `${generatedId}-confirm`;

    const isMismatch = showMismatch ?? (confirmPassword.length > 0 && password !== confirmPassword);
    const isBelowMinLength = minLength !== undefined && minLength > 0 && password.length > 0 && password.length < minLength;

    const resolvedHint =
        minLengthHint !== undefined
            ? minLengthHint
            : minLength === 8
              ? t('auth.passwordMinChars')
              : undefined;

    return (
        <div className={`space-y-2 ${className}`}>
            <div className={layout === 'grid' ? 'grid grid-cols-1 sm:grid-cols-2 gap-4' : 'space-y-4'}>
                <div className="space-y-1.5">
                    <label
                        htmlFor={resolvedPasswordId}
                        className="block text-xs font-bold text-theme-secondary"
                    >
                        {passwordLabel ?? t('auth.password')}
                    </label>
                    <Input
                        ref={passwordRef}
                        id={resolvedPasswordId}
                        data-testid={passwordTestId}
                        type="password"
                        showToggle={showToggle}
                        value={password}
                        onChange={(e) => onPasswordChange(e.target.value)}
                        placeholder={passwordPlaceholder ?? t('auth.passwordPlaceholder')}
                        minLength={minLength}
                        required={required}
                        disabled={disabled}
                        autoComplete={autoComplete}
                        aria-invalid={isBelowMinLength || undefined}
                        className={isBelowMinLength ? 'border-amber-400 focus:border-amber-500 focus:ring-amber-500/20' : ''}
                    />
                    {resolvedHint && (
                        <p
                            data-testid="password-minlength-hint"
                            className={`text-[11px] leading-snug ${
                                isBelowMinLength ? 'text-amber-700 font-medium' : 'text-theme-light'
                            }`}
                        >
                            {resolvedHint}
                        </p>
                    )}
                </div>

                <div className="space-y-1.5">
                    <label
                        htmlFor={resolvedConfirmId}
                        className="block text-xs font-bold text-theme-secondary"
                    >
                        {confirmLabel ?? t('auth.confirmPassword')}
                    </label>
                    <Input
                        id={resolvedConfirmId}
                        data-testid={confirmTestId}
                        type="password"
                        showToggle={showToggle}
                        value={confirmPassword}
                        onChange={(e) => onConfirmPasswordChange(e.target.value)}
                        placeholder={confirmPlaceholder ?? t('auth.repeatPassword')}
                        minLength={minLength}
                        required={required}
                        disabled={disabled}
                        autoComplete={autoComplete}
                        aria-invalid={isMismatch || undefined}
                        className={isMismatch ? 'border-theme-error focus:border-theme-error focus:ring-theme-error/20' : ''}
                    />
                </div>
            </div>

            {isMismatch && (
                <p
                    role="alert"
                    data-testid="password-mismatch-error"
                    className="text-xs font-semibold text-theme-error"
                >
                    {mismatchMessage ?? t('auth.passwordsDontMatch')}
                </p>
            )}
        </div>
    );
};
