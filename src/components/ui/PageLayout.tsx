import React from 'react';

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
    return (
        <div className={`flex flex-col h-full ${className}`}>
            {customHeader ? (
                customHeader
            ) : (title || onBack) ? (
                <header className="flex-shrink-0 mb-6">
                    <div className="flex items-center gap-4 mb-2">
                        {onBack && (
                            <button
                                onClick={onBack}
                                className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                                title="Back"
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                                </svg>
                            </button>
                        )}
                        {title && (
                            typeof title === 'string' ? (
                                <h1 className="text-2xl font-bold text-theme-primary">{title}</h1>
                            ) : (
                                title
                            )
                        )}
                        {actions && (
                            <div className="flex-grow flex justify-end items-center gap-2">
                                {actions}
                            </div>
                        )}
                        {!actions && <div className="flex-grow"></div>}
                    </div>
                    {description && (
                        <p className="text-theme-light ml-14">{description}</p>
                    )}
                </header>
            ) : null}

            <div className="flex-grow overflow-y-auto">
                {children}
            </div>
        </div>
    );
}
