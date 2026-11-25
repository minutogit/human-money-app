import { renderHook, act, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
    useVoucherCreation,
    standardRequiresCreatorGender,
    getStandardShortSummary,
} from '../useVoucherCreation';
import { standardsService } from '../../services/standardsService';
import { profileService } from '../../services/profileService';
import { marketService } from '../../services/marketService';
import { settingsService } from '../../services/settingsService';
import type {
    FrontendMyMarketProfile,
    VoucherStandardDefinition,
    VoucherStandardInfo,
} from '../../types';

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
        const emptyMarketProfile: FrontendMyMarketProfile = {
            profile: { id: 'did:key:zOwn' },
            visibility: {
                name: 'nobody',
                organization: 'nobody',
                community: 'nobody',
                city: 'nobody',
                url: 'nobody',
                picture: 'nobody',
                street: 'nobody',
                email: 'nobody',
                phone: 'nobody',
                coordinates: 'nobody',
                gender: 'nobody',
            },
            skills: [],
            ads: [],
            maxHops: 2,
            allowInquiries: false,
            published: false,
            qrIsPartial: false,
        };
        vi.mocked(marketService.getMyMarketProfile).mockResolvedValue(emptyMarketProfile);
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

    it('detects creator gender requirements and resolves localized standard summaries', () => {
        const creatorGenderStd = makeDefinition('uuid-cg', 'CreatorGender', 'P1Y');
        creatorGenderStd.immutable.customRules = {
            r1: {
                expression: "creator.gender in ['1', '2']",
                message: 'Gender required',
            },
        };
        expect(standardRequiresCreatorGender(creatorGenderStd)).toBe(true);

        const guarantorOnlyStd = makeDefinition('uuid-go', 'GuarantorOnly', 'P1Y');
        guarantorOnlyStd.immutable.customRules = {
            r1: {
                expression: "s.role != 'creator' && s.gender != ''",
                message: 'Guarantor gender required',
            },
        };
        expect(standardRequiresCreatorGender(guarantorOnlyStd)).toBe(false);
        expect(standardRequiresCreatorGender(null)).toBe(false);

        const localizedStd = makeDefinition('uuid-loc', 'FreeTaler', 'P1Y');
        localizedStd.mutable.i18n.descriptions = {
            de: 'Gutschein über {{amount}} Taler',
            en: 'Voucher for {{amount}} Taler',
        };
        const fallbackSpy = vi.fn((n, u) => `${n}:${u}`);
        expect(getStandardShortSummary(localizedStd, 'de-DE', ' 25 ', fallbackSpy)).toBe(
            'Gutschein über 25 Taler',
        );
        expect(getStandardShortSummary(localizedStd, 'de-DE', '   ', fallbackSpy)).toBe(
            'Gutschein über 1 Taler',
        );
        expect(fallbackSpy).not.toHaveBeenCalled();

        localizedStd.mutable.i18n.descriptions = {};
        expect(getStandardShortSummary(localizedStd, 'en', '10', fallbackSpy)).toBe(
            'FreeTaler:FreeTaler',
        );
        expect(fallbackSpy).toHaveBeenCalledWith('FreeTaler', 'FreeTaler');
    });

    it('validates required fields, offline-only protection confirmation, gender rule, and coordinate normalization', async () => {
        const creatorGenderDef = makeDefinition('uuid-cg', 'Minuto', 'P1Y');
        creatorGenderDef.immutable.customRules = {
            r1: { expression: "creator.gender != '0'", message: 'need gender' },
        };
        vi.mocked(standardsService.parseStandard).mockResolvedValue(creatorGenderDef);

        const { result } = renderHook(() => useVoucherCreation());
        await waitFor(() => expect(result.current.standards).toHaveLength(2));

        act(() => {
            result.current.setNonRedeemable(false);
        });
        expect(result.current.requiresOfflineOnlyConfirm).toBe(true);

        let validationErrors: Record<string, boolean> = {};
        act(() => {
            validationErrors = result.current.validate();
        });
        expect(validationErrors).toEqual({
            standard: true,
            amount: true,
            firstName: true,
            lastName: true,
            offlineOnlyConfirm: true,
        });

        await act(async () => {
            await result.current.handleStandardChange('std-a');
        });
        act(() => {
            validationErrors = result.current.validate();
        });
        expect(validationErrors.gender).toBe(true);

        act(() => {
            result.current.setAmount('50');
            result.current.setAllowOfflineOnly(true);
            result.current.setIdentity((prev) => ({
                ...prev,
                firstName: 'Alice',
                lastName: 'Smith',
                gender: '1',
                coordinates: '48.1351, 11.5820',
            }));
        });
        act(() => {
            result.current.handleCoordBlur();
            validationErrors = result.current.validate();
        });
        expect(validationErrors).toEqual({});
        expect(result.current.coordWarning).toBe('');
        expect(result.current.identity.coordinates).toBe('48.135100, 11.582000');

        act(() => {
            result.current.setIdentity((prev) => ({ ...prev, coordinates: 'not-coords' }));
        });
        act(() => {
            result.current.handleCoordBlur();
        });
        expect(result.current.coordWarning).toBe('Invalid GPS coordinate format.');
    });

    it('prepares voucher payload with decimal normalization, live committee, and clears state on empty or failed standard parse', async () => {
        const defA = makeDefinition('uuid-a', 'Standard A', 'P2Y');
        vi.mocked(standardsService.parseStandard)
            .mockResolvedValueOnce(defA)
            .mockRejectedValueOnce(new Error('invalid toml'));

        vi.mocked(settingsService.getLockProtectionOverview).mockResolvedValue({
            status: 'ok',
            readyTrustedCount: 1,
            servers: [
                {
                    writeUrl: 'https://sync.example/write',
                    readUrl: 'https://sync.example/read',
                    adminPub: 'admin-pub-1',
                    lockPub: 'pub-trusted-1',
                    source: 'own',
                    status: 'ok',
                    trusted: true,
                    allowPowTickets: false,
                    maxRetentionQuarter: 1,
                    probedAtSecs: 1,
                    error: null,
                },
            ],
            hasOfflineCachedAnchor: false,
            unresolvedIntentsCount: 0,
            pendingRestoreReconciliation: false,
        });

        const { result } = renderHook(() => useVoucherCreation());
        await waitFor(() => {
            expect(result.current.standards).toHaveLength(2);
            expect(result.current.lockOverview?.readyTrustedCount).toBe(1);
        });

        await act(async () => {
            await result.current.handleStandardChange('std-a');
        });
        expect(result.current.validityValue).toBe(2);
        expect(result.current.validityUnit).toBe('Y');

        act(() => {
            result.current.setAmount('12,50');
            result.current.setCollateral({ amount: '5,25', unit: 'EUR', abbreviation: 'EUR' });
            result.current.setNonRedeemable(false);
            result.current.setAllowOfflineOnly(false);
        });

        const prepared = result.current.prepareVoucherData();
        expect(prepared.nominalValue).toEqual({ amount: '12.50', unit: 'Standard A' });
        expect(prepared.collateral.amount).toBe('5.25');
        expect(prepared.validityDuration).toBe('P2Y');
        expect(prepared.creator.gender).toBe('0');
        expect(prepared.expectedCommittee).toEqual({
            lockPubs: ['pub-trusted-1'],
            threshold: 1,
            source: 'live',
        });

        await act(async () => {
            await result.current.handleStandardChange('');
        });
        expect(result.current.parsedStandard).toBeNull();
        expect(result.current.isParsingStandard).toBe(false);

        await act(async () => {
            await result.current.handleStandardChange('std-b');
        });
        expect(result.current.parsedStandard).toBeNull();
        expect(result.current.isParsingStandard).toBe(false);
    });
});
