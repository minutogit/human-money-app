import React, { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { MoreHorizontal } from 'lucide-react';
import { BackButton } from './BackButton';
import { useUiMode } from '../../context/UiModeContext';

export interface PageHeaderProps {
    title?: string | React.ReactNode;
    description?: string | React.ReactNode;
    onBack?: () => void;
    actions?: React.ReactNode;
    advancedActions?: React.ReactNode;
    className?: string;
}

export function PageHeader({
    title,
    description,
    onBack,
    actions,
    advancedActions,
    className = '',
}: PageHeaderProps) {
    const { t } = useTranslation();
    const { isExpert } = useUiMode();
    const [isMoreOpen, setIsMoreOpen] = useState(false);
    const menuRef = useRef<HTMLDivElement | null>(null);

    useEffect(() => {
        if (!isMoreOpen) return;

        const handlePointerDown = (event: MouseEvent) => {
            if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
                setIsMoreOpen(false);
            }
        };

        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape') {
                setIsMoreOpen(false);
            }
        };

        document.addEventListener('mousedown', handlePointerDown);
        document.addEventListener('keydown', handleKeyDown);
        return () => {
            document.removeEventListener('mousedown', handlePointerDown);
            document.removeEventListener('keydown', handleKeyDown);
        };
    }, [isMoreOpen]);

    if (!title && !onBack && !actions && !advancedActions) {
        return null;
    }

    return (
        <header className={`flex-shrink-0 sticky top-0 z-20 px-4 sm:px-6 py-3.5 mb-5 glass-panel border-b-0 rounded-b-3xl ${className}`.trim()}>
            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-3">
                <div className="flex items-center gap-3 min-w-[min(100%,14rem)] flex-[1_1_14rem]">
                    {onBack && <BackButton onClick={onBack} />}
                    {title && (
                        <div className="flex flex-col min-w-0">
                            {typeof title === 'string' ? (
                                <h1 className="text-xl sm:text-2xl font-black text-theme-primary tracking-tight leading-tight [overflow-wrap:anywhere]">{title}</h1>
                            ) : (
                                title
                            )}
                            {description && (
                                typeof description === 'string' ? (
                                    <p className="text-[11px] sm:text-xs font-medium text-theme-light uppercase tracking-wider opacity-75 mt-0.5 leading-snug">{description}</p>
                                ) : (
                                    <div className="text-[11px] sm:text-xs font-medium text-theme-light mt-0.5 leading-snug">{description}</div>
                                )
                            )}
                        </div>
                    )}
                </div>
                {(actions || advancedActions) && (
                    <div className="flex flex-wrap items-center gap-2 sm:justify-end shrink-0 max-w-full ml-auto">
                        {actions}
                        {advancedActions && isExpert && advancedActions}
                        {advancedActions && !isExpert && (
                            <div className="relative" ref={menuRef}>
                                <button
                                    type="button"
                                    onClick={() => setIsMoreOpen((prev) => !prev)}
                                    aria-label={t('common.moreActions')}
                                    aria-expanded={isMoreOpen}
                                    data-testid="page-header-more-actions"
                                    className="inline-flex items-center justify-center p-2 rounded-xl border border-theme-subtle bg-white dark:bg-slate-900 text-theme-secondary hover:border-theme-primary/40 hover:text-theme-primary transition-colors cursor-pointer"
                                >
                                    <MoreHorizontal size={18} />
                                </button>
                                {isMoreOpen && (
                                    <div
                                        data-testid="page-header-more-menu"
                                        onClick={() => setIsMoreOpen(false)}
                                        className="absolute right-0 mt-2 min-w-[14rem] z-40 rounded-2xl border border-theme-subtle bg-white dark:bg-slate-900 shadow-xl p-2 flex flex-col gap-1.5"
                                    >
                                        {advancedActions}
                                    </div>
                                )}
                            </div>
                        )}
                    </div>
                )}
            </div>
        </header>
    );
}

export default PageHeader;
