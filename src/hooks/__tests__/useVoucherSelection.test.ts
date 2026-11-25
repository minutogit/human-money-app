import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useVoucherSelection, parseStrictPositiveDecimal } from '../useVoucherSelection';
import { transferService, type TransferPlan } from '../../services/transferService';
import type { VoucherSummary } from '../../types';

vi.mock('../../services/transferService', () => ({
  transferService: {
    prepareTransferPlan: vi.fn(),
  },
}));

function makeVoucher(overrides: Partial<VoucherSummary> & { localInstanceId: string }): VoucherSummary {
  return {
    status: 'active',
    validUntil: '2030-01-01T00:00:00Z',
    creatorId: 'did:key:alice',
    description: '',
    currentAmount: '10',
    unit: 'Minuto',
    rawStandardName: 'Minuto',
    voucherStandardUuid: 'std-1',
    transactionCount: 1,
    signaturesCount: 1,
    hasCollateral: false,
    isTestVoucher: false,
    displayCurrency: 'Minuto',
    displayStandardName: 'Minuto',
    allowPartialTransfers: true,
    ...overrides,
  };
}

describe('useVoucherSelection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('computes exact BigInt selectionStats without float drift (0.1 + 0.2 === 0.3)', () => {
    const vouchers: VoucherSummary[] = [
      makeVoucher({ localInstanceId: 'v-1', currentAmount: '0.1' }),
      makeVoucher({ localInstanceId: 'v-2', creatorId: 'did:key:bob', currentAmount: '0.2' }),
    ];

    const { result } = renderHook(() => useVoucherSelection(vouchers));

    act(() => {
      result.current.toggleVoucher('v-1', '0.1');
      result.current.toggleVoucher('v-2', '0.2');
    });

    expect(result.current.selectionStats.Minuto).toEqual({
      totalExact: '0.3',
      count: 2,
    });
  });

  it('validates strict positive decimals while rejecting zero, negative, scientific, and over-precision inputs', () => {
    expect(parseStrictPositiveDecimal(' 12,50 ', 4)).toEqual({ normalized: '12.50', value: 12.5 });
    expect(parseStrictPositiveDecimal('5', 4)).toEqual({ normalized: '5', value: 5 });

    expect(parseStrictPositiveDecimal('', 4)).toBeNull();
    expect(parseStrictPositiveDecimal('0', 4)).toBeNull();
    expect(parseStrictPositiveDecimal('0.00', 4)).toBeNull();
    expect(parseStrictPositiveDecimal('-5', 4)).toBeNull();
    expect(parseStrictPositiveDecimal('1e3', 4)).toBeNull();
    expect(parseStrictPositiveDecimal('abc', 4)).toBeNull();
    expect(parseStrictPositiveDecimal('1.23456', 4)).toBeNull();
  });

  it('isolates test vs real realms in toggleVoucher, normalizes setVoucherAmount, and splits greedily in selectAmount', () => {
    const vouchers: VoucherSummary[] = [
      makeVoucher({
        localInstanceId: 'v-real-2',
        validUntil: '2029-01-01T00:00:00Z',
        currentAmount: '20',
        isTestVoucher: false,
      }),
      makeVoucher({
        localInstanceId: 'v-real-1',
        validUntil: '2028-01-01T00:00:00Z',
        currentAmount: '10',
        isTestVoucher: false,
      }),
      makeVoucher({
        localInstanceId: 'v-test-1',
        validUntil: '2028-06-01T00:00:00Z',
        currentAmount: '50',
        isTestVoucher: true,
      }),
    ];

    const { result } = renderHook(() => useVoucherSelection(vouchers));

    act(() => {
      result.current.toggleVoucher('v-real-1', '10');
    });
    expect(result.current.selectedMap.get('v-real-1')).toBe('10');

    // Toggling a test voucher must evict previously selected real vouchers
    act(() => {
      result.current.toggleVoucher('v-test-1', '50');
    });
    expect(result.current.selectedMap.has('v-real-1')).toBe(false);
    expect(result.current.selectedMap.get('v-test-1')).toBe('50');

    act(() => {
      result.current.setVoucherAmount('v-test-1', '12,5');
      result.current.setVoucherAmount('unselected', '5');
    });
    expect(result.current.selectedMap.get('v-test-1')).toBe('12.5');
    expect(result.current.selectedMap.has('unselected')).toBe(false);

    // Selecting real target evicts test selection and picks earlier-expiring real voucher first + partial split
    act(() => {
      result.current.selectAmount(15, 'Minuto', 2, false, 'std-1', 'draft-15');
    });
    expect(result.current.selectedMap.has('v-test-1')).toBe(false);
    expect(result.current.selectedMap.get('v-real-1')).toBe('10');
    expect(result.current.selectedMap.get('v-real-2')).toBe('5.00');
    expect(result.current.resolvedDraftKey).toBe('draft-15');
  });

  it('autoSelectGravitational computes exact BigInt split remainders and discards stale out-of-order responses', async () => {
    const vouchers: VoucherSummary[] = [
      makeVoucher({ localInstanceId: 'v-1', currentAmount: '0.1' }),
      makeVoucher({ localInstanceId: 'v-2', currentAmount: '0.5' }),
    ];

    let resolveFirst!: (val: TransferPlan) => void;
    let resolveSecond!: (val: TransferPlan) => void;
    const firstPromise = new Promise<TransferPlan>((res) => {
      resolveFirst = res;
    });
    const secondPromise = new Promise<TransferPlan>((res) => {
      resolveSecond = res;
    });

    vi.mocked(transferService.prepareTransferPlan)
      .mockReturnValueOnce(firstPromise)
      .mockReturnValueOnce(secondPromise);

    const { result } = renderHook(() => useVoucherSelection(vouchers));
    const dummyBreakdown = { sBoomerang: 1, sEigen: 1, sAge: 1, sChain: 1, sFit: 1 };

    let stalePromise!: Promise<TransferPlan | null>;
    let latestPromise!: Promise<TransferPlan | null>;
    act(() => {
      stalePromise = result.current.autoSelectGravitational('0.3', 'did:key:zBob', 'std-1', 2, false, 'draft-a');
      latestPromise = result.current.autoSelectGravitational('0.3', 'did:key:zBob', 'std-1', 2, false, 'draft-b');
    });

    const latestPlan: TransferPlan = {
      selected: [
        { localInstanceId: 'v-1', amount: '0.1', score: 1, breakdown: dummyBreakdown, reason: 'fit', standardUuid: 'std-1' },
        { localInstanceId: 'v-2', amount: '0.5', score: 0.9, breakdown: dummyBreakdown, reason: 'split', standardUuid: 'std-1' },
      ],
      totalSelected: '0.6',
      target: '0.3',
      needsSplit: true,
      planReason: 'fit',
    };
    const stalePlan: TransferPlan = {
      selected: [
        { localInstanceId: 'v-2', amount: '0.5', score: 0.5, breakdown: dummyBreakdown, reason: 'stale', standardUuid: 'std-1' },
      ],
      totalSelected: '0.5',
      target: '0.3',
      needsSplit: false,
      planReason: 'stale',
    };

    let latestRes: TransferPlan | null = null;
    await act(async () => {
      resolveSecond(latestPlan);
      latestRes = await latestPromise;
    });
    expect(latestRes).toEqual(latestPlan);
    expect(result.current.selectedMap.get('v-1')).toBe('0.1');
    expect(result.current.selectedMap.get('v-2')).toBe('0.20');
    expect(result.current.resolvedDraftKey).toBe('draft-b');

    let staleRes: TransferPlan | null = latestPlan;
    await act(async () => {
      resolveFirst(stalePlan);
      staleRes = await stalePromise;
    });
    expect(staleRes).toBeNull();
    expect(result.current.selectedMap.get('v-1')).toBe('0.1');
    expect(result.current.resolvedDraftKey).toBe('draft-b');
  });

  it('rejects autoSelectGravitational when transfer plan mixes real and test vouchers', async () => {
    const vouchers: VoucherSummary[] = [
      makeVoucher({ localInstanceId: 'v-real', currentAmount: '10', isTestVoucher: false }),
      makeVoucher({ localInstanceId: 'v-test', currentAmount: '10', isTestVoucher: true }),
    ];
    const dummyBreakdown = { sBoomerang: 1, sEigen: 1, sAge: 1, sChain: 1, sFit: 1 };

    vi.mocked(transferService.prepareTransferPlan).mockResolvedValueOnce({
      selected: [
        { localInstanceId: 'v-real', amount: '10', score: 1, breakdown: dummyBreakdown, reason: 'fit', standardUuid: 'std-1' },
        { localInstanceId: 'v-test', amount: '10', score: 1, breakdown: dummyBreakdown, reason: 'fit', standardUuid: 'std-1' },
      ],
      totalSelected: '20',
      target: '15',
      needsSplit: true,
      planReason: 'mixed',
    });

    const { result } = renderHook(() => useVoucherSelection(vouchers));

    let caughtMessage = '';
    await act(async () => {
      try {
        await result.current.autoSelectGravitational('15', 'did:key:zBob', 'std-1', 2, null);
      } catch (err) {
        caughtMessage = err instanceof Error ? err.message : String(err);
      }
    });

    expect(caughtMessage).toBe('Transfer plan mixed test and real vouchers');
    expect(result.current.currentPlan).toBeNull();
  });
});
