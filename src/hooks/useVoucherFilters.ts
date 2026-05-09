// src/hooks/useVoucherFilters.ts
import { useState, useMemo } from 'react';
import { VoucherSummary } from '../types';

interface UseVoucherFiltersProps {
    vouchers: VoucherSummary[];
    initialStatusFilter?: string;
    initialStandardFilter?: string;
}

export function useVoucherFilters({ 
    vouchers, 
    initialStatusFilter, 
    initialStandardFilter 
}: UseVoucherFiltersProps) {
    const [statusFilters, setStatusFilters] = useState<string[]>(initialStatusFilter ? [initialStatusFilter] : []);
    const [standardFilters, setStandardFilters] = useState<string[]>(initialStandardFilter ? [initialStandardFilter] : []);

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

    const filteredVouchers = useMemo(() => (vouchers || []).filter(v => {
        if (!v) return false;
        const statusName = (typeof v.status === 'string' ? v.status : (Object.keys(v.status)[0] || 'unknown')).toLowerCase();
        const matchesStatus = statusFilters.length === 0 || statusFilters.includes(statusName);
        const matchesStandard = standardFilters.length === 0 || standardFilters.includes(v.displayStandardName || "");
        return matchesStatus && matchesStandard;
    }), [vouchers, statusFilters, standardFilters]);

    const activeFilterCount = statusFilters.length + standardFilters.length;

    const toggleStatusFilter = (status: string) => {
        setStatusFilters(prev => prev.includes(status) ? prev.filter(s => s !== status) : [...prev, status]);
    };

    const toggleStandardFilter = (standard: string) => {
        setStandardFilters(prev => prev.includes(standard) ? prev.filter(s => s !== standard) : [...prev, standard]);
    };

    const resetFilters = () => {
        setStatusFilters([]);
        setStandardFilters([]);
    };

    return {
        statusFilters,
        standardFilters,
        statusCounts,
        standardCounts,
        filteredVouchers,
        activeFilterCount,
        toggleStatusFilter,
        toggleStandardFilter,
        resetFilters
    };
}
