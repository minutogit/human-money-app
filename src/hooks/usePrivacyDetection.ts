import { useMemo } from 'react';
import { VoucherSummary, VoucherStandardDefinition } from '../types';

export type PrivacyMode = 'stealth' | 'public' | 'flexible';

export function usePrivacyDetection(
  selectedIds: string[],
  vouchers: VoucherSummary[],
  standards: Record<string, VoucherStandardDefinition>
) {
  return useMemo(() => {
    if (selectedIds.length === 0) return 'flexible' as PrivacyMode;

    const selectedVouchers = selectedIds
      .map(id => vouchers.find(v => v.localInstanceId === id))
      .filter((v): v is VoucherSummary => !!v);

    const requiredModes = new Set<PrivacyMode>();

    selectedVouchers.forEach(v => {
      const standard = standards[v.voucherStandardUuid];
      if (standard?.immutable.features.privacyMode) {
        requiredModes.add(standard.immutable.features.privacyMode);
      }
    });

    // If any voucher requires stealth, the whole transfer must be stealth
    if (requiredModes.has('stealth')) {
      // Check for conflicts: if another requires public, we have a problem (but standard rules usually prevent this)
      return 'stealth';
    }

    // If all are public, return public
    if (requiredModes.size === 1 && requiredModes.has('public')) {
      return 'public';
    }

    // Otherwise (mix of public and flexible, or just flexible), return flexible
    return 'flexible';
  }, [selectedIds, vouchers, standards]);
}
