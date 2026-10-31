// src/components/voucher/VoucherFilterBar.tsx
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Filter, ChevronDown, History, X, Search, ArrowUpDown, Clock } from "lucide-react";
import { Card } from "../ui/Card";
import { VoucherOriginFilter, VoucherModeFilter, VoucherSortOption } from "../../hooks/useVoucherFilters";

interface VoucherFilterBarProps {
    statusFilters: string[];
    standardFilters: string[];
    searchQuery: string;
    setSearchQuery: (query: string) => void;
    originFilter: VoucherOriginFilter;
    setOriginFilter: (origin: VoucherOriginFilter) => void;
    modeFilter: VoucherModeFilter;
    setModeFilter: (mode: VoucherModeFilter) => void;
    expiringSoonOnly: boolean;
    setExpiringSoonOnly: (val: boolean | ((prev: boolean) => boolean)) => void;
    sortOption: VoucherSortOption;
    setSortOption: (sort: VoucherSortOption) => void;
    statusCounts: Record<string, number>;
    standardCounts: Record<string, number>;
    activeFilterCount: number;
    toggleStatusFilter: (status: string) => void;
    toggleStandardFilter: (standard: string) => void;
    resetFilters: () => void;
    availableStatuses: string[];
    availableStandards: string[];
}

export function VoucherFilterBar({
    statusFilters,
    standardFilters,
    searchQuery,
    setSearchQuery,
    originFilter,
    setOriginFilter,
    modeFilter,
    setModeFilter,
    expiringSoonOnly,
    setExpiringSoonOnly,
    sortOption,
    setSortOption,
    statusCounts,
    standardCounts,
    activeFilterCount,
    toggleStatusFilter,
    toggleStandardFilter,
    resetFilters,
    availableStatuses,
    availableStandards
}: VoucherFilterBarProps) {
    const { t } = useTranslation();
    const [isFiltersExpanded, setIsFiltersExpanded] = useState(false);

    const getStatusLabel = (status: string) => {
        switch (status.toLowerCase()) {
            case 'active':
                return t('voucher.statusActive');
            case 'archived':
                return t('voucher.statusArchived');
            case 'endorsed':
                return t('voucher.statusEndorsed');
            case 'expired':
                return t('voucher.statusExpired');
            case 'incomplete':
                return t('voucher.statusIncomplete');
            case 'quarantined':
                return t('voucher.statusQuarantined');
            default:
                return t('voucher.statusUnknown');
        }
    };

    return (
        <Card variant="glass" className="overflow-hidden border-none shadow-premium space-y-2 p-3 sm:p-4">
            {/* Top Bar: Search Input, Sort Select, Filter Toggle */}
            <div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-2 sm:gap-3">
                {/* Search Field */}
                <div className="relative flex-1">
                    <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-theme-light" />
                    <input
                        type="text"
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        placeholder={t('voucher.filter.searchPlaceholder')}
                        className="w-full pl-9 pr-8 py-2 rounded-xl bg-white/70 border border-theme-subtle/50 text-xs text-theme-secondary placeholder-theme-placeholder focus:outline-none focus:border-theme-primary transition-colors"
                    />
                    {searchQuery && (
                        <button
                            type="button"
                            onClick={() => setSearchQuery('')}
                            className="absolute right-2.5 top-1/2 -translate-y-1/2 text-theme-light hover:text-theme-secondary p-0.5 rounded-full"
                        >
                            <X size={14} />
                        </button>
                    )}
                </div>

                {/* Sort Option Dropdown */}
                <div className="relative flex items-center min-w-[180px]">
                    <ArrowUpDown size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-theme-light pointer-events-none" />
                    <select
                        value={sortOption}
                        onChange={(e) => setSortOption(e.target.value as VoucherSortOption)}
                        className="w-full appearance-none pl-8 pr-8 py-2 rounded-xl bg-white/70 border border-theme-subtle/50 text-xs font-bold text-theme-secondary focus:outline-none focus:border-theme-primary transition-colors cursor-pointer"
                    >
                        <option value="expiry_asc">{t('voucher.filter.sortExpiryAsc')}</option>
                        <option value="amount_desc">{t('voucher.filter.sortAmountDesc')}</option>
                        <option value="amount_asc">{t('voucher.filter.sortAmountAsc')}</option>
                        <option value="creator_asc">{t('voucher.filter.sortCreatorAsc')}</option>
                    </select>
                    <ChevronDown size={14} className="absolute right-3 top-1/2 -translate-y-1/2 text-theme-light pointer-events-none" />
                </div>

                {/* Filter Expand Button */}
                <button 
                    type="button"
                    onClick={() => setIsFiltersExpanded(!isFiltersExpanded)}
                    className={`flex items-center justify-center gap-2 px-4 py-2 rounded-xl border text-xs font-bold transition-all ${
                        isFiltersExpanded || activeFilterCount > 0
                            ? 'bg-theme-primary text-white border-theme-primary shadow-sm'
                            : 'bg-white/70 text-theme-secondary border-theme-subtle/50 hover:bg-white'
                    }`}
                >
                    <Filter size={14} />
                    <span>{t('voucher.filter.filters')}</span>
                    {activeFilterCount > 0 && (
                        <span className={`text-[10px] px-1.5 py-0.2 rounded-full font-bold ${
                            isFiltersExpanded || activeFilterCount > 0 ? 'bg-white text-theme-primary' : 'bg-theme-primary text-white'
                        }`}>
                            {activeFilterCount}
                        </span>
                    )}
                    <ChevronDown size={14} className={`transition-transform duration-300 ${isFiltersExpanded ? 'rotate-180' : ''}`} />
                </button>
            </div>

            {/* Expandable Filter Panel */}
            <div className={`transition-all duration-300 ease-in-out ${isFiltersExpanded ? 'max-h-[800px] pt-3 opacity-100' : 'max-h-0 opacity-0 overflow-hidden'}`}>
                <div className="space-y-4 pt-3 border-t border-theme-subtle/30">
                    {/* Origin & Mode & Validity Quick Chips */}
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                        {/* Origin Filter */}
                        <div className="space-y-1.5">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">
                                {t('voucher.filter.byOrigin')}
                            </label>
                            <div className="flex gap-1.5">
                                {(['all', 'received', 'own'] as const).map((orig) => (
                                    <button
                                        key={orig}
                                        type="button"
                                        onClick={() => setOriginFilter(orig)}
                                        className={`flex-1 py-1.5 px-2 rounded-lg text-xs font-bold transition-all border text-center ${
                                            originFilter === orig
                                                ? 'bg-theme-primary text-white border-theme-primary shadow-xs'
                                                : 'bg-white/80 text-theme-secondary border-theme-subtle/50 hover:bg-white'
                                        }`}
                                    >
                                        {orig === 'all' && t('voucher.filter.originAll')}
                                        {orig === 'received' && t('voucher.filter.originReceived')}
                                        {orig === 'own' && t('voucher.filter.originOwn')}
                                    </button>
                                ))}
                            </div>
                        </div>

                        {/* Mode Filter */}
                        <div className="space-y-1.5">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">
                                {t('voucher.filter.byMode')}
                            </label>
                            <div className="flex gap-1.5">
                                {(['all', 'real', 'test'] as const).map((mode) => (
                                    <button
                                        key={mode}
                                        type="button"
                                        onClick={() => setModeFilter(mode)}
                                        className={`flex-1 py-1.5 px-2 rounded-lg text-xs font-bold transition-all border text-center ${
                                            modeFilter === mode
                                                ? 'bg-theme-primary text-white border-theme-primary shadow-xs'
                                                : 'bg-white/80 text-theme-secondary border-theme-subtle/50 hover:bg-white'
                                        }`}
                                    >
                                        {mode === 'all' && t('voucher.filter.modeAll')}
                                        {mode === 'real' && t('voucher.filter.modeReal')}
                                        {mode === 'test' && t('voucher.filter.modeTest')}
                                    </button>
                                ))}
                            </div>
                        </div>

                        {/* Validity Quick Chip */}
                        <div className="space-y-1.5">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">
                                {t('voucher.filter.byValidity')}
                            </label>
                            <button
                                type="button"
                                onClick={() => setExpiringSoonOnly(!expiringSoonOnly)}
                                className={`w-full py-1.5 px-3 rounded-lg text-xs font-bold transition-all border flex items-center justify-center gap-1.5 ${
                                    expiringSoonOnly
                                        ? 'bg-amber-500 text-white border-amber-600 shadow-xs'
                                        : 'bg-white/80 text-theme-secondary border-theme-subtle/50 hover:bg-white'
                                }`}
                            >
                                <Clock size={13} />
                                <span>{t('voucher.filter.expiringSoon')}</span>
                            </button>
                        </div>
                    </div>

                    {/* Status Filter */}
                    <div className="space-y-1.5">
                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">
                            {t('voucher.filter.byStatus')}
                        </label>
                        <div className="flex flex-wrap gap-1.5">
                            {availableStatuses.map(status => {
                                const isActive = statusFilters.includes(status);
                                return (
                                    <button
                                        key={status}
                                        type="button"
                                        onClick={() => toggleStatusFilter(status)}
                                        className={`px-3 py-1.5 rounded-xl text-xs font-bold transition-all border flex items-center gap-1.5 ${
                                            isActive 
                                            ? 'bg-theme-primary text-white border-theme-primary shadow-xs' 
                                            : 'bg-white text-theme-secondary border-theme-subtle/50 hover:border-theme-primary hover:bg-white shadow-xs'
                                        }`}
                                    >
                                        <History size={12} className={isActive ? 'text-white' : 'text-theme-light'} />
                                        <span>{getStatusLabel(status)}</span>
                                        <span className={`text-[10px] px-1.5 py-0.2 rounded-md ${isActive ? 'bg-white/20 text-white' : 'bg-theme-subtle/30 text-theme-light'}`}>
                                            {statusCounts[status] || 0}
                                        </span>
                                    </button>
                                );
                            })}
                        </div>
                    </div>

                    {/* Standard Filter */}
                    {availableStandards.length > 0 && (
                        <div className="space-y-1.5">
                            <label className="text-[10px] font-black text-theme-light uppercase tracking-widest px-1">
                                {t('voucher.filter.byStandard')}
                            </label>
                            <div className="flex flex-wrap gap-1.5">
                                {availableStandards.map(standard => {
                                    const isActive = standardFilters.includes(standard);
                                    return (
                                        <button
                                            key={standard}
                                            type="button"
                                            onClick={() => toggleStandardFilter(standard)}
                                            className={`px-3 py-1.5 rounded-xl text-xs font-bold transition-all border flex items-center gap-1.5 ${
                                                isActive 
                                                ? 'bg-theme-accent text-white border-theme-accent shadow-xs' 
                                                : 'bg-white text-theme-secondary border-theme-subtle/50 hover:border-theme-accent hover:bg-white shadow-xs'
                                            }`}
                                        >
                                            <span className="capitalize">{standard}</span>
                                            <span className={`text-[10px] px-1.5 py-0.2 rounded-md ${isActive ? 'bg-white/20 text-white' : 'bg-theme-subtle/30 text-theme-light'}`}>
                                                {standardCounts[standard] || 0}
                                            </span>
                                        </button>
                                    );
                                })}
                            </div>
                        </div>
                    )}
                    
                    {activeFilterCount > 0 && (
                        <div className="flex justify-end pt-1">
                            <button 
                                type="button"
                                onClick={resetFilters}
                                className="text-[10px] font-black text-rose-500 uppercase tracking-widest flex items-center gap-1.5 hover:underline px-2 py-1"
                            >
                                <X size={12} />
                                {t('voucher.filter.reset')}
                            </button>
                        </div>
                    )}
                </div>
            </div>
        </Card>
    );
}
