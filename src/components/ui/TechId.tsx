// src/components/ui/TechId.tsx
import React, { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import Avatar from 'boring-avatars';
import { Check, Copy, Hash } from 'lucide-react';
import { useUiMode } from '../../context/UiModeContext';
import { copyToClipboard } from '../../utils/clipboard';

export type TechIdKind = 'did' | 'bundle' | 'proof' | 'device' | 'voucher' | 'hash';

export interface TechIdProps {
    value: string;
    kind?: TechIdKind;
    label?: string;
    copyable?: boolean;
    className?: string;
}

const AVATAR_COLORS = ['#E63946', '#2B1B17', '#E76F51', '#F4A261', '#2A9D8F'];

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
            className={`inline-flex items-center gap-1.5 max-w-full rounded-lg border border-theme-subtle/70 bg-bg-app/70 px-2 py-0.5 text-xs text-theme-secondary ${className}`}
            data-testid="tech-id"
            data-mode={isExpert ? 'expert' : 'simple'}
        >
            {showAvatar ? (
                <span className="shrink-0 inline-flex items-center" aria-hidden="true">
                    <Avatar
                        size={18}
                        name={value}
                        variant="beam"
                        colors={AVATAR_COLORS}
                    />
                </span>
            ) : (
                <Hash size={12} className="shrink-0 text-theme-light" aria-hidden="true" />
            )}

            {label && (
                <span className="text-[11px] font-medium text-theme-light shrink-0">
                    {label}:
                </span>
            )}

            {isExpert ? (
                <code
                    className="font-mono text-[11px] text-theme-secondary break-all select-all"
                    data-testid="tech-id-value"
                >
                    {value}
                </code>
            ) : (
                <span
                    className="font-mono text-[11px] font-semibold text-theme-secondary"
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
                        <Check size={12} className="text-theme-success" />
                    ) : (
                        <Copy size={12} />
                    )}
                </button>
            )}
        </span>
    );
};
