import { renderHook, act, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useVoucherCreation } from '../useVoucherCreation';
import { standardsService } from '../../services/standardsService';
import { profileService } from '../../services/profileService';
import { marketService } from '../../services/marketService';
import { settingsService } from '../../services/settingsService';
import type { VoucherStandardDefinition, VoucherStandardInfo } from '../../types';

vi.mock('../../services/standardsService', () => ({
    standardsService: {
        getStandards: vi.fn(),
        parseStandard: vi.fn(),
    },
}));

vi.mock('../../services/profileService', () => ({
    profileService: {
        getProfile: vi.fn(),
    },
}));

vi.mock('../../services/marketService', () => ({
    marketService: {
        getMyMarketProfile: vi.fn(),
    },
}));

vi.mock('../../services/settingsService', () => ({
    settingsService: {
        getLockProtectionOverview: vi.fn(),
    },
}));

function makeDefinition(uuid: string, name: string, duration: string): VoucherStandardDefinition {
    return {
        immutable: {
            identity: { uuid, name, abbreviation: name.slice(0, 3).toUpperCase() },
            blueprint: { unit: name, primaryRedemptionType: 'goods' },
            features: {
                allowPartialTransfers: true,
                balancesAreSummable: true,
                amountDecimalPlaces: 2,
                privacyMode: 'flexible',
                allowedTTypes: ['init', 'transfer'],
                defaultValidityDuration: duration,
            },
            issuance: {
                minInitialAmount: '1',
                maxInitialAmount: '1000',
                issuanceMinimumValidityDuration: 'P1Y',
                additionalSignaturesRequired: 0,
            },
            customRules: {},
        },
        mutable: {
            metadata: { issuerName: 'Test' },
            appConfig: { defaultValidityDuration: duration },
            i18n: { descriptions: { en: name } },
        },
    } as unknown as VoucherStandardDefinition;
}

describe('useVoucherCreation', () => {
    const stdA: VoucherStandardInfo = { id: 'std-a', displayName: 'Standard A', content: 'toml-a' };
    const stdB: VoucherStandardInfo = { id: 'std-b', displayName: 'Standard B', content: 'toml-b' };

    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(standardsService.getStandards).mockResolvedValue([stdA, stdB]);
        vi.mocked(profileService.getProfile).mockResolvedValue({ id: 'did:key:zOwn' });
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValue({ skills: [] } as never);
        vi.mocked(settingsService.getLockProtectionOverview).mockResolvedValue({
            status: 'none',
            readyTrustedCount: 0,
            servers: [],
            hasOfflineCachedAnchor: false,
            unresolvedIntentsCount: 0,
            pendingRestoreReconciliation: false,
        });
    });

    it('discards stale async parseStandard responses when switching standards rapidly', async () => {
        let resolveA!: (val: VoucherStandardDefinition) => void;
        let resolveB!: (val: VoucherStandardDefinition) => void;
        const promiseA = new Promise<VoucherStandardDefinition>((res) => {
            resolveA = res;
        });
        const promiseB = new Promise<VoucherStandardDefinition>((res) => {
            resolveB = res;
        });

        vi.mocked(standardsService.parseStandard).mockImplementation((content) => {
            if (content === 'toml-a') return promiseA;
            return promiseB;
        });

        const { result } = renderHook(() => useVoucherCreation());
        await waitFor(() => expect(result.current.standards).toHaveLength(2));

        act(() => {
            void result.current.handleStandardChange('std-a');
        });
        act(() => {
            void result.current.handleStandardChange('std-b');
        });

        const defB = makeDefinition('uuid-b', 'Standard B', 'P2Y');
        await act(async () => {
            resolveB(defB);
            await promiseB;
        });
        expect(result.current.selectedStandardId).toBe('std-b');
        expect(result.current.parsedStandard?.immutable.identity.uuid).toBe('uuid-b');

        // Stale resolution from standard A arrives later and must be ignored
        const defA = makeDefinition('uuid-a', 'Standard A', 'P5Y');
        await act(async () => {
            resolveA(defA);
            await promiseA;
        });
        expect(result.current.selectedStandardId).toBe('std-b');
        expect(result.current.parsedStandard?.immutable.identity.uuid).toBe('uuid-b');
    });
});
