import { renderHook, act } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { useVoucherFilters } from '../useVoucherFilters';
import { VoucherSummary } from '../../types';

const mockVouchers: VoucherSummary[] = [
    {
        localInstanceId: 'inst-1',
        creatorId: 'did:key:user-own',
        currentAmount: '100',
        unit: 'MIN',
        displayCurrency: 'Minuto',
        displayStandardName: 'Minuto Standard',
        rawStandardName: 'Minuto Standard',
        voucherStandardUuid: 'uuid-minuto',
        status: 'active',
        isTestVoucher: false,
        validUntil: new Date(Date.now() + 10 * 24 * 60 * 60 * 1000).toISOString(), // 10 days (expiring soon)
        creatorFirstName: 'Alice',
        creatorLastName: 'Schmidt',
        description: 'Honey harvest voucher',
        transactionCount: 1,
        signaturesCount: 2,
        hasCollateral: false,
        allowPartialTransfers: true
    },
    {
        localInstanceId: 'inst-2',
        creatorId: 'did:key:other-user',
        currentAmount: '50',
        unit: 'MIN',
        displayCurrency: 'Minuto',
        displayStandardName: 'Minuto Standard',
        rawStandardName: 'Minuto Standard',
        voucherStandardUuid: 'uuid-minuto',
        status: 'active',
        isTestVoucher: false,
        validUntil: new Date(Date.now() + 90 * 24 * 60 * 60 * 1000).toISOString(), // 90 days
        creatorFirstName: 'Bob',
        creatorLastName: 'Meier',
        description: 'Bread voucher',
        transactionCount: 2,
        signaturesCount: 2,
        hasCollateral: false,
        allowPartialTransfers: true
    },
    {
        localInstanceId: 'inst-3',
        creatorId: 'did:key:other-user',
        currentAmount: '200',
        unit: 'FT',
        displayCurrency: 'FreeTaler [TEST]',
        displayStandardName: 'FreeTaler Standard [TEST]',
        rawStandardName: 'FreeTaler Standard',
        voucherStandardUuid: 'uuid-freetaler',
        status: 'archived',
        isTestVoucher: true,
        validUntil: new Date(Date.now() + 5 * 24 * 60 * 60 * 1000).toISOString(),
        creatorFirstName: 'Charlie',
        creatorLastName: 'Braun',
        description: 'Test service voucher',
        transactionCount: 0,
        signaturesCount: 1,
        hasCollateral: false,
        allowPartialTransfers: false
    }
];

describe('useVoucherFilters', () => {
    it('returns all vouchers by default and computes counts', () => {
        const { result } = renderHook(() => useVoucherFilters({ vouchers: mockVouchers, ownUserId: 'did:key:user-own' }));

        expect(result.current.filteredVouchers.length).toBe(3);
        expect(result.current.statusCounts.active).toBe(2);
        expect(result.current.statusCounts.archived).toBe(1);
        expect(result.current.standardCounts['Minuto Standard']).toBe(2);
        expect(result.current.standardCounts['FreeTaler Standard [TEST]']).toBe(1);
    });

    it('filters by status and standard', () => {
        const { result } = renderHook(() => useVoucherFilters({ vouchers: mockVouchers, ownUserId: 'did:key:user-own' }));

        act(() => {
            result.current.toggleStatusFilter('active');
        });
        expect(result.current.filteredVouchers.length).toBe(2);

        act(() => {
            result.current.toggleStandardFilter('Minuto Standard');
        });
        expect(result.current.filteredVouchers.length).toBe(2);

        act(() => {
            result.current.resetFilters();
        });
        expect(result.current.filteredVouchers.length).toBe(3);
    });

    it('filters by origin (own vs received)', () => {
        const { result } = renderHook(() => useVoucherFilters({ vouchers: mockVouchers, ownUserId: 'did:key:user-own' }));

        act(() => {
            result.current.setOriginFilter('own');
        });
        expect(result.current.filteredVouchers.length).toBe(1);
        expect(result.current.filteredVouchers[0].localInstanceId).toBe('inst-1');

        act(() => {
            result.current.setOriginFilter('received');
        });
        expect(result.current.filteredVouchers.length).toBe(2);
        expect(result.current.filteredVouchers.map(v => v.localInstanceId)).toEqual(['inst-3', 'inst-2']);
    });

    it('filters by mode (real vs test)', () => {
        const { result } = renderHook(() => useVoucherFilters({ vouchers: mockVouchers, ownUserId: 'did:key:user-own' }));

        act(() => {
            result.current.setModeFilter('real');
        });
        expect(result.current.filteredVouchers.length).toBe(2);
        expect(result.current.filteredVouchers.every(v => !v.isTestVoucher)).toBe(true);

        act(() => {
            result.current.setModeFilter('test');
        });
        expect(result.current.filteredVouchers.length).toBe(1);
        expect(result.current.filteredVouchers[0].localInstanceId).toBe('inst-3');
    });

    it('filters by expiringSoonOnly (< 30 days and active)', () => {
        const { result } = renderHook(() => useVoucherFilters({ vouchers: mockVouchers, ownUserId: 'did:key:user-own' }));

        act(() => {
            result.current.setExpiringSoonOnly(true);
        });
        // inst-1 is active & 10 days -> matches
        // inst-2 is active & 90 days -> does not match
        // inst-3 is archived & 5 days -> status is not active -> does not match
        expect(result.current.filteredVouchers.length).toBe(1);
        expect(result.current.filteredVouchers[0].localInstanceId).toBe('inst-1');
    });

    it('searches by free-text case-insensitive', () => {
        const { result } = renderHook(() => useVoucherFilters({ vouchers: mockVouchers, ownUserId: 'did:key:user-own' }));

        // Search by last name
        act(() => {
            result.current.setSearchQuery('meier');
        });
        expect(result.current.filteredVouchers.length).toBe(1);
        expect(result.current.filteredVouchers[0].localInstanceId).toBe('inst-2');

        // Search by currency
        act(() => {
            result.current.setSearchQuery('freetaler');
        });
        expect(result.current.filteredVouchers.length).toBe(1);
        expect(result.current.filteredVouchers[0].localInstanceId).toBe('inst-3');

        // Search by instance ID
        act(() => {
            result.current.setSearchQuery('inst-1');
        });
        expect(result.current.filteredVouchers.length).toBe(1);
        expect(result.current.filteredVouchers[0].localInstanceId).toBe('inst-1');
    });

    it('sorts correctly by amount and creator', () => {
        const { result } = renderHook(() => useVoucherFilters({ vouchers: mockVouchers, ownUserId: 'did:key:user-own' }));

        // amount_desc: inst-3 (200), inst-1 (100), inst-2 (50)
        act(() => {
            result.current.setSortOption('amount_desc');
        });
        expect(result.current.filteredVouchers.map(v => v.localInstanceId)).toEqual(['inst-3', 'inst-1', 'inst-2']);

        // amount_asc: inst-2 (50), inst-1 (100), inst-3 (200)
        act(() => {
            result.current.setSortOption('amount_asc');
        });
        expect(result.current.filteredVouchers.map(v => v.localInstanceId)).toEqual(['inst-2', 'inst-1', 'inst-3']);

        // creator_asc: Alice Schmidt, Bob Meier, Charlie Braun
        act(() => {
            result.current.setSortOption('creator_asc');
        });
        expect(result.current.filteredVouchers.map(v => v.localInstanceId)).toEqual(['inst-1', 'inst-2', 'inst-3']);
    });
});
