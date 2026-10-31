import React from 'react';
import { BackButton } from './BackButton';

export interface PageLayoutProps {
    title?: string | React.ReactNode;
    description?: string;
    onBack?: () => void;
    actions?: React.ReactNode;
    customHeader?: React.ReactNode;
    maxWidth?: '3xl' | '4xl' | '5xl' | '6xl' | 'full';
    children: React.ReactNode;
    className?: string;
    contentClassName?: string;
}

const MAX_WIDTH_MAP: Record<NonNullable<PageLayoutProps['maxWidth']>, string> = {
    '3xl': 'max-w-3xl',
    '4xl': 'max-w-4xl',
    '5xl': 'max-w-5xl',
    '6xl': 'max-w-6xl',
    'full': 'max-w-full',
};

export function PageLayout({
    title,
    description,
    onBack,
    actions,
    customHeader,
    maxWidth,
    children,
    className = '',
    contentClassName = '',
}: PageLayoutProps) {
    const maxWidthClass = maxWidth ? MAX_WIDTH_MAP[maxWidth] : '';

    return (
        <div className={`flex flex-col min-h-full ${className}`.trim()}>
            {customHeader ? (
                customHeader
            ) : (title || onBack || actions) ? (
                <header className="flex-shrink-0 sticky top-0 z-20 px-4 sm:px-6 py-3.5 mb-5 glass-panel border-b-0 rounded-b-3xl">
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
                                        <p className="text-[11px] sm:text-xs font-medium text-theme-light uppercase tracking-wider opacity-75 mt-0.5 leading-snug">{description}</p>
                                    )}
                                </div>
                            )}
                        </div>
                        {actions && (
                            <div className="flex flex-wrap items-center gap-2 sm:justify-end shrink-0 max-w-full ml-auto">
                                {actions}
                            </div>
                        )}
                    </div>
                </header>
            ) : null}

            <div className={`flex-grow py-2 ${contentClassName}`.trim()}>
                {maxWidthClass ? (
                    <div className={`${maxWidthClass} mx-auto w-full`}>
                        {children}
                    </div>
                ) : (
                    children
                )}
            </div>
        </div>
    );
}

export default PageLayout;
