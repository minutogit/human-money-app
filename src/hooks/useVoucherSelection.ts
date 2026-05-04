import { useState, useCallback, useMemo } from 'react';
import { VoucherSummary } from '../types';



export function useVoucherSelection(vouchers: VoucherSummary[]) {
  const [selectedMap, setSelectedMap] = useState<Map<string, string>>(new Map());

  const toggleVoucher = useCallback((localInstanceId: string, amount: string) => {
    setSelectedMap(prev => {
      const next = new Map(prev);
      if (next.has(localInstanceId)) {
        next.delete(localInstanceId);
      } else {
        next.set(localInstanceId, amount);
      }
      return next;
    });
  }, []);

  const selectAmount = useCallback((targetAmount: number, unit: string) => {
    const relevantVouchers = vouchers
      .filter(v => {
        const statusName = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
        return v.unit === unit && statusName.toLowerCase() === 'active';
      })
      .sort((a, b) => parseFloat(b.currentAmount) - parseFloat(a.currentAmount));

    let accumulated = 0;
    const newSelection = new Map(selectedMap);

    // Clear existing selection for this unit to avoid duplicates/confusion during auto-select
    for (const [id] of newSelection.entries()) {
      const v = vouchers.find(v => v.localInstanceId === id);
      if (v && v.unit === unit) {
        newSelection.delete(id);
      }
    }

    for (const v of relevantVouchers) {
      if (accumulated >= targetAmount) break;
      newSelection.set(v.localInstanceId, v.currentAmount);
      accumulated += parseFloat(v.currentAmount);
    }

    setSelectedMap(newSelection);
  }, [vouchers, selectedMap]);

  const clearSelection = useCallback(() => {
    setSelectedMap(new Map());
  }, []);

  const selectionStats = useMemo(() => {
    const stats: Record<string, { total: number, count: number }> = {};
    
    selectedMap.forEach((amount, id) => {
      const v = vouchers.find(v => v.localInstanceId === id);
      if (v) {
        if (!stats[v.unit]) {
          stats[v.unit] = { total: 0, count: 0 };
        }
        stats[v.unit].total += parseFloat(amount);
        stats[v.unit].count += 1;
      }
    });

    return stats;
  }, [selectedMap, vouchers]);

  return {
    selectedMap,
    toggleVoucher,
    selectAmount,
    clearSelection,
    selectionStats
  };
}
