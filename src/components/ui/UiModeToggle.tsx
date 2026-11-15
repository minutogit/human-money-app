// src/components/ui/UiModeToggle.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Sparkles, Wrench } from 'lucide-react';
import { useUiMode } from '../../context/UiModeContext';

export interface UiModeToggleProps {
    variant?: 'compact' | 'full';
    className?: string;
}

/**
 * Switches between Simple ("Einfach") and Expert ("Erweitert") UI modes.
 */
export const UiModeToggle: React.FC<UiModeToggleProps> = ({
    variant = 'compact',
    className = '',
}) => {
    const { t } = useTranslation();
    const { mode, setMode } = useUiMode();

    if (variant === 'full') {
        return (
            <div
                className={`grid grid-cols-1 sm:grid-cols-2 gap-3 ${className}`}
                data-testid="ui-mode-toggle-full"
                role="group"
                aria-label={t('settings.viewModeTitle')}
            >
                <button
                    type="button"
                    onClick={() => setMode('simple')}
                    aria-pressed={mode === 'simple'}
                    data-testid="ui-mode-card-simple"
                    className={`text-left p-4 rounded-2xl border transition-all cursor-pointer flex items-start gap-3 ${
                        mode === 'simple'
                            ? 'bg-theme-primary/10 border-theme-primary ring-2 ring-theme-primary/20 shadow-sm'
                            : 'bg-white dark:bg-slate-900 border-theme-subtle hover:border-theme-primary/40'
                    }`}
                >
                    <Sparkles
                        size={18}
                        className={`shrink-0 mt-0.5 ${
                            mode === 'simple' ? 'text-theme-primary' : 'text-theme-light'
                        }`}
                    />
                    <div className="min-w-0">
                        <div className="font-bold text-sm text-theme-secondary">
                            {t('common.uiModeSimple')}
                        </div>
                        <p className="text-xs text-theme-light mt-1 leading-relaxed">
                            {t('common.uiModeSimpleDesc')}
                        </p>
                    </div>
                </button>

                <button
                    type="button"
                    onClick={() => setMode('expert')}
                    aria-pressed={mode === 'expert'}
                    data-testid="ui-mode-card-expert"
                    className={`text-left p-4 rounded-2xl border transition-all cursor-pointer flex items-start gap-3 ${
                        mode === 'expert'
                            ? 'bg-theme-primary/10 border-theme-primary ring-2 ring-theme-primary/20 shadow-sm'
                            : 'bg-white dark:bg-slate-900 border-theme-subtle hover:border-theme-primary/40'
                    }`}
                >
                    <Wrench
                        size={18}
                        className={`shrink-0 mt-0.5 ${
                            mode === 'expert' ? 'text-theme-primary' : 'text-theme-light'
                        }`}
                    />
                    <div className="min-w-0">
                        <div className="font-bold text-sm text-theme-secondary">
                            {t('common.uiModeExpert')}
                        </div>
                        <p className="text-xs text-theme-light mt-1 leading-relaxed">
                            {t('common.uiModeExpertDesc')}
                        </p>
                    </div>
                </button>
            </div>
        );
    }

    return (
        <div
            className={`inline-flex items-center p-0.5 rounded-xl bg-theme-subtle/30 border border-theme-subtle/60 ${className}`}
            data-testid="ui-mode-toggle"
            role="group"
            aria-label={t('settings.viewModeTitle')}
        >
            <button
                type="button"
                onClick={() => setMode('simple')}
                aria-pressed={mode === 'simple'}
                data-testid="ui-mode-simple"
                className={`px-2.5 py-1 rounded-lg text-xs font-bold transition-all cursor-pointer ${
                    mode === 'simple'
                        ? 'bg-white dark:bg-slate-900 text-theme-primary shadow-sm'
                        : 'text-theme-light hover:text-theme-secondary'
                }`}
            >
                {t('common.uiModeSimple')}
            </button>
            <button
                type="button"
                onClick={() => setMode('expert')}
                aria-pressed={mode === 'expert'}
                data-testid="ui-mode-expert"
                className={`px-2.5 py-1 rounded-lg text-xs font-bold transition-all cursor-pointer ${
                    mode === 'expert'
                        ? 'bg-white dark:bg-slate-900 text-theme-primary shadow-sm'
                        : 'text-theme-light hover:text-theme-secondary'
                }`}
            >
                {t('common.uiModeExpert')}
            </button>
        </div>
    );
};
