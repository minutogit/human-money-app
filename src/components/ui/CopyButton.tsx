import React from 'react';
import { useTranslation } from 'react-i18next';
import { Copy, Check } from 'lucide-react';
import { Button, ButtonProps } from './Button';
import { useCopyFeedback } from '../../hooks/useCopyFeedback';

export interface CopyButtonProps {
    /** Text value to copy to the clipboard. */
    value: string;
    /** Default button label. Defaults to `t('common.copy')`. */
    label?: React.ReactNode;
    /** Label shown while in the copied feedback state. Defaults to `t('common.copied')`. */
    copiedLabel?: React.ReactNode;
    /** Render only the Copy/Check icon without visible text. */
    iconOnly?: boolean;
    /** Visual button variant. Defaults to 'outline'. */
    variant?: ButtonProps['variant'];
    /** Optional variant override while copied feedback is active. */
    copiedVariant?: ButtonProps['variant'];
    /** Button size token. Defaults to 'sm'. */
    size?: ButtonProps['size'];
    /** Icon pixel size. Defaults to 14. */
    iconSize?: number;
    /** Feedback duration in milliseconds. Defaults to 2000. */
    resetDelayMs?: number;
    /** Accessible label override. */
    ariaLabel?: string;
    /** Tooltip title override. */
    title?: string;
    /** Disable interactions. */
    disabled?: boolean;
    /** Stop click event propagation (useful inside clickable cards/rows). */
    stopPropagation?: boolean;
    /** Optional extra CSS classes. */
    className?: string;
    /** Callback invoked after a successful copy. */
    onCopied?: (value: string) => void;
    /** Callback invoked when copying fails or value is empty. */
    onCopyFailed?: () => void;
    /** Optional test ID. */
    'data-testid'?: string;
}

/**
 * Reusable clipboard copy button powered by `useCopyFeedback`.
 */
export const CopyButton: React.FC<CopyButtonProps> = ({
    value,
    label,
    copiedLabel,
    iconOnly = false,
    variant = 'outline',
    copiedVariant,
    size = 'sm',
    iconSize = 14,
    resetDelayMs = 2000,
    ariaLabel,
    title,
    disabled = false,
    stopPropagation = false,
    className = '',
    onCopied,
    onCopyFailed,
    'data-testid': testId,
}) => {
    const { t } = useTranslation();
    const { copied, copyText } = useCopyFeedback(resetDelayMs);

    const resolvedLabel = label ?? t('common.copy');
    const resolvedCopiedLabel = copiedLabel ?? t('common.copied');
    const activeVariant = copied && copiedVariant ? copiedVariant : variant;

    const computedAriaLabel =
        ariaLabel ??
        (copied
            ? typeof resolvedCopiedLabel === 'string'
                ? resolvedCopiedLabel
                : t('common.copied')
            : typeof resolvedLabel === 'string'
              ? resolvedLabel
              : t('common.copy'));

    const handleClick = async (event: React.MouseEvent<HTMLButtonElement>) => {
        if (stopPropagation) {
            event.stopPropagation();
        }
        const ok = await copyText(value);
        if (ok) {
            onCopied?.(value);
        } else {
            onCopyFailed?.();
        }
    };

    const copiedIconColor =
        activeVariant === 'primary' || activeVariant === 'danger'
            ? ''
            : 'text-theme-success';

    return (
        <Button
            type="button"
            variant={activeVariant}
            size={size}
            disabled={disabled || !value}
            onClick={handleClick}
            aria-label={computedAriaLabel}
            title={title ?? computedAriaLabel}
            data-testid={testId}
            className={`gap-1.5 ${iconOnly ? 'px-2.5' : ''} ${className}`}
        >
            {copied ? (
                <Check size={iconSize} className={`shrink-0 ${copiedIconColor}`} aria-hidden="true" />
            ) : (
                <Copy size={iconSize} className="shrink-0" aria-hidden="true" />
            )}
            {!iconOnly && <span>{copied ? resolvedCopiedLabel : resolvedLabel}</span>}
        </Button>
    );
};
