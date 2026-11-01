import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { SignRequestView } from '../SignRequestView';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { VoucherDetails, VoucherStandardInfo, AppSettings, SignatureImpact } from '../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeFile: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  revealItemInDir: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../../utils/settingsUtils', () => ({
  updateLastUsedDirectory: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((callback) => callback('test-password')),
  }),
}));

describe('SignRequestView Component (Minuto Standard)', () => {
  const mockVoucherDetails: VoucherDetails = {
    localInstanceId: 'voucher-123',
    status: 'active',
    voucher: {
      voucherStandard: {
        name: 'Minuto Standard',
        uuid: 'minuto-uuid-123',
        standardDefinitionHash: 'hash-123',
        template: {
          description: 'Minuto test voucher',
          allowPartialTransfers: true,
        },
      },
      voucherId: 'voucher-id-123',
      voucherNonce: 'nonce-123',
      creationDate: '2024-01-01T00:00:00Z',
      validUntil: '2025-12-31T00:00:00Z',
      nonRedeemableTestVoucher: false,
      nominalValue: {
        unit: 'Minutos',
        amount: '100',
        abbreviation: 'MIN',
      },
      creator: {
        id: 'did:key:z123',
        firstName: 'John',
        lastName: 'Doe',
      },
      signatures: [],
      transactions: [],
    },
    displayCurrency: 'MIN',
    displayStandardName: 'Minuto Standard',
    isTestVoucher: false,
  };

  const mockStandards: VoucherStandardInfo[] = [
    {
      id: 'minuto_v1',
      displayName: 'Minuto Standard',
      content: `name = "Minuto Standard"
uuid = "minuto-uuid-123"`,
    },
  ];

  const mockSettings: AppSettings = {
    sessionTimeoutSeconds: 300,
    privacyDefault: 'public',
  };

  const mockSignatureImpact: SignatureImpact = {
    isAllowedRole: true,
    fatalConflicts: [],
    resolvedRules: ['Guarantor signature required'],
    gentleHints: [],
  };

  const mockOnBack = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') {
        return Promise.resolve(mockStandards);
      }
      if (cmd === 'get_app_settings') {
        return Promise.resolve(mockSettings);
      }
      if (cmd === 'get_profile') {
        return Promise.resolve({ id: 'did:key:signer', firstName: 'Alice' });
      }
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'minuto-uuid-123', name: 'Minuto Standard', abbreviation: 'MIN' },
            blueprint: { unit: 'Minutos', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 0, privacyMode: 'flexible', allowedTTypes: ['init', 'transfer', 'split'] },
            issuance: { validityDurationRange: ['P1Y', 'P10Y'], issuanceMinimumValidityDuration: 'P3Y', additionalSignaturesRange: [2, 2], allowedSignatureRoles: ['guarantor'] },
            customRules: {},
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P5Y', roundUpValidityTo: 'P1Y', serverHistoryRetention: 'P3M' },
            i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
          }
        });
      }
      if (cmd === 'evaluate_signature_suitability') {
        return Promise.resolve(mockSignatureImpact);
      }
      if (cmd === 'create_detached_signature_response_bundle') {
        return Promise.resolve([1, 2, 3, 4]);
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders voucher details for Minuto Standard', async () => {
    render(
      <SignRequestView voucherData={mockVoucherDetails} onBack={mockOnBack} />
    );

    await waitFor(() => {
      expect(screen.getByText(/Signature Request/i)).toBeInTheDocument();
      expect(screen.getByText(/Voucher Details/i)).toBeInTheDocument();
      expect(screen.getByText(/guarantor/i)).toBeInTheDocument();
    });
  });

  it('shows inline gender selection when gender is required and profile lacks valid gender', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_app_settings') return Promise.resolve(mockSettings);
      if (cmd === 'get_profile') return Promise.resolve({ id: 'did:key:signer', firstName: 'Alice' });
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'minuto-uuid-123', name: 'Minuto Standard', abbreviation: 'MIN' },
            issuance: { allowedSignatureRoles: ['guarantor'] },
            customRules: {
              genderRule: { expression: 'details.gender in ["1", "2"]' },
            },
          },
        });
      }
      if (cmd === 'evaluate_signature_suitability') return Promise.resolve(mockSignatureImpact);
      return Promise.resolve(undefined);
    });

    render(<SignRequestView voucherData={mockVoucherDetails} onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByText(/Gender Specification Required in Profile/i)).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Save to Profile & Verify/i })).toBeInTheDocument();
    });
  });

  it('shows gender warning when gender is required and includeDetails is unchecked', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_app_settings') return Promise.resolve(mockSettings);
      if (cmd === 'get_profile') return Promise.resolve({ id: 'did:key:signer', firstName: 'Alice', gender: '1' });
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'minuto-uuid-123', name: 'Minuto Standard', abbreviation: 'MIN' },
            issuance: { allowedSignatureRoles: ['guarantor'] },
            customRules: {
              genderRule: { expression: 'details.gender in ["1", "2"]' },
            },
          },
        });
      }
      if (cmd === 'evaluate_signature_suitability') return Promise.resolve(mockSignatureImpact);
      return Promise.resolve(undefined);
    });

    render(<SignRequestView voucherData={mockVoucherDetails} onBack={mockOnBack} />);

    // Initially gender is set, so inline selection is not needed
    await waitFor(() => {
      expect(screen.queryByText(/Gender Specification Required in Profile/i)).not.toBeInTheDocument();
    });

    // Uncheck includeDetails
    const checkbox = screen.getByRole('checkbox');
    fireEvent.click(checkbox);

    await waitFor(() => {
      expect(screen.getByText(/requires disclosing your identity to verify gender parity/i)).toBeInTheDocument();
    });
  });

  it('shows success screen after signing and saving signature file', async () => {
    (save as Mock).mockResolvedValue('/tmp/signature-voucher-.sig');

    render(<SignRequestView voucherData={mockVoucherDetails} onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Sign Now/i })).toBeInTheDocument();
    });

    const signButton = screen.getByRole('button', { name: /Sign Now/i });
    fireEvent.click(signButton);

    await waitFor(() => {
      expect(screen.getAllByText(/Signature \(\.sig\) Saved!/i).length).toBeGreaterThan(0);
      expect(screen.getByText('/tmp/signature-voucher-.sig')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /Done/i })).toBeInTheDocument();
    });

    // Clicking Done calls onBack
    fireEvent.click(screen.getByRole('button', { name: /Done/i }));
    expect(mockOnBack).toHaveBeenCalledTimes(1);
  });
});
