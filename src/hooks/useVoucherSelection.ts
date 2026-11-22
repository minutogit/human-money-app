import { useState, useCallback, useMemo, useRef } from 'react';
import { VoucherSummary } from '../types';
import { transferService, TransferPlan } from '../services/transferService';
import {
  formatScaledBigInt,
  getDecimalFractionLength as getFracLength,
  parseDecimalToScaledBigInt,
  sumExactDecimalStrings,
} from '../utils/format';

export { parseDecimalToScaledBigInt };

const STRICT_POSITIVE_DECIMAL_RE = /^\d+(?:[.,]\d+)?$/;

export function parseStrictPositiveDecimal(
  raw: string,
  maxDecimals?: number,
): { normalized: string; value: number } | null {
  const trimmed = raw.trim();
  if (!STRICT_POSITIVE_DECIMAL_RE.test(trimmed)) return null;
  const normalized = trimmed.replace(',', '.');
  const [, fracPart = ''] = normalized.split('.');
  if (maxDecimals !== undefined && fracPart.length > maxDecimals) return null;
  if (/^0+(?:\.0+)?$/.test(normalized)) return null;
  const parsed = Number(normalized);
  if (!Number.isFinite(parsed) || parsed <= 0) return null;
  return { normalized, value: parsed };
}

export function useVoucherSelection(vouchers: VoucherSummary[]) {
  const [selectedMap, setSelectedMap] = useState<Map<string, string>>(new Map());
  const [currentPlan, setCurrentPlan] = useState<TransferPlan | null>(null);
  const [resolvedDraftKey, setResolvedDraftKey] = useState<string | null>(null);
  const latestRequestId = useRef(0);
  const vouchersRef = useRef(vouchers);
  vouchersRef.current = vouchers;

  const invalidatePreview = useCallback(() => {
    latestRequestId.current += 1;
    setResolvedDraftKey(null);
    setCurrentPlan(null);
  }, []);

  const toggleVoucher = useCallback((localInstanceId: string, amount: string) => {
    latestRequestId.current += 1;
    setResolvedDraftKey(null);
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
    latestRequestId.current += 1;
    setResolvedDraftKey(null);
    setCurrentPlan(null);
    const normalized = amount.replace(',', '.');
    setSelectedMap(prev => {
      if (!prev.has(localInstanceId)) return prev;
      const next = new Map(prev);
      next.set(localInstanceId, normalized);
      return next;
    });
  }, []);

  const selectAmount = useCallback((
    targetAmount: number,
    displayCurrency: string,
    precision: number = 4,
    isTest: boolean | null = null,
    standardUuid: string | null = null,
    draftKey?: string
  ) => {
    latestRequestId.current += 1;
    setCurrentPlan(null);
    setResolvedDraftKey(draftKey ?? null);
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
      const targetStr = targetAmount.toFixed(precision);
      const targetScaled = parseDecimalToScaledBigInt(targetStr, precision);
      let accumulatedScaled = 0n;
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

      if (targetScaled === null || targetScaled <= 0n) {
        return newSelection;
      }

      for (const v of relevantVouchers) {
        if (accumulatedScaled >= targetScaled) break;

        const remainingScaled = targetScaled - accumulatedScaled;
        const voucherScaled = parseDecimalToScaledBigInt(v.currentAmount, precision);
        if (voucherScaled === null || voucherScaled <= 0n) continue;

        if (v.allowPartialTransfers && voucherScaled > remainingScaled) {
          newSelection.set(v.localInstanceId, formatScaledBigInt(remainingScaled, precision));
          accumulatedScaled += remainingScaled;
        } else {
          newSelection.set(v.localInstanceId, v.currentAmount);
          accumulatedScaled += voucherScaled;
        }
      }

      return newSelection;
    });
  }, [vouchers]);

  const selectSingleVoucher = useCallback((localInstanceId: string, amount: string) => {
    latestRequestId.current += 1;
    setResolvedDraftKey(null);
    setCurrentPlan(null);
    setSelectedMap(new Map([[localInstanceId, amount]]));
  }, []);

  const clearSelection = useCallback(() => {
    latestRequestId.current += 1;
    setResolvedDraftKey(null);
    setSelectedMap(new Map());
    setCurrentPlan(null);
  }, []);

  const autoSelectGravitational = useCallback(async (
    targetAmountStr: string,
    recipientId: string,
    standardUuid: string | null = null,
    precision: number = 4,
    isTest: boolean | null = null,
    draftKey?: string
  ) => {
    const parsedTarget = parseStrictPositiveDecimal(targetAmountStr, precision);
    if (!parsedTarget) {
      return null;
    }
    const normalizedTarget = parsedTarget.normalized;
    const requestId = ++latestRequestId.current;
    try {
      const plan = await transferService.prepareTransferPlan({
        targetAmount: normalizedTarget,
        standardUuidFilter: standardUuid,
        recipientId: recipientId.trim(),
        isTestVoucher: isTest,
      });
      if (requestId !== latestRequestId.current) return null;
      if (!plan) {
        throw new Error('Transfer plan unavailable');
      }

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

      // Map plan to selectedMap using exact integer-scaled decimal subtraction for splits
      const next = new Map<string, string>();
      const splitScale = Math.max(
        precision,
        getFracLength(plan.target),
        ...plan.selected.map(rv => getFracLength(rv.amount)),
      );
      const targetScaled = parseDecimalToScaledBigInt(plan.target, splitScale);
      let accumulatedScaled = 0n;
      for (let i = 0; i < plan.selected.length; i++) {
        const rv = plan.selected[i];
        const isLast = i === plan.selected.length - 1;
        if (isLast && plan.needsSplit) {
          if (targetScaled === null) {
            throw new Error('Invalid target amount in transfer plan');
          }
          const remainingScaled = targetScaled - accumulatedScaled;
          if (remainingScaled <= 0n) {
            throw new Error('Invalid split calculation in transfer plan');
          }
          let remStr = formatScaledBigInt(remainingScaled, splitScale);
          if (splitScale > precision && remStr.includes('.')) {
            const [intPart, fracPart] = remStr.split('.');
            const trimmedExtra = fracPart.slice(precision).replace(/0+$/, '');
            const keptFrac = `${fracPart.slice(0, precision)}${trimmedExtra}`;
            remStr = keptFrac.length > 0 ? `${intPart}.${keptFrac}` : intPart;
          }
          next.set(rv.localInstanceId, remStr);
        } else {
          next.set(rv.localInstanceId, rv.amount);
          const amtScaled = parseDecimalToScaledBigInt(rv.amount, splitScale);
          if (amtScaled === null) {
            throw new Error('Invalid selected amount in transfer plan');
          }
          accumulatedScaled += amtScaled;
        }
      }
      setSelectedMap(next);
      setResolvedDraftKey(draftKey ?? null);
      return plan;
    } catch (e) {
      if (requestId !== latestRequestId.current) return null;
      // On failure keep previous selection but clear plan
      setCurrentPlan(null);
      throw e;
    }
  }, []);

  const selectionStats = useMemo(() => {
    const grouped: Record<string, { amounts: string[]; count: number }> = {};

    selectedMap.forEach((amount, id) => {
      const v = vouchers.find(v => v.localInstanceId === id);
      if (v) {
        const key = v.displayCurrency;
        if (!grouped[key]) {
          grouped[key] = { amounts: [], count: 0 };
        }
        grouped[key].amounts.push(String(amount));
        grouped[key].count += 1;
      }
    });

    const stats: Record<string, { totalExact: string; count: number }> = {};
    for (const [key, entry] of Object.entries(grouped)) {
      const { exact } = sumExactDecimalStrings(entry.amounts);
      stats[key] = {
        totalExact: exact,
        count: entry.count,
      };
    }

    return stats;
  }, [selectedMap, vouchers]);

  return {
    selectedMap,
    toggleVoucher,
    selectSingleVoucher,
    setVoucherAmount,
    selectAmount,
    clearSelection,
    invalidatePreview,
    resolvedDraftKey,
    selectionStats,
    currentPlan,
    autoSelectGravitational
  };
}
