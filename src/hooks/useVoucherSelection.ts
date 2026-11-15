import { useState, useCallback, useMemo, useRef } from 'react';
import { VoucherSummary } from '../types';
import { transferService, TransferPlan } from '../services/transferService';

export function useVoucherSelection(vouchers: VoucherSummary[]) {
  const [selectedMap, setSelectedMap] = useState<Map<string, string>>(new Map());
  const [currentPlan, setCurrentPlan] = useState<TransferPlan | null>(null);
  const latestRequestId = useRef(0);
  const vouchersRef = useRef(vouchers);
  vouchersRef.current = vouchers;

  const toggleVoucher = useCallback((localInstanceId: string, amount: string) => {
    setCurrentPlan(null);
    const currentVouchers = vouchersRef.current;
    const targetVoucher = currentVouchers.find(v => v.localInstanceId === localInstanceId);
    setSelectedMap(prev => {
      const next = new Map(prev);
      if (next.has(localInstanceId)) {
        next.delete(localInstanceId);
      } else {
        if (targetVoucher) {
          const targetIsTest = Boolean(targetVoucher.isTestVoucher);
          for (const [id] of next.entries()) {
            const existing = currentVouchers.find(v => v.localInstanceId === id);
            if (existing && Boolean(existing.isTestVoucher) !== targetIsTest) {
              next.delete(id);
            }
          }
        }
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

  const selectAmount = useCallback((
    targetAmount: number,
    displayCurrency: string,
    precision: number = 4,
    isTest: boolean | null = null,
    standardUuid: string | null = null
  ) => {
    setCurrentPlan(null);
    const matchingActive = vouchers.filter(v => {
      const statusName = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
      if (statusName.toLowerCase() !== 'active') return false;
      if (standardUuid && v.voucherStandardUuid !== standardUuid) return false;
      return v.displayCurrency === displayCurrency;
    });

    const effectiveIsTest = isTest !== null
      ? Boolean(isTest)
      : matchingActive.some(v => !v.isTestVoucher)
        ? false
        : Boolean(matchingActive[0]?.isTestVoucher);

    const relevantVouchers = matchingActive
      .filter(v => Boolean(v.isTestVoucher) === effectiveIsTest)
      .sort((a, b) => {
        const dateA = a.validUntil || '9999-12-31';
        const dateB = b.validUntil || '9999-12-31';
        const dateCmp = dateA.localeCompare(dateB);
        if (dateCmp !== 0) return dateCmp;
        return parseFloat(b.currentAmount) - parseFloat(a.currentAmount);
      });

    setSelectedMap(prev => {
      let accumulated = 0;
      const newSelection = new Map(prev);

      // Clear opposite test/real realm entries and existing entries for this currency/standard
      for (const [id] of newSelection.entries()) {
        const v = vouchers.find(item => item.localInstanceId === id);
        if (!v) continue;
        const matchesSameClass = v.displayCurrency === displayCurrency && (!standardUuid || v.voucherStandardUuid === standardUuid);
        const oppositeTestRealm = Boolean(v.isTestVoucher) !== effectiveIsTest;
        if (matchesSameClass || oppositeTestRealm) {
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

      return newSelection;
    });
  }, [vouchers]);

  const clearSelection = useCallback(() => {
    setSelectedMap(new Map());
    setCurrentPlan(null);
  }, []);

  const autoSelectGravitational = useCallback(async (
    targetAmountStr: string,
    recipientId: string,
    standardUuid: string | null = null,
    precision: number = 4,
    isTest: boolean | null = null
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

      // Guardrail: never allow mixed Test + Real vouchers from transfer plan
      const selectedVouchers = plan.selected
        .map(rv => vouchersRef.current.find(v => v.localInstanceId === rv.localInstanceId))
        .filter((v): v is VoucherSummary => Boolean(v));
      if (selectedVouchers.length > 0) {
        const expectedIsTest = isTest !== null ? Boolean(isTest) : Boolean(selectedVouchers[0].isTestVoucher);
        const hasMismatch = selectedVouchers.some(v => Boolean(v.isTestVoucher) !== expectedIsTest);
        if (hasMismatch) {
          setCurrentPlan(null);
          throw new Error('Transfer plan mixed test and real vouchers');
        }
      }

      setCurrentPlan(plan);

      // Map plan to selectedMap with correct split amount handling
      const next = new Map<string, string>();
      let accumulated = 0;
      const targetNum = parseFloat(plan.target);
      for (let i = 0; i < plan.selected.length; i++) {
        const rv = plan.selected[i];
        const isLast = i === plan.selected.length - 1;
        if (isLast && plan.needsSplit) {
          const remaining = targetNum - accumulated;
          const remStr = remaining.toFixed(precision);
          next.set(rv.localInstanceId, remStr);
        } else {
          next.set(rv.localInstanceId, rv.amount);
          accumulated += parseFloat(rv.amount);
        }
      }
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
