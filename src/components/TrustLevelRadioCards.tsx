// src/components/TrustLevelRadioCards.tsx
import React from 'react';
import { useTranslation } from 'react-i18next';
import { Check, Info } from 'lucide-react';

export interface TrustLevelRadioCardsProps {
    value: number | null;
    onChange: (level: number | null) => void;
    allowUnrated?: boolean;
    allowedLevels?: number[];
    className?: string;
    idPrefix?: string;
}

interface TrustOption {
    level: number | null;
    labelKey: string;
    descKey: string;
    cardClass: string;
    activeCardClass: string;
    radioBorderClass: string;
    activeRadioBg: string;
}

export const TrustLevelRadioCards: React.FC<TrustLevelRadioCardsProps> = ({
    value,
    onChange,
    allowUnrated = true,
    allowedLevels,
    className = '',
    idPrefix = 'trust-radio',
}) => {
    const { t } = useTranslation();

    const allOptions: TrustOption[] = [
        ...(allowUnrated
            ? [
                  {
                      level: null,
                      labelKey: 'contacts.trustLevelUnrated',
                      descKey: 'contacts.trustLevelUnratedDesc',
                      cardClass: 'border-slate-200 hover:border-slate-300 bg-white',
                      activeCardClass: 'border-slate-400 bg-slate-50 text-slate-800 ring-2 ring-slate-400/20 shadow-sm',
                      radioBorderClass: 'border-slate-300',
                      activeRadioBg: 'bg-slate-700',
                  },
              ]
            : []),
        {
            level: 0,
            labelKey: 'contacts.trustLevel0',
            descKey: 'contacts.trustLevel0Desc',
            cardClass: 'border-sky-200 hover:border-sky-300 bg-white',
            activeCardClass: 'border-sky-500 bg-sky-50/70 text-sky-950 ring-2 ring-sky-400/20 shadow-sm',
            radioBorderClass: 'border-sky-300',
            activeRadioBg: 'bg-sky-600',
        },
        {
            level: 1,
            labelKey: 'contacts.trustLevel1',
            descKey: 'contacts.trustLevel1Desc',
            cardClass: 'border-amber-200 hover:border-amber-300 bg-white',
            activeCardClass: 'border-amber-500 bg-amber-50/70 text-amber-950 ring-2 ring-amber-400/20 shadow-sm',
            radioBorderClass: 'border-amber-300',
            activeRadioBg: 'bg-amber-600',
        },
        {
            level: 2,
            labelKey: 'contacts.trustLevel2',
            descKey: 'contacts.trustLevel2Desc',
            cardClass: 'border-teal-200 hover:border-teal-300 bg-white',
            activeCardClass: 'border-teal-500 bg-teal-50/70 text-teal-950 ring-2 ring-teal-400/20 shadow-sm',
            radioBorderClass: 'border-teal-300',
            activeRadioBg: 'bg-teal-600',
        },
        {
            level: 3,
            labelKey: 'contacts.trustLevel3',
            descKey: 'contacts.trustLevel3Desc',
            cardClass: 'border-emerald-300 hover:border-emerald-400 bg-white',
            activeCardClass: 'border-emerald-600 bg-emerald-50 text-emerald-950 ring-2 ring-emerald-500/20 shadow-sm',
            radioBorderClass: 'border-emerald-300',
            activeRadioBg: 'bg-emerald-600',
        },
    ];

    const options = allowedLevels
        ? allOptions.filter((opt) => opt.level === null ? allowUnrated : allowedLevels.includes(opt.level))
        : allOptions;

    return (
        <div className={`space-y-2.5 ${className}`} role="radiogroup" aria-label={t('contacts.trustSectionLabel')}>
            {options.map((opt) => {
                const isSelected = value === opt.level;
                const optionId = `${idPrefix}-${opt.level ?? 'unrated'}`;

                return (
                    <button
                        key={opt.level ?? 'unrated'}
                        id={optionId}
                        data-testid={`trust-level-${opt.level ?? 'unrated'}`}
                        type="button"
                        role="radio"
                        aria-checked={isSelected}
                        onClick={() => onChange(opt.level)}
                        className={`w-full text-left p-3.5 rounded-2xl border transition-all flex flex-col gap-1.5 cursor-pointer focus:outline-none focus:ring-2 focus:ring-theme-primary/30 ${
                            isSelected ? opt.activeCardClass : opt.cardClass
                        }`}
                    >
                        <div className="flex items-start justify-between gap-3">
                            <div className="flex items-center gap-2.5 min-w-0 flex-1">
                                <div
                                    className={`w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors ${
                                        isSelected
                                            ? `${opt.activeRadioBg} border-transparent`
                                            : `${opt.radioBorderClass} bg-white`
                                    }`}
                                    aria-hidden="true"
                                >
                                    {isSelected && <Check size={10} className="text-white stroke-[3]" />}
                                </div>
                                <span className="font-extrabold text-xs text-theme-primary tracking-tight">
                                    {t(opt.labelKey)}
                                </span>
                            </div>
                        </div>

                        <p className="text-[11px] text-theme-secondary/80 pl-6.5 font-normal leading-relaxed">
                            {t(opt.descKey)}
                        </p>

                        {isSelected && opt.level === 0 && (
                            <div
                                data-testid="trust-hint-level-0"
                                className="mt-1 ml-6.5 p-2.5 rounded-xl bg-sky-100/70 border border-sky-200 text-sky-900 text-[11px] leading-relaxed flex items-start gap-2 animate-in fade-in duration-150"
                            >
                                <Info size={14} className="text-sky-700 shrink-0 mt-0.5" />
                                <span>{t('contacts.trustLevel0SelectedHint')}</span>
                            </div>
                        )}

                        {isSelected && opt.level !== null && opt.level >= 1 && (
                            <div
                                data-testid="trust-hint-levels-1-to-3"
                                className="mt-1 ml-6.5 p-2.5 rounded-xl bg-amber-100/70 border border-amber-200 text-amber-950 text-[11px] leading-relaxed flex items-start gap-2 animate-in fade-in duration-150"
                            >
                                <Info size={14} className="text-amber-700 shrink-0 mt-0.5" />
                                <span>{t('contacts.trustLevels1To3Hint')}</span>
                            </div>
                        )}
                    </button>
                );
            })}
        </div>
    );
};

export default TrustLevelRadioCards;
