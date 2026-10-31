import React from 'react';
import { useTranslation } from 'react-i18next';

interface PageLayoutProps {
    title?: string | React.ReactNode;
    description?: string;
    onBack?: () => void;
    actions?: React.ReactNode;
    customHeader?: React.ReactNode;
    children: React.ReactNode;
    className?: string;
}

export function PageLayout({
    title,
    description,
    onBack,
    actions,
    customHeader,
    children,
    className = ""
}: PageLayoutProps) {
    const { t } = useTranslation();

    return (
        <div className={`flex flex-col h-full ${className}`}>
            {customHeader ? (
                customHeader
            ) : (title || onBack) ? (
                <header className="flex-shrink-0 sticky top-0 z-20 px-6 py-4 mb-2 glass-panel border-b-0 rounded-b-3xl">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <div className="flex items-center gap-3 min-w-0 flex-1">
                            {onBack && (
                                <button
                                    onClick={onBack}
                                    className="p-2.5 rounded-full bg-white/50 border border-theme-subtle hover:bg-theme-primary hover:text-white transition-all text-theme-light shadow-sm active:scale-90 shrink-0"
                                    title={t('common.back', 'Back')}
                                    aria-label={t('common.back', 'Back')}
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                                    </svg>
                                </button>
                            )}
                            {title && (
                                <div className="flex flex-col min-w-0">
                                    {typeof title === 'string' ? (
                                        <h1 className="text-2xl font-black text-theme-primary tracking-tight break-words">{title}</h1>
                                    ) : (
                                        title
                                    )}
                                    {description && (
                                        <p className="text-xs font-medium text-theme-light uppercase tracking-widest opacity-70">{description}</p>
                                    )}
                                </div>
                            )}
                        </div>
                        {actions && (
                            <div className="flex flex-wrap items-center justify-end gap-2 shrink-0 max-w-full">
                                {actions}
                            </div>
                        )}
                    </div>
                </header>
            ) : null}

            <div className="flex-grow overflow-y-auto px-6 py-4">
                {children}
            </div>
        </div>
    );
}
