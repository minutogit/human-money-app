// src/components/ui/TechId.tsx
import React, { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Check, Copy, Hash } from 'lucide-react';
import { UserAvatar } from './UserAvatar';
import { useUiMode } from '../../context/UiModeContext';
import { copyToClipboard } from '../../utils/clipboard';

export type TechIdKind = 'did' | 'bundle' | 'proof' | 'device' | 'voucher' | 'hash';

export interface TechIdProps {
    value: string;
    kind?: TechIdKind;
    label?: string;
    copyable?: boolean;
    compact?: boolean;
    className?: string;
}

export function formatShortFingerprint(value: string): string {
    const trimmed = value.trim();
    if (trimmed.length <= 8) return trimmed;
    return `…${trimmed.slice(-6)}`;
}

/**
 * Displays technical identifiers safely per UI mode:
 * - Simple mode: compact Identicon (or #) + last-6 fingerprint + copy button (no raw did:key leak in DOM).
 * - Expert mode: full monospace identifier + copy button.
 */
export const TechId: React.FC<TechIdProps> = ({
    value,
    kind = 'did',
    label,
    copyable = true,
    compact = false,
    className = '',
}) => {
    const { t } = useTranslation();
    const { isExpert } = useUiMode();
    const [copied, setCopied] = useState(false);
    const timeoutRef = useRef<number | null>(null);

    useEffect(() => {
        return () => {
            if (timeoutRef.current !== null) {
                window.clearTimeout(timeoutRef.current);
            }
        };
    }, []);

    if (!value) return null;

    const handleCopy = async (e: React.MouseEvent) => {
        e.stopPropagation();
        const ok = await copyToClipboard(value);
        if (ok) {
            setCopied(true);
            if (timeoutRef.current !== null) {
                window.clearTimeout(timeoutRef.current);
            }
            timeoutRef.current = window.setTimeout(() => setCopied(false), 1800);
        }
    };

    const showAvatar = kind === 'did' || value.startsWith('did:');
    const shortFingerprint = formatShortFingerprint(value);

    return (
        <span
            className={`inline-flex items-center max-w-full border border-theme-subtle/70 bg-bg-app/70 text-theme-secondary ${
                compact
                    ? 'gap-1 rounded-md px-1.5 py-0 text-[10px]'
                    : 'gap-1.5 rounded-lg px-2 py-0.5 text-xs'
            } ${className}`}
            data-testid="tech-id"
            data-mode={isExpert ? 'expert' : 'simple'}
        >
            {showAvatar ? (
                <span className="shrink-0 inline-flex items-center" aria-hidden="true">
                    <UserAvatar size={compact ? 14 : 18} />
                </span>
            ) : (
                <Hash size={compact ? 10 : 12} className="shrink-0 text-theme-light" aria-hidden="true" />
            )}

            {label && (
                <span className={`${compact ? 'text-[9px]' : 'text-[11px]'} font-medium text-theme-light shrink-0`}>
                    {label}:
                </span>
            )}

            {isExpert ? (
                <code
                    className={`font-mono text-theme-secondary break-all select-all ${
                        compact ? 'text-[9px] leading-tight' : 'text-[11px]'
                    }`}
                    data-testid="tech-id-value"
                >
                    {value}
                </code>
            ) : (
                <span
                    className={`font-mono font-semibold text-theme-secondary ${
                        compact ? 'text-[9px] leading-tight' : 'text-[11px]'
                    }`}
                    data-testid="tech-id-value"
                >
                    {shortFingerprint}
                </span>
            )}

            {copyable && (
                <button
                    type="button"
                    onClick={handleCopy}
                    className="shrink-0 p-0.5 rounded text-theme-light hover:text-theme-primary transition-colors cursor-pointer"
                    aria-label={copied ? t('common.copied') : t('common.copyId')}
                    title={copied ? t('common.copied') : t('common.copyId')}
                    data-testid="tech-id-copy"
                >
                    {copied ? (
                        <Check size={compact ? 10 : 12} className="text-theme-success" />
                    ) : (
                        <Copy size={compact ? 10 : 12} />
                    )}
                </button>
            )}
        </span>
    );
};
