// src/components/ui/Disclosure.tsx
import React, { forwardRef, useId, useState } from 'react';
import { ChevronDown } from 'lucide-react';

export interface DisclosureProps {
    title: React.ReactNode;
    subtitle?: React.ReactNode;
    badge?: React.ReactNode;
    icon?: React.ReactNode;
    defaultOpen?: boolean;
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    storageKey?: string;
    toggleTestId?: string;
    contentTestId?: string;
    indicatorDot?: boolean;
    indicatorDotTestId?: string;
    indicatorDotTitle?: string;
    className?: string;
    buttonClassName?: string;
    contentClassName?: string;
    onInvalidCapture?: React.FormEventHandler<HTMLDivElement>;
    children: React.ReactNode;
}

function readStoredOpenState(storageKey: string | undefined, defaultOpen: boolean): boolean {
    if (!storageKey) return defaultOpen;
    try {
        const stored = localStorage.getItem(storageKey);
        if (stored === 'true') return true;
        if (stored === 'false') return false;
    } catch {
        // Ignore storage access errors
    }
    return defaultOpen;
}

export const Disclosure = forwardRef<HTMLDivElement, DisclosureProps>(function Disclosure(
    {
        title,
        subtitle,
        badge,
        icon,
        defaultOpen = false,
        open,
        onOpenChange,
        storageKey,
        toggleTestId,
        contentTestId,
        indicatorDot = false,
        indicatorDotTestId,
        indicatorDotTitle,
        className = '',
        buttonClassName = '',
        contentClassName = '',
        onInvalidCapture,
        children,
    },
    ref
) {
    const contentId = useId();
    const isControlled = open !== undefined;
    const [uncontrolledOpen, setUncontrolledOpen] = useState<boolean>(() =>
        readStoredOpenState(storageKey, defaultOpen)
    );

    const isOpen = isControlled ? Boolean(open) : uncontrolledOpen;

    const handleToggle = () => {
        const next = !isOpen;
        if (!isControlled) {
            setUncontrolledOpen(next);
            if (storageKey) {
                try {
                    localStorage.setItem(storageKey, String(next));
                } catch {
                    // Best-effort storage
                }
            }
        }
        onOpenChange?.(next);
    };

    return (
        <div ref={ref} className={className} onInvalidCapture={onInvalidCapture}>
            <button
                type="button"
                onClick={handleToggle}
                aria-expanded={isOpen}
                aria-controls={contentId}
                data-testid={toggleTestId}
                className={
                    buttonClassName ||
                    'w-full flex items-center justify-between gap-3 px-4 py-3 text-left transition-colors cursor-pointer'
                }
            >
                <div className="flex items-center gap-2.5 min-w-0 flex-1">
                    {icon && <span className="shrink-0">{icon}</span>}
                    <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2 flex-wrap">
                            <span className="min-w-0">{title}</span>
                            {!isOpen && indicatorDot && (
                                <span
                                    className="w-2 h-2 rounded-full bg-rose-500 shrink-0 animate-pulse"
                                    data-testid={indicatorDotTestId}
                                    title={indicatorDotTitle}
                                />
                            )}
                            {badge}
                        </div>
                        {subtitle && <div className="mt-0.5">{subtitle}</div>}
                    </div>
                </div>
                <ChevronDown
                    size={16}
                    className={`shrink-0 text-theme-light transition-transform duration-200 ${
                        isOpen ? 'rotate-180' : ''
                    }`}
                />
            </button>

            {isOpen && (
                <div id={contentId} data-testid={contentTestId} className={contentClassName}>
                    {children}
                </div>
            )}
        </div>
    );
});
