import React from 'react';
import { useTranslation } from 'react-i18next';
import { CheckCircle2, AlertTriangle, Info, X } from 'lucide-react';

export interface AlertBannerProps {
    variant?: 'success' | 'error' | 'warning' | 'info';
    title?: React.ReactNode;
    children: React.ReactNode;
    icon?: React.ReactNode;
    action?: React.ReactNode;
    onDismiss?: () => void;
    dismissLabel?: string;
    className?: string;
    'data-testid'?: string;
}

const variantStyles: Record<
    NonNullable<AlertBannerProps['variant']>,
    { container: string; defaultIcon: React.ReactNode }
> = {
    success: {
        container: 'bg-emerald-50 border-emerald-200 text-emerald-800',
        defaultIcon: <CheckCircle2 size={18} className="text-emerald-600 shrink-0" />,
    },
    error: {
        container: 'bg-rose-50 border-rose-200 text-rose-800',
        defaultIcon: <AlertTriangle size={18} className="text-rose-500 shrink-0" />,
    },
    warning: {
        container: 'bg-amber-500/10 border-amber-500/25 text-amber-900',
        defaultIcon: <AlertTriangle size={18} className="text-amber-600 shrink-0" />,
    },
    info: {
        container: 'bg-blue-50 border-blue-200 text-blue-800',
        defaultIcon: <Info size={18} className="text-blue-600 shrink-0" />,
    },
};

export function AlertBanner({
    variant = 'info',
    title,
    children,
    icon,
    action,
    onDismiss,
    dismissLabel,
    className = '',
    'data-testid': testId,
}: AlertBannerProps) {
    const { t } = useTranslation();
    const config = variantStyles[variant] ?? variantStyles.info;
    const resolvedIcon = icon !== undefined ? icon : config.defaultIcon;
    const closeText = dismissLabel ?? t('common.close', 'Close');

    return (
        <div
            role="alert"
            data-testid={testId}
            className={`p-4 rounded-2xl border flex items-start justify-between gap-3 animate-in fade-in duration-200 ${config.container} ${className}`}
        >
            <div className="flex items-start gap-3 min-w-0 flex-1">
                {resolvedIcon && (
                    <div className="shrink-0 mt-0.5">
                        {resolvedIcon}
                    </div>
                )}
                <div className="min-w-0 flex-1 [overflow-wrap:anywhere]">
                    {title && (
                        <div className="font-black text-sm tracking-tight mb-0.5">
                            {title}
                        </div>
                    )}
                    <div className="text-xs sm:text-sm font-semibold leading-relaxed">
                        {children}
                    </div>
                </div>
            </div>

            {(action || onDismiss) && (
                <div className="flex items-center gap-2 shrink-0">
                    {action}
                    {onDismiss && (
                        <button
                            type="button"
                            onClick={onDismiss}
                            className="p-1 -mr-1 rounded-lg text-current/60 hover:text-current hover:bg-black/5 transition-colors cursor-pointer shrink-0"
                            aria-label={closeText}
                            title={closeText}
                        >
                            <X size={15} />
                        </button>
                    )}
                </div>
            )}
        </div>
    );
}
