import React from 'react';
import { PageHeader } from './PageHeader';

export interface PageLayoutProps {
    title?: string | React.ReactNode;
    description?: string | React.ReactNode;
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
            ) : (
                <PageHeader
                    title={title}
                    description={description}
                    onBack={onBack}
                    actions={actions}
                />
            )}

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
