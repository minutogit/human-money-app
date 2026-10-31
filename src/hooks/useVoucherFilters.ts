// src/hooks/useVoucherFilters.ts
import { useState, useMemo, useCallback } from 'react';
import { VoucherSummary } from '../types';

export type VoucherOriginFilter = 'all' | 'received' | 'own';
export type VoucherModeFilter = 'all' | 'real' | 'test';
export type VoucherSortOption = 'expiry_asc' | 'amount_desc' | 'amount_asc' | 'creator_asc';

interface UseVoucherFiltersProps {
    vouchers: VoucherSummary[];
    ownUserId?: string;
    initialStatusFilter?: string;
    initialStandardFilter?: string;
    initialFilter?: 'all' | 'received' | 'own' | 'expiringSoon';
}

export function useVoucherFilters({ 
    vouchers, 
    ownUserId,
    initialStatusFilter, 
    initialStandardFilter,
    initialFilter
}: UseVoucherFiltersProps) {
    const [statusFilters, setStatusFilters] = useState<string[]>(initialStatusFilter ? [initialStatusFilter] : []);
    const [standardFilters, setStandardFilters] = useState<string[]>(initialStandardFilter ? [initialStandardFilter] : []);
    const [searchQuery, setSearchQuery] = useState<string>('');
    const [originFilter, setOriginFilter] = useState<VoucherOriginFilter>(
        initialFilter === 'received' ? 'received' : initialFilter === 'own' ? 'own' : 'all'
    );
    const [modeFilter, setModeFilter] = useState<VoucherModeFilter>('all');
    const [expiringSoonOnly, setExpiringSoonOnly] = useState<boolean>(initialFilter === 'expiringSoon');
    const [sortOption, setSortOption] = useState<VoucherSortOption>('expiry_asc');

    const statusCounts = useMemo(() => (vouchers || []).reduce((acc, v) => {
        if (!v) return acc;
        const s = (typeof v.status === 'string' ? v.status : (Object.keys(v.status)[0] || 'unknown')).toLowerCase();
        acc[s] = (acc[s] || 0) + 1;
        return acc;
    }, {} as Record<string, number>), [vouchers]);

    const standardCounts = useMemo(() => (vouchers || []).reduce((acc, v) => {
        if (!v || !v.displayStandardName) return acc;
        acc[v.displayStandardName] = (acc[v.displayStandardName] || 0) + 1;
        return acc;
    }, {} as Record<string, number>), [vouchers]);

    const [referenceTime] = useState(() => Date.now());

    const isExpiringWithin30Days = useCallback((validUntil?: string) => {
        if (!validUntil) return false;
        const expiry = new Date(validUntil).getTime();
        if (isNaN(expiry)) return false;
        const thirtyDaysMs = 30 * 24 * 60 * 60 * 1000;
        return expiry > referenceTime && expiry <= referenceTime + thirtyDaysMs;
    }, [referenceTime]);

    const filteredVouchers = useMemo(() => {
        const query = searchQuery.trim().toLowerCase();

        const filtered = (vouchers || []).filter(v => {
            if (!v) return false;

            // 1. Status Filter
            const statusName = (typeof v.status === 'string' ? v.status : (Object.keys(v.status)[0] || 'unknown')).toLowerCase();
            if (statusFilters.length > 0 && !statusFilters.includes(statusName)) {
                return false;
            }

            // 2. Standard Filter
            if (standardFilters.length > 0 && !standardFilters.includes(v.displayStandardName || "")) {
                return false;
            }

            // 3. Origin Filter (Received vs Own)
            if (originFilter === 'own') {
                if (!ownUserId || v.creatorId !== ownUserId) return false;
            } else if (originFilter === 'received') {
                if (ownUserId && v.creatorId === ownUserId) return false;
            }

            // 4. Mode Filter (Real vs Test)
            if (modeFilter === 'real' && v.isTestVoucher) return false;
            if (modeFilter === 'test' && !v.isTestVoucher) return false;

            // 5. Expiring soon filter (< 30 days)
            if (expiringSoonOnly) {
                if (statusName !== 'active') return false;
                if (!isExpiringWithin30Days(v.validUntil)) return false;
            }

            // 6. Search Query
            if (query) {
                const fullName = `${v.creatorFirstName || ''} ${v.creatorLastName || ''}`.toLowerCase();
                const matchesName = fullName.includes(query);
                const matchesStandard = (v.displayStandardName || '').toLowerCase().includes(query) ||
                                        (v.rawStandardName || '').toLowerCase().includes(query);
                const matchesCurrency = (v.displayCurrency || '').toLowerCase().includes(query) ||
                                        (v.unit || '').toLowerCase().includes(query);
                const matchesId = (v.localInstanceId || '').toLowerCase().includes(query) ||
                                  (v.creatorId || '').toLowerCase().includes(query);

                if (!matchesName && !matchesStandard && !matchesCurrency && !matchesId) {
                    return false;
                }
            }

            return true;
        });

        // Sort vouchers
        return [...filtered].sort((a, b) => {
            switch (sortOption) {
                case 'expiry_asc': {
                    const timeA = a.validUntil ? new Date(a.validUntil).getTime() : Number.MAX_SAFE_INTEGER;
                    const timeB = b.validUntil ? new Date(b.validUntil).getTime() : Number.MAX_SAFE_INTEGER;
                    return timeA - timeB;
                }
                case 'amount_desc': {
                    const amtA = parseFloat(a.currentAmount || "0");
                    const amtB = parseFloat(b.currentAmount || "0");
                    return amtB - amtA;
                }
                case 'amount_asc': {
                    const amtA = parseFloat(a.currentAmount || "0");
                    const amtB = parseFloat(b.currentAmount || "0");
                    return amtA - amtB;
                }
                case 'creator_asc': {
                    const nameA = `${a.creatorFirstName || ''} ${a.creatorLastName || ''}`.trim() || a.creatorId || '';
                    const nameB = `${b.creatorFirstName || ''} ${b.creatorLastName || ''}`.trim() || b.creatorId || '';
                    return nameA.localeCompare(nameB);
                }
                default:
                    return 0;
            }
        });
    }, [vouchers, statusFilters, standardFilters, originFilter, modeFilter, expiringSoonOnly, searchQuery, sortOption, ownUserId, isExpiringWithin30Days]);

    const activeFilterCount = statusFilters.length + 
        standardFilters.length + 
        (searchQuery ? 1 : 0) + 
        (originFilter !== 'all' ? 1 : 0) + 
        (modeFilter !== 'all' ? 1 : 0) + 
        (expiringSoonOnly ? 1 : 0);

    const toggleStatusFilter = (status: string) => {
        setStatusFilters(prev => prev.includes(status) ? prev.filter(s => s !== status) : [...prev, status]);
    };

    const toggleStandardFilter = (standard: string) => {
        setStandardFilters(prev => prev.includes(standard) ? prev.filter(s => s !== standard) : [...prev, standard]);
    };

    const resetFilters = () => {
        setStatusFilters([]);
        setStandardFilters([]);
        setSearchQuery('');
        setOriginFilter('all');
        setModeFilter('all');
        setExpiringSoonOnly(false);
        setSortOption('expiry_asc');
    };

    return {
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
        filteredVouchers,
        activeFilterCount,
        toggleStatusFilter,
        toggleStandardFilter,
        resetFilters
    };
}
