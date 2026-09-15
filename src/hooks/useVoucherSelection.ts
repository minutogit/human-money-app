import { useState, useCallback, useMemo, useRef } from 'react';
import { VoucherSummary } from '../types';
import { transferService, TransferPlan } from '../services/transferService';

export function useVoucherSelection(vouchers: VoucherSummary[]) {
  const [selectedMap, setSelectedMap] = useState<Map<string, string>>(new Map());
  const [currentPlan, setCurrentPlan] = useState<TransferPlan | null>(null);
  const latestRequestId = useRef(0);

  const toggleVoucher = useCallback((localInstanceId: string, amount: string) => {
    setCurrentPlan(null);
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

  const setVoucherAmount = useCallback((localInstanceId: string, amount: string) => {
    setCurrentPlan(null);
    setSelectedMap(prev => {
      if (!prev.has(localInstanceId)) return prev;
      const next = new Map(prev);
      next.set(localInstanceId, amount);
      return next;
    });
  }, []);

  const selectAmount = useCallback((targetAmount: number, displayCurrency: string, precision: number = 4) => {
    setCurrentPlan(null);
    const relevantVouchers = vouchers
      .filter(v => {
        const statusName = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
        return v.displayCurrency === displayCurrency && statusName.toLowerCase() === 'active';
      })
      .sort((a, b) => parseFloat(b.currentAmount) - parseFloat(a.currentAmount));

    let accumulated = 0;
    const newSelection = new Map(selectedMap);

    // Clear existing selection for this asset type to avoid duplicates/confusion during auto-select
    for (const [id] of newSelection.entries()) {
      const v = vouchers.find(v => v.localInstanceId === id);
      if (v && v.displayCurrency === displayCurrency) {
        newSelection.delete(id);
      }
    }

    for (const v of relevantVouchers) {
      if (accumulated >= targetAmount) break;
      
      const remainingNeeded = targetAmount - accumulated;
      const voucherAmount = parseFloat(v.currentAmount);

      if (v.allowPartialTransfers && voucherAmount > remainingNeeded) {
        // Take only what's needed
        newSelection.set(v.localInstanceId, remainingNeeded.toFixed(precision));
        accumulated += remainingNeeded;
      } else {
        // Take full voucher
        newSelection.set(v.localInstanceId, v.currentAmount);
        accumulated += voucherAmount;
      }
    }

    setSelectedMap(newSelection);
  }, [vouchers, selectedMap]);

  const clearSelection = useCallback(() => {
    setSelectedMap(new Map());
    setCurrentPlan(null);
  }, []);

  const autoSelectGravitational = useCallback(async (
    targetAmountStr: string,
    recipientId: string,
    standardUuid: string | null = null,
    precision: number = 4
  ) => {
    const target = parseFloat(targetAmountStr);
    if (!target || target <= 0 || !recipientId || recipientId.trim().length < 10) {
      return null;
    }
    const requestId = ++latestRequestId.current;
    try {
      const plan = await transferService.prepareTransferPlan({
        targetAmount: targetAmountStr,
        standardUuidFilter: standardUuid,
        recipientId: recipientId.trim()
      });
      if (requestId !== latestRequestId.current) return null;
      setCurrentPlan(plan);

      // Map plan to selectedMap with correct split amount handling
      const next = new Map<string, string>();
      // Preserve selections of other asset classes not in plan? Replace only relevant class.
      // For simplicity, replace entirely with plan selection (gravitational is full plan).
      // But keep unrelated currencies if desired — here we replace all.
      // To keep other currencies, we filter out currencies of plan first.
      // For now, clear and set plan.
      let accumulated = 0;
      const targetNum = parseFloat(plan.target);
      for (let i = 0; i < plan.selected.length; i++) {
        const rv = plan.selected[i];
        const isLast = i === plan.selected.length - 1;
        if (isLast && plan.needsSplit) {
          const remaining = targetNum - accumulated;
          // Ensure remaining is positive and formatted with precision
          const remStr = remaining.toFixed(precision);
          next.set(rv.localInstanceId, remStr);
        } else {
          next.set(rv.localInstanceId, rv.amount);
          accumulated += parseFloat(rv.amount);
          // For split case we already handled accumulated before, but for non-split we track
          // If needsSplit, accumulated for earlier items still needed for remaining calc
        }
        // For needsSplit, we need accumulated of non-last items only; we already tracked
        if (!isLast || !plan.needsSplit) {
          // accumulated already updated for non-last; for last non-split already added
        }
      }
      // If needsSplit we didn't add last full amount to accumulated before calc, but we used prior accumulated correctly
      setSelectedMap(next);
      return plan;
    } catch (e) {
      if (requestId !== latestRequestId.current) return null;
      // On failure keep previous selection but clear plan
      setCurrentPlan(null);
      throw e;
    }
  }, []);

  const selectionStats = useMemo(() => {
    const stats: Record<string, { total: number, count: number }> = {};
    
    selectedMap.forEach((amount, id) => {
      const v = vouchers.find(v => v.localInstanceId === id);
      if (v) {
        const key = v.displayCurrency;
        if (!stats[key]) {
          stats[key] = { total: 0, count: 0 };
        }
        stats[key].total += parseFloat(amount);
        stats[key].count += 1;
      }
    });

    return stats;
  }, [selectedMap, vouchers]);

  return {
    selectedMap,
    toggleVoucher,
    setVoucherAmount,
    selectAmount,
    clearSelection,
    selectionStats,
    currentPlan,
    autoSelectGravitational
  };
}
