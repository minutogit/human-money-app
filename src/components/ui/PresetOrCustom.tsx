// src/components/ui/PresetOrCustom.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { SlidersHorizontal } from 'lucide-react';
import { useUiMode } from '../../context/UiModeContext';

export interface PresetOption<T extends string> {
    id: T;
    title: React.ReactNode;
    description?: React.ReactNode;
    badge?: React.ReactNode;
    icon?: React.ReactNode;
    recommended?: boolean;
}

export interface PresetOrCustomProps<T extends string> {
    presets: Array<PresetOption<T>>;
    value: T | 'custom';
    onChange: (presetId: T) => void;
    renderCustom?: () => React.ReactNode;
    customLabel?: React.ReactNode;
    className?: string;
    gridClassName?: string;
}

/**
 * Shows easy-to-scan preset cards in both modes.
 * If the current backend/settings combination differs from all presets, displays a "Benutzerdefiniert" / "Custom" indicator.
 * In Expert mode, also renders granular custom controls below via `renderCustom`.
 */
export function PresetOrCustom<T extends string>({
    presets,
    value,
    onChange,
    renderCustom,
    customLabel,
    className = '',
    gridClassName = 'grid grid-cols-1 sm:grid-cols-3 gap-3',
}: PresetOrCustomProps<T>) {
    const { t } = useTranslation();
    const { isExpert } = useUiMode();

    return (
        <div className={`space-y-4 ${className}`} data-testid="preset-or-custom">
            {value === 'custom' && (
                <div
                    className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-bold bg-amber-50 text-amber-800 border border-amber-200"
                    data-testid="preset-custom-status"
                >
                    <SlidersHorizontal size={13} className="shrink-0" />
                    <span>{customLabel ?? t('common.customPreset')}</span>
                </div>
            )}

            <div className={gridClassName}>
                {presets.map((preset) => {
                    const isSelected = value === preset.id;
                    return (
                        <button
                            key={preset.id}
                            type="button"
                            onClick={() => onChange(preset.id)}
                            aria-pressed={isSelected}
                            data-testid={`preset-option-${preset.id}`}
                            className={`text-left p-4 rounded-2xl border transition-all cursor-pointer flex flex-col gap-1.5 ${
                                isSelected
                                    ? 'bg-theme-primary/10 border-theme-primary shadow-sm ring-2 ring-theme-primary/20'
                                    : 'bg-white dark:bg-slate-900 border-theme-subtle hover:border-theme-primary/40 hover:bg-bg-app/50'
                            }`}
                        >
                            <div className="flex items-center justify-between gap-2 w-full">
                                <div className="flex items-center gap-2 min-w-0">
                                    {preset.icon && (
                                        <span className="shrink-0">{preset.icon}</span>
                                    )}
                                    <span className="font-bold text-sm text-theme-secondary truncate">
                                        {preset.title}
                                    </span>
                                </div>
                                {preset.badge ?? (preset.recommended && (
                                    <span className="shrink-0 px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-500/10 text-emerald-700 border border-emerald-500/20">
                                        {t('common.recommended')}
                                    </span>
                                ))}
                            </div>
                            {preset.description && (
                                <p className="text-xs text-theme-light leading-relaxed">
                                    {preset.description}
                                </p>
                            )}
                        </button>
                    );
                })}
            </div>

            {isExpert && renderCustom && (
                <div
                    className="pt-3 border-t border-theme-subtle/60"
                    data-testid="preset-custom-controls"
                >
                    {renderCustom()}
                </div>
            )}
        </div>
    );
}
