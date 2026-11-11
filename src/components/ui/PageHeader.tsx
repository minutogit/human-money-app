import React from 'react';
import { BackButton } from './BackButton';

export interface PageHeaderProps {
    title?: string | React.ReactNode;
    description?: string | React.ReactNode;
    onBack?: () => void;
    actions?: React.ReactNode;
    className?: string;
}

export function PageHeader({
    title,
    description,
    onBack,
    actions,
    className = '',
}: PageHeaderProps) {
    if (!title && !onBack && !actions) {
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
                {actions && (
                    <div className="flex flex-wrap items-center gap-2 sm:justify-end shrink-0 max-w-full ml-auto">
                        {actions}
                    </div>
                )}
            </div>
        </header>
    );
}

export default PageHeader;
