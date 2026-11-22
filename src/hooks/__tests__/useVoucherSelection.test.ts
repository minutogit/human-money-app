import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { useVoucherSelection } from '../useVoucherSelection';
import type { VoucherSummary } from '../../types';

vi.mock('../../services/transferService', () => ({
  transferService: {
    planTransfer: vi.fn(),
  },
}));

describe('useVoucherSelection', () => {
  it('computes exact BigInt selectionStats without float drift (0.1 + 0.2 === 0.3)', () => {
    const vouchers: VoucherSummary[] = [
      {
        localInstanceId: 'v-1',
        status: 'active',
        validUntil: '2030-01-01T00:00:00Z',
        creatorId: 'did:key:alice',
        description: '',
        currentAmount: '0.1',
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
      },
      {
        localInstanceId: 'v-2',
        status: 'active',
        validUntil: '2030-01-01T00:00:00Z',
        creatorId: 'did:key:bob',
        description: '',
        currentAmount: '0.2',
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
      },
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
});
