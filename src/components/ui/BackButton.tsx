import { ArrowLeft } from 'lucide-react';
import { useTranslation } from 'react-i18next';

export interface BackButtonProps {
    onClick: () => void;
    label?: string;
    title?: string;
    className?: string;
}

export function BackButton({ onClick, label, title, className = '' }: BackButtonProps) {
    const { t } = useTranslation();
    const resolvedTitle = title ?? t('common.back', 'Back');

    if (label) {
        return (
            <button
                type="button"
                onClick={onClick}
                title={resolvedTitle}
                aria-label={resolvedTitle}
                className={`inline-flex items-center gap-1.5 px-3.5 py-2 rounded-2xl bg-white/70 border border-theme-subtle hover:bg-bg-app text-xs font-bold text-theme-secondary transition-all shadow-sm active:scale-95 shrink-0 cursor-pointer ${className}`.trim()}
            >
                <ArrowLeft className="h-4 w-4 shrink-0" />
                <span>{label}</span>
            </button>
        );
    }

    return (
        <button
            type="button"
            onClick={onClick}
            title={resolvedTitle}
            aria-label={resolvedTitle}
            className={`p-2.5 rounded-full bg-white/70 border border-theme-subtle hover:bg-theme-primary hover:text-white transition-all text-theme-light shadow-sm active:scale-95 shrink-0 cursor-pointer ${className}`.trim()}
        >
            <ArrowLeft className="h-5 w-5" />
        </button>
    );
}

export default BackButton;
