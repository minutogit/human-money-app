import { render, screen, waitFor, fireEvent, within } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { StandardsManager } from '../StandardsManager';
import { standardsService } from '../../../services/standardsService';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import * as clipboardUtils from '../../../utils/clipboard';
import { VoucherStandardInfo, VoucherStandardDefinition } from '../../../types';

vi.mock('../../../services/standardsService', () => ({
    standardsService: {
        getStandards: vi.fn(),
        parseStandard: vi.fn(),
        exportStandard: vi.fn(),
        inspectStandardFile: vi.fn(),
        importStandard: vi.fn(),
        deleteStandard: vi.fn(),
    },
}));

vi.mock('../../../utils/clipboard', () => ({
    copyToClipboard: vi.fn().mockResolvedValue(true),
    readFromClipboard: vi.fn().mockResolvedValue({ text: '', error: false }),
}));

let mockSessionValue: {
    protectAction: ReturnType<typeof vi.fn>;
    isAuthenticated: boolean;
} | null = null;

vi.mock('../../../context/SessionContext', () => ({
    useOptionalSession: () => mockSessionValue,
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn(),
    save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
    readFile: vi.fn(),
    writeFile: vi.fn(),
}));

vi.mock('react-i18next', async () => {
    const enMod = await import('../../../locales/en.json');
    const enTranslations = enMod.default as Record<string, string>;

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

    const stableT = (key: string, optionsOrDefault?: string | Record<string, unknown>) => {
        let val = customTranslations[key] || enTranslations[key];
        if (!val && typeof optionsOrDefault === 'string') {
            val = optionsOrDefault;
        }
        val = val || key;
        if (optionsOrDefault && typeof optionsOrDefault === 'object') {
            if (!customTranslations[key] && !enTranslations[key] && typeof optionsOrDefault.defaultValue === 'string') {
                val = optionsOrDefault.defaultValue;
            }
            for (const [k, v] of Object.entries(optionsOrDefault)) {
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
                issuerId: '0:is@did:key:z6Mkexample',
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
        mockSessionValue = null;
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
        try {
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
        } finally {
            consoleWarnSpy.mockRestore();
        }
    });

    it('protects standard deletion with requireFullAccess and forwards walletPassword', async () => {
        const protectActionMock = vi.fn(async (action: (pwd: string | null) => Promise<unknown>) => {
            return await action('master-pw');
        });
        mockSessionValue = {
            protectAction: protectActionMock,
            isAuthenticated: true,
        };
        vi.mocked(standardsService.deleteStandard).mockResolvedValue(undefined);

        render(<StandardsManager />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByTitle('Delete'));
        const modal = screen.getByRole('dialog');
        fireEvent.click(within(modal).getByRole('button', { name: 'Delete' }));

        await waitFor(() => {
            expect(protectActionMock).toHaveBeenCalledWith(expect.any(Function), {
                requireFullAccess: true,
            });
            expect(standardsService.deleteStandard).toHaveBeenCalledWith(
                'minuto-standard-uuid-1234',
                'master-pw'
            );
        });
    });

    it('exports standard to chosen path via ExportStandardModal', async () => {
        const exportedBytes = [1, 2, 3, 4];
        vi.mocked(standardsService.exportStandard).mockResolvedValue(exportedBytes);
        vi.mocked(save).mockResolvedValue('/tmp/minuto-standard.standard');
        vi.mocked(writeFile).mockResolvedValue(undefined);

        render(<StandardsManager />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });

        fireEvent.click(screen.getByRole('button', { name: /Export \(\.standard\)/i }));
        const modal = screen.getByRole('dialog');
        fireEvent.click(within(modal).getByRole('button', { name: /Export \(\.standard\)/i }));

        await waitFor(() => {
            expect(standardsService.exportStandard).toHaveBeenCalledWith(
                'minuto-standard-uuid-1234',
                { type: 'Cleartext' },
                null,
            );
            expect(writeFile).toHaveBeenCalledWith(
                '/tmp/minuto-standard.standard',
                new Uint8Array(exportedBytes),
            );
            expect(screen.getByText(/exported|exportiert/i)).toBeInTheDocument();
        });
    });

    it('keeps standard intact on delete cancel and surfaces in-use error when deletion fails', async () => {
        vi.mocked(standardsService.deleteStandard).mockRejectedValueOnce(
            new Error('Standard is still in use by 2 vouchers'),
        );

        render(<StandardsManager />);

        await waitFor(() => {
            expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
        });

        // Cancel delete modal
        fireEvent.click(screen.getByTitle('Delete'));
        const cancelModal = screen.getByRole('dialog');
        fireEvent.click(within(cancelModal).getByRole('button', { name: /Cancel/i }));
        expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
        expect(standardsService.deleteStandard).not.toHaveBeenCalled();

        // Confirm delete fails with in-use error
        fireEvent.click(screen.getByTitle('Delete'));
        const confirmModal = screen.getByRole('dialog');
        fireEvent.click(within(confirmModal).getByRole('button', { name: 'Delete' }));

        await waitFor(() => {
            expect(
                screen.getByText(/still in use by active vouchers|nicht gelöscht werden/i),
            ).toBeInTheDocument();
        });
        expect(screen.getByText('Minuto Standard')).toBeInTheDocument();
    });
});
