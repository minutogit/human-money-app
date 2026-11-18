import React from 'react';
import { useTranslation } from 'react-i18next';
import { Search, X } from 'lucide-react';

export interface SearchFilterBarProps {
    /** Current search input value. */
    value: string;
    /** Called with the updated search string when typing or clearing. */
    onChange: (value: string) => void;
    /** Input placeholder text. */
    placeholder?: string;
    /** Accessible label for the search input. */
    ariaLabel?: string;
    /** Accessible label for the clear button. Defaults to `t('common.clear')`. */
    clearLabel?: string;
    /** Show a clear (X) button inside the input when value is non-empty. Defaults to true. */
    showClearButton?: boolean;
    /** Optional filter controls or actions rendered to the right of the search bar. */
    rightSlot?: React.ReactNode;
    /** Disable the search input and clear action. */
    disabled?: boolean;
    /** Optional CSS classes for the outer flex container. */
    className?: string;
    /** Optional CSS classes for the search <input> element. */
    inputClassName?: string;
    /** Optional test ID for the outer wrapper. */
    'data-testid'?: string;
    /** Optional test ID for the <input> element. */
    inputTestId?: string;
    /** Optional test ID for the clear button. */
    clearTestId?: string;
}

/**
 * Reusable search bar with leading search icon, optional clear button,
 * and an optional right-side filter/action slot.
 */
export const SearchFilterBar: React.FC<SearchFilterBarProps> = ({
    value,
    onChange,
    placeholder,
    ariaLabel,
    clearLabel,
    showClearButton = true,
    rightSlot,
    disabled = false,
    className = '',
    inputClassName = '',
    'data-testid': testId,
    inputTestId,
    clearTestId,
}) => {
    const { t } = useTranslation();
    const resolvedClearLabel = clearLabel ?? t('common.clear');
    const canClear = showClearButton && value.length > 0 && !disabled;

    return (
        <div
            data-testid={testId}
            className={`flex flex-col sm:flex-row items-stretch sm:items-center gap-3 ${className}`}
        >
            <div className="relative flex-1 group">
                <Search
                    size={18}
                    aria-hidden="true"
                    className="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 text-theme-light group-focus-within:text-theme-primary transition-colors"
                />
                <input
                    type="text"
                    value={value}
                    onChange={(e) => onChange(e.target.value)}
                    placeholder={placeholder}
                    aria-label={ariaLabel ?? placeholder}
                    disabled={disabled}
                    data-testid={inputTestId}
                    className={`w-full pl-11 ${
                        canClear ? 'pr-10' : 'pr-4'
                    } py-3 bg-white border border-theme-subtle rounded-2xl text-sm text-theme-secondary placeholder:text-theme-placeholder shadow-inner-soft focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:border-theme-primary disabled:opacity-60 disabled:cursor-not-allowed transition-all ${inputClassName}`}
                />
                {canClear && (
                    <button
                        type="button"
                        onClick={() => onChange('')}
                        aria-label={resolvedClearLabel}
                        title={resolvedClearLabel}
                        data-testid={clearTestId}
                        className="absolute right-2.5 top-1/2 -translate-y-1/2 p-1 rounded-full text-theme-light hover:text-theme-primary hover:bg-bg-app transition-colors cursor-pointer"
                    >
                        <X size={16} aria-hidden="true" />
                    </button>
                )}
            </div>

            {rightSlot && (
                <div className="flex items-center gap-2 shrink-0">
                    {rightSlot}
                </div>
            )}
        </div>
    );
};
