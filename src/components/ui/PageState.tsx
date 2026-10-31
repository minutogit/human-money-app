import React from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, RefreshCw } from 'lucide-react';
import { AlertBanner } from './AlertBanner';
import { Button } from './Button';

export interface PageLoadingStateProps {
    message?: React.ReactNode;
    className?: string;
}

export function PageLoadingState({ message, className = '' }: PageLoadingStateProps) {
    const { t } = useTranslation();

    return (
        <div className={`flex flex-col items-center justify-center min-h-[16rem] py-12 px-4 text-center ${className}`}>
            <div className="p-3.5 rounded-2xl bg-theme-primary/10 text-theme-primary mb-3 shadow-inner-soft animate-pulse-subtle">
                <Loader2 size={28} className="animate-spin text-theme-primary" />
            </div>
            <p className="text-xs sm:text-sm font-bold text-theme-light tracking-wide">
                {message ?? t('common.loading', 'Loading...')}
            </p>
        </div>
    );
}

export interface PageEmptyStateProps {
    icon?: React.ReactNode;
    title: React.ReactNode;
    description?: React.ReactNode;
    action?: React.ReactNode;
    className?: string;
}

export function PageEmptyState({
    icon,
    title,
    description,
    action,
    className = '',
}: PageEmptyStateProps) {
    return (
        <div
            className={`py-16 px-6 flex flex-col items-center justify-center text-center bg-white/40 backdrop-blur-sm rounded-[32px] border-2 border-dashed border-theme-subtle/60 ${className}`}
        >
            {icon && (
                <div className="w-14 h-14 rounded-2xl bg-theme-subtle/20 flex items-center justify-center text-theme-light mb-4 shadow-inner-soft">
                    {icon}
                </div>
            )}
            {typeof title === 'string' ? (
                <h3 className="text-lg sm:text-xl font-black text-theme-primary mb-2 tracking-tight">
                    {title}
                </h3>
            ) : (
                title
            )}
            {description && (
                typeof description === 'string' ? (
                    <p className="text-xs sm:text-sm text-theme-light max-w-md leading-relaxed font-medium mb-6">
                        {description}
                    </p>
                ) : (
                    <div className="text-xs sm:text-sm text-theme-light max-w-md leading-relaxed font-medium mb-6">
                        {description}
                    </div>
                )
            )}
            {action && (
                <div className="flex items-center justify-center gap-3">
                    {action}
                </div>
            )}
        </div>
    );
}

export interface PageErrorStateProps {
    title?: React.ReactNode;
    message: React.ReactNode;
    onRetry?: () => void;
    action?: React.ReactNode;
    className?: string;
}

export function PageErrorState({
    title,
    message,
    onRetry,
    action,
    className = '',
}: PageErrorStateProps) {
    const { t } = useTranslation();

    const retryButton = onRetry ? (
        <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={onRetry}
            className="flex items-center gap-1.5"
        >
            <RefreshCw size={14} />
            <span>{t('common.retry', 'Retry')}</span>
        </Button>
    ) : null;

    const actionNode = (action || retryButton) ? (
        <div className="flex items-center gap-2">
            {retryButton}
            {action}
        </div>
    ) : undefined;

    return (
        <div className={`w-full py-4 ${className}`}>
            <AlertBanner
                variant="error"
                title={title ?? t('common.error', 'Error')}
                action={actionNode}
            >
                {message}
            </AlertBanner>
        </div>
    );
}
