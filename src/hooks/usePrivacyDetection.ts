import { useMemo } from 'react';
import { VoucherSummary, VoucherStandardDefinition } from '../types';

export type DetectedPrivacyMode = 'stealth' | 'public' | 'flexible' | 'incompatible';

export function usePrivacyDetection(
  selectedIds: string[],
  vouchers: VoucherSummary[],
  standards: Record<string, VoucherStandardDefinition>
): DetectedPrivacyMode {
  return useMemo(() => {
    if (selectedIds.length === 0) return 'flexible';

    const selectedVouchers = selectedIds
      .map(id => vouchers.find(v => v.localInstanceId === id))
      .filter((v): v is VoucherSummary => !!v);

    const requiredModes = new Set<string>();

    selectedVouchers.forEach(v => {
      const standard = standards[v.voucherStandardUuid];
      if (standard?.immutable.features.privacyMode) {
        requiredModes.add(standard.immutable.features.privacyMode);
      }
    });

    if (requiredModes.has('stealth') && requiredModes.has('public')) {
      return 'incompatible';
    }

    // If any voucher requires stealth, the whole transfer must be stealth
    if (requiredModes.has('stealth')) {
      return 'stealth';
    }

    // If any voucher requires public (without stealth conflict), the whole transfer must be public
    if (requiredModes.has('public')) {
      return 'public';
    }

    return 'flexible';
  }, [selectedIds, vouchers, standards]);
}
