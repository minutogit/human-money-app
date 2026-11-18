// src/components/transfer/HistoryFilterBar.tsx
import { useTranslation } from 'react-i18next';
import { Calendar, Filter } from 'lucide-react';
import { SearchFilterBar } from '../ui/SearchFilterBar';
import { AdvancedDisclosure } from '../ui/AdvancedDisclosure';
import { formatAmount } from '../../utils/format';
import type { TransactionHistorySummary } from '../../utils/walletEvents';

export type DirectionFilter = 'all' | 'received' | 'sent';
export type PeriodFilter = 'all' | 'last30Days' | 'thisYear';

interface HistoryFilterBarProps {
    searchQuery: string;
    onSearchChange: (value: string) => void;
    directionFilter: DirectionFilter;
    onDirectionChange: (value: DirectionFilter) => void;
    currencyFilter: string;
    onCurrencyChange: (value: string) => void;
    periodFilter: PeriodFilter;
    onPeriodChange: (value: PeriodFilter) => void;
    availableCurrencies: string[];
    summaryData: TransactionHistorySummary;
}

export function HistoryFilterBar({
    searchQuery,
    onSearchChange,
    directionFilter,
    onDirectionChange,
    currencyFilter,
    onCurrencyChange,
    periodFilter,
    onPeriodChange,
    availableCurrencies,
    summaryData,
}: HistoryFilterBarProps) {
    const { t } = useTranslation();

    return (
        <>
            <div className="bg-white/70 backdrop-blur-md border border-theme-subtle rounded-2xl p-4 shadow-sm space-y-3">
                <SearchFilterBar
                    value={searchQuery}
                    onChange={onSearchChange}
                    placeholder={t('history.searchPlaceholder')}
                    inputClassName="py-2.5 bg-bg-app/50 rounded-xl font-medium"
                />

                <div className="flex flex-wrap items-center justify-between gap-3 pt-1 border-t border-theme-subtle/50 text-xs">
                    <div className="flex items-center gap-1 bg-bg-app p-1 rounded-xl border border-theme-subtle/60">
                        <button
                            type="button"
                            onClick={() => onDirectionChange('all')}
                            className={`px-2.5 py-1 rounded-lg font-bold transition-all ${
                                directionFilter === 'all'
                                    ? 'bg-white text-theme-secondary shadow-xs'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            {t('history.filterAllDirections')}
                        </button>
                        <button
                            type="button"
                            onClick={() => onDirectionChange('received')}
                            className={`px-2.5 py-1 rounded-lg font-bold transition-all ${
                                directionFilter === 'received'
                                    ? 'bg-emerald-500 text-white shadow-xs'
                                    : 'text-emerald-600 hover:text-emerald-700'
                            }`}
                        >
                            {t('history.filterInbound')}
                        </button>
                        <button
                            type="button"
                            onClick={() => onDirectionChange('sent')}
                            className={`px-2.5 py-1 rounded-lg font-bold transition-all ${
                                directionFilter === 'sent'
                                    ? 'bg-slate-700 text-white shadow-xs'
                                    : 'text-theme-light hover:text-theme-secondary'
                            }`}
                        >
                            {t('history.filterOutbound')}
                        </button>
                    </div>
                </div>

                <AdvancedDisclosure
                    title={t('common.moreOptions')}
                    isCustom={currencyFilter !== 'all' || periodFilter !== 'all'}
                    toggleTestId="history-filters-toggle"
                    contentTestId="history-filters-content"
                    className="border-t border-theme-subtle/50 pt-2"
                    buttonClassName="w-full flex items-center justify-between gap-2 py-1 px-1.5 rounded-lg text-xs font-bold text-theme-secondary hover:bg-bg-app/60 transition-colors text-left cursor-pointer"
                    contentClassName="pt-2 flex flex-wrap items-center gap-2 text-xs"
                >
                    {availableCurrencies.length > 1 && (
                        <div className="flex items-center gap-1.5 bg-bg-app px-2 py-1 rounded-xl border border-theme-subtle/60">
                            <Filter size={12} className="text-theme-light" />
                            <select
                                value={currencyFilter}
                                onChange={(e) => onCurrencyChange(e.target.value)}
                                className="bg-transparent font-bold text-theme-secondary focus:outline-none cursor-pointer pr-1"
                            >
                                <option value="all">{t('history.filterAllCurrencies')}</option>
                                {availableCurrencies.map((c) => (
                                    <option key={c} value={c}>
                                        {c}
                                    </option>
                                ))}
                            </select>
                        </div>
                    )}

                    <div className="flex items-center gap-1.5 bg-bg-app px-2 py-1 rounded-xl border border-theme-subtle/60">
                        <Calendar size={12} className="text-theme-light" />
                        <select
                            value={periodFilter}
                            onChange={(e) => onPeriodChange(e.target.value as PeriodFilter)}
                            className="bg-transparent font-bold text-theme-secondary focus:outline-none cursor-pointer pr-1"
                        >
                            <option value="all">{t('history.periodAll')}</option>
                            <option value="last30Days">{t('history.periodLast30Days')}</option>
                            <option value="thisYear">{t('history.periodThisYear')}</option>
                        </select>
                    </div>
                </AdvancedDisclosure>
            </div>

            <div className="flex flex-wrap items-center justify-between gap-3 px-4 py-2.5 bg-white/50 backdrop-blur-sm border border-theme-subtle/60 rounded-xl text-xs text-theme-secondary shadow-xs">
                <div className="flex items-center gap-2 font-medium">
                    <span className="font-bold text-theme-primary">
                        {t('history.summaryBookings', { count: summaryData.totalBookings })}
                    </span>
                </div>

                <div className="flex flex-wrap items-center gap-3 font-mono">
                    {currencyFilter !== 'all' ? (
                        <>
                            <span className="text-emerald-600 font-bold">
                                +{formatAmount(summaryData.inboundSums[currencyFilter] || 0)}{' '}
                                {currencyFilter}
                            </span>
                            <span className="text-theme-light opacity-40">|</span>
                            <span className="text-slate-700 font-bold">
                                -{formatAmount(summaryData.outboundSums[currencyFilter] || 0)}{' '}
                                {currencyFilter}
                            </span>
                        </>
                    ) : summaryData.summaryUnits.length > 0 ? (
                        <div className="flex flex-wrap items-center gap-3">
                            {summaryData.summaryUnits.map((unit) => {
                                const inVal = summaryData.inboundSums[unit] || 0;
                                const outVal = summaryData.outboundSums[unit] || 0;
                                return (
                                    <span
                                        key={unit}
                                        className="text-[11px] font-semibold text-theme-light"
                                    >
                                        <span className="text-emerald-600 font-bold">
                                            +{formatAmount(inVal)}
                                        </span>
                                        {' / '}
                                        <span className="text-slate-700 font-bold">
                                            -{formatAmount(outVal)}
                                        </span>{' '}
                                        {unit}
                                    </span>
                                );
                            })}
                        </div>
                    ) : null}
                </div>
            </div>
        </>
    );
}
