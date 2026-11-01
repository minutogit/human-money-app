/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent, within } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { StandardsManager } from '../StandardsManager';
import { standardsService } from '../../services/standardsService';
import * as clipboardUtils from '../../utils/clipboard';
import { VoucherStandardInfo, VoucherStandardDefinition } from '../../types';

vi.mock('../../services/standardsService', () => ({
    standardsService: {
        getStandards: vi.fn(),
        parseStandard: vi.fn(),
        exportStandard: vi.fn(),
        inspectStandardFile: vi.fn(),
        importStandard: vi.fn(),
        deleteStandard: vi.fn(),
    },
}));

vi.mock('../../utils/clipboard', () => ({
    copyToClipboard: vi.fn().mockResolvedValue(true),
    readFromClipboard: vi.fn().mockResolvedValue({ text: '', error: false }),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn(),
    save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
    readFile: vi.fn(),
    writeFile: vi.fn(),
}));

vi.mock('react-i18next', () => {
    const fs = require('fs');
    const path = require('path');
    const enPath = path.resolve(process.cwd(), 'src/locales/en.json');
    let enTranslations: Record<string, string> = {};
    try {
        enTranslations = JSON.parse(fs.readFileSync(enPath, 'utf-8'));
    } catch {
        enTranslations = {};
    }

    const customTranslations: Record<string, string> = {
        'standards.detailsButton': 'Details',
        'standards.detailsModalTitle': 'Standard Details',
        'standards.unitLabel': 'Unit',
        'standards.issuerLabel': 'Issuer / Org',
        'standards.validityRangeLabel': 'Validity Range',
        'standards.divisibleLabel': 'Divisible',
        'standards.privacyModeLabel': 'Privacy Default',
        'standards.signaturesRequiredLabel': 'Signatures Required',
        'standards.allowedRolesLabel': 'Allowed Co-Signer Roles',
        'standards.customRulesCount': 'Contains {{count}} compliance rule(s)',
        'standards.showRawToml': 'Show Raw TOML',
        'standards.hideRawToml': 'Hide Raw TOML',
        'standards.copyToml': 'Copy TOML',
        'standards.decentralizedIssuer': 'De-centralized',
        'standards.unlimitedValidity': 'Unlimited',
    };

    const stableT = (key: string, options?: any) => {
        let val = customTranslations[key] || enTranslations[key] || key;
        if (options && typeof options === 'object') {
            for (const [k, v] of Object.entries(options)) {
                val = val.replace(new RegExp(`\\{\\{\\s*${k}\\s*\\}\\}`, 'g'), String(v));
            }
        }
        return val;
    };

    return {
        useTranslation: () => ({
            t: stableT,
            i18n: { language: 'en', changeLanguage: vi.fn() },
            ready: true,
        }),
    };
});

describe('StandardsManager & StandardDetailsModal', () => {
    const mockTomlContent = `
[immutable.identity]
name = "Minuto Voucher"
uuid = "minuto-standard-uuid-1234"
abbreviation = "MIN"

[immutable.blueprint]
unit = "Minuto"
primaryRedemptionType = "time"
collateralType = "personalGuarantee"

[immutable.features]
allowPartialTransfers = true
balancesAreSummable = true
amountDecimalPlaces = 2
privacyMode = "stealth"
allowedTTypes = ["transfer", "split"]

[immutable.issuance]
validityDurationRange = ["1Y", "3Y"]
issuanceMinimumValidityDuration = "1Y"
additionalSignaturesRange = [1, 2]
allowedSignatureRoles = ["Guarantor", "Auditor"]

[immutable.customRules.rule1]
expression = "amount <= 1000"
message = "Max 1000 per voucher"

[mutable.metadata]
issuerName = "Minuto Initiative"
keywords = ["local", "time-based"]

[mutable.appConfig]
defaultValidityDuration = "2Y"

[mutable.i18n.descriptions]
en = "A human-backed time currency standard based on 1 minute of work."
de = "Ein Minuto-Gutschein für eine Minute Arbeit."

[mutable.i18n.footnotes]
en = "Redeemable with community members."

[mutable.i18n.collateralDescriptions]
en = "Backed by personal labor guarantee."
`;

    const mockParsedDefinition: VoucherStandardDefinition = {
        immutable: {
            identity: {
                name: 'Minuto Voucher',
                uuid: 'minuto-standard-uuid-1234',
                abbreviation: 'MIN',
            },
            blueprint: {
                unit: 'Minuto',
                primaryRedemptionType: 'time',
                collateralType: 'personalGuarantee',
            },
            features: {
                allowPartialTransfers: true,
                balancesAreSummable: true,
                amountDecimalPlaces: 2,
                privacyMode: 'stealth',
                allowedTTypes: ['transfer', 'split'],
            },
            issuance: {
                validityDurationRange: ['1Y', '3Y'],
                issuanceMinimumValidityDuration: '1Y',
                additionalSignaturesRange: [1, 2],
                allowedSignatureRoles: ['Guarantor', 'Auditor'],
            },
            customRules: {
                rule1: {
                    expression: 'amount <= 1000',
                    message: 'Max 1000 per voucher',
                },
            },
        },
        mutable: {
            metadata: {
                issuerName: 'Minuto Initiative',
                keywords: ['local', 'time-based'],
            },
            appConfig: {
                defaultValidityDuration: '2Y',
            },
            i18n: {
                descriptions: {
                    en: 'A human-backed time currency standard based on 1 minute of work.',
                    de: 'Ein Minuto-Gutschein für eine Minute Arbeit.',
                },
                footnotes: {
                    en: 'Redeemable with community members.',
                },
                collateralDescriptions: {
                    en: 'Backed by personal labor guarantee.',
                },
            },
        },
    };

    const mockStandards: VoucherStandardInfo[] = [
        {
            id: 'minuto-standard-uuid-1234',
            displayName: 'Minuto Standard',
            content: mockTomlContent,
        },
    ];

    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(standardsService.getStandards).mockResolvedValue(mockStandards);
        vi.mocked(standardsService.parseStandard).mockResolvedValue(mockParsedDefinition);
    });

    it('renders standards list with unit badge and description preview', async () => {
        render(<StandardsManager />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });

        // Unit badge and TOML badge
        expect(screen.getByText('Minuto')).toBeInTheDocument();
        expect(screen.getByText('TOML')).toBeInTheDocument();

        // Localized preview text
        expect(
            screen.getByText('A human-backed time currency standard based on 1 minute of work.')
        ).toBeInTheDocument();

        // Details button and Export button
        expect(screen.getByText('Details')).toBeInTheDocument();
        expect(screen.getByText('Export (.standard)')).toBeInTheDocument();
    });

    it('clicking Details button opens StandardDetailsModal with metadata, rules, and localized description', async () => {
        render(<StandardsManager />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });

        const detailsBtn = screen.getByText('Details');
        fireEvent.click(detailsBtn);

        const modal = screen.getByRole('dialog');
        const inModal = within(modal);

        // Modal title
        expect(inModal.getByText('Standard Details')).toBeInTheDocument();

        // Identity and abbreviation
        expect(inModal.getByText(/Minuto Voucher/)).toBeInTheDocument();
        expect(inModal.getByText('(MIN)')).toBeInTheDocument();

        // UUID
        expect(inModal.getByText(/UUID: minuto-standard-uuid-1234/)).toBeInTheDocument();

        // Localized description & footnote
        expect(
            inModal.getByText('A human-backed time currency standard based on 1 minute of work.')
        ).toBeInTheDocument();
        expect(inModal.getByText('Redeemable with community members.')).toBeInTheDocument();

        // Key rules grid
        expect(inModal.getByText('Minuto Initiative')).toBeInTheDocument();
        expect(inModal.getByText('1Y – 3Y')).toBeInTheDocument();
        expect(inModal.getByText('Yes')).toBeInTheDocument();
        expect(inModal.getByText('stealth')).toBeInTheDocument();

        // Signatures required and roles
        expect(inModal.getByText('1–2')).toBeInTheDocument();
        expect(inModal.getByText('Guarantor')).toBeInTheDocument();
        expect(inModal.getByText('Auditor')).toBeInTheDocument();

        // Custom rules count
        expect(inModal.getByText('Contains 1 compliance rule(s)')).toBeInTheDocument();
    });

    it('toggles raw TOML viewer, displays raw content, and copies to clipboard', async () => {
        render(<StandardsManager />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByText('Details'));

        // Toggle raw TOML button initially shows "Show Raw TOML"
        const toggleBtn = screen.getByText('Show Raw TOML');
        expect(toggleBtn).toBeInTheDocument();
        expect(screen.queryByText(/\[immutable\.identity\]/)).not.toBeInTheDocument();

        // Expand raw TOML
        fireEvent.click(toggleBtn);
        expect(screen.getByText('Hide Raw TOML')).toBeInTheDocument();

        // The raw TOML pre block is now visible
        expect(screen.getByText(/minuto-standard-uuid-1234\.toml/)).toBeInTheDocument();
        const preElement = screen.getByText(/\[immutable\.identity\]/);
        expect(preElement).toBeInTheDocument();

        // Copy button
        const copyBtn = screen.getByText('Copy TOML');
        fireEvent.click(copyBtn);

        expect(clipboardUtils.copyToClipboard).toHaveBeenCalledWith(mockTomlContent);
        await waitFor(() => {
            expect(screen.getByText('Copied!')).toBeInTheDocument();
        });

        // Collapse raw TOML
        fireEvent.click(screen.getByText('Hide Raw TOML'));
        expect(screen.queryByText(/minuto-standard-uuid-1234\.toml/)).not.toBeInTheDocument();
    });

    it('clicking card header opens the modal and close button closes it', async () => {
        render(<StandardsManager />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });

        // Click the title to open modal
        fireEvent.click(screen.getByText('Minuto Standard'));

        expect(screen.getByText('Standard Details')).toBeInTheDocument();

        // Close modal
        const closeBtn = screen.getByText('Close');
        fireEvent.click(closeBtn);

        await waitFor(() => {
            expect(screen.queryByText('Standard Details')).not.toBeInTheDocument();
        });
    });

    it('handles fallback gracefully when parseStandard fails, keeping raw TOML accessible', async () => {
        const consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
        vi.mocked(standardsService.parseStandard).mockRejectedValue(new Error('Corrupt TOML'));

        render(<StandardsManager />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });

        // Click Details
        fireEvent.click(screen.getByText('Details'));

        // Modal opens with fallback values (displayName & id)
        const modal = screen.getByRole('dialog');
        const inModal = within(modal);

        await waitFor(() => {
            expect(inModal.getByText('Standard Details')).toBeInTheDocument();
        });

        expect(inModal.getByText(/Minuto Standard/)).toBeInTheDocument();
        expect(inModal.getByText(/UUID: minuto-standard-uuid-1234/)).toBeInTheDocument();

        // Raw TOML is still expandable and functions properly
        fireEvent.click(inModal.getByText('Show Raw TOML'));
        expect(inModal.getByText(/\[immutable\.identity\]/)).toBeInTheDocument();

        consoleWarnSpy.mockRestore();
    });
});
