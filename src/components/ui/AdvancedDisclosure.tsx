// src/components/ui/AdvancedDisclosure.tsx
import React, { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { UiMode } from '../../types';
import { useUiMode } from '../../context/UiModeContext';
import { Disclosure } from './Disclosure';

export interface AdvancedDisclosureProps {
    title?: React.ReactNode;
    subtitle?: React.ReactNode;
    icon?: React.ReactNode;
    isCustom?: boolean;
    hasError?: boolean;
    defaultOpen?: boolean;
    toggleTestId?: string;
    contentTestId?: string;
    className?: string;
    buttonClassName?: string;
    contentClassName?: string;
    children: React.ReactNode;
}

const FOCUS_SELECTORS = [
    '[aria-invalid="true"]',
    'input:invalid, select:invalid, textarea:invalid',
    'input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled])',
    'button:not([disabled])',
];

function findFocusTarget(root: HTMLElement): HTMLElement | null {
    const toggleBtn = root.querySelector<HTMLButtonElement>('button[aria-controls]');
    const contentId = toggleBtn?.getAttribute('aria-controls');
    const contentChild = contentId
        ? Array.from(root.children).find(
              (el): el is HTMLElement => el instanceof HTMLElement && el.id === contentId
          )
        : null;
    const scope = contentChild || root;

    for (const selector of FOCUS_SELECTORS) {
        try {
            const candidate = scope.querySelector<HTMLElement>(selector);
            if (candidate && typeof candidate.focus === 'function') {
                return candidate;
            }
        } catch {
            // Skip pseudo-selectors not supported in constrained DOM engines
        }
    }
    return null;
}

/**
 * Mode-aware collapsible section for settings/options that already have safe defaults.
 * Simple mode: collapsed by default (shows "Angepasst"/"Customized" badge if `isCustom` is true).
 * Expert mode: open by default.
 * Automatically opens and focuses the first invalid/focusable descendant on `hasError` or HTML5 `invalid`.
 */
export const AdvancedDisclosure: React.FC<AdvancedDisclosureProps> = ({
    title,
    subtitle,
    icon,
    isCustom = false,
    hasError = false,
    defaultOpen = false,
    toggleTestId,
    contentTestId,
    className = 'border border-theme-subtle rounded-2xl overflow-hidden bg-white/50 dark:bg-slate-900/50',
    buttonClassName = 'w-full flex items-center justify-between gap-3 p-4 bg-slate-50/70 dark:bg-slate-800/40 hover:bg-slate-100/70 transition-colors text-left cursor-pointer',
    contentClassName = 'p-4 border-t border-theme-subtle bg-white dark:bg-slate-900',
    children,
}) => {
    const { t } = useTranslation();
    const { mode, isExpert } = useUiMode();
    const containerRef = useRef<HTMLDivElement | null>(null);
    const pendingFocusRef = useRef<HTMLElement | null>(null);

    const [manualState, setManualState] = useState<{ forMode: UiMode; open: boolean } | null>(null);
    const [errorOpened, setErrorOpened] = useState(false);

    const modeDefaultOpen = isExpert || defaultOpen;
    const isManualForCurrentMode = manualState?.forMode === mode;
    const isOpen =
        hasError ||
        errorOpened ||
        (isManualForCurrentMode ? manualState.open : modeDefaultOpen);

    useEffect(() => {
        if (!hasError && !errorOpened) return;

        const target = pendingFocusRef.current;
        pendingFocusRef.current = null;

        if (target && typeof target.focus === 'function') {
            target.focus();
            return;
        }

        const root = containerRef.current;
        if (!root) return;

        const candidate = findFocusTarget(root);
        if (candidate) {
            candidate.focus();
        }
    }, [hasError, errorOpened, isOpen]);

    const handleInvalidCapture: React.FormEventHandler<HTMLDivElement> = (event) => {
        if (event.target instanceof HTMLElement) {
            pendingFocusRef.current = event.target;
        }
        setErrorOpened(true);
    };

    const handleOpenChange = (nextOpen: boolean) => {
        setManualState({ forMode: mode, open: nextOpen });
        if (!nextOpen) {
            setErrorOpened(false);
        }
    };

    const resolvedTitle = title ?? (
        <span className="text-xs font-bold text-theme-secondary">
            {t('common.moreOptions')}
        </span>
    );

    const customBadge =
        isCustom && !isOpen ? (
            <span
                className="px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider rounded-full bg-theme-primary/10 text-theme-primary border border-theme-primary/20"
                data-testid="advanced-disclosure-custom-badge"
            >
                {t('common.customized')}
            </span>
        ) : undefined;

    return (
        <Disclosure
            ref={containerRef}
            title={resolvedTitle}
            subtitle={subtitle}
            badge={customBadge}
            icon={icon}
            open={isOpen}
            onOpenChange={handleOpenChange}
            onInvalidCapture={handleInvalidCapture}
            toggleTestId={toggleTestId}
            contentTestId={contentTestId}
            className={className}
            buttonClassName={buttonClassName}
            contentClassName={contentClassName}
        >
            {children}
        </Disclosure>
    );
};
