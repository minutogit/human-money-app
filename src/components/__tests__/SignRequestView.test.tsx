import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { SignRequestView } from '../SignRequestView';
import { invoke } from '@tauri-apps/api/core';
import { VoucherDetails, VoucherStandardInfo, AppSettings, SignatureImpact } from '../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeFile: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../../utils/settingsUtils', () => ({
  updateLastUsedDirectory: vi.fn(),
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((callback) => callback('test-password')),
  }),
}));

describe('SignRequestView Component (Minuto Standard)', () => {
  const mockVoucherDetails: VoucherDetails = {
    local_instance_id: 'voucher-123',
    status: 'Active',
    voucher: {
      voucher_standard: {
        name: 'Minuto Standard',
        uuid: 'minuto-uuid-123',
        standard_definition_hash: 'hash-123',
        template: {
          description: 'Minuto test voucher',
          divisible: true,
        },
      },
      voucher_id: 'voucher-id-123',
      voucher_nonce: 'nonce-123',
      creation_date: '2024-01-01T00:00:00Z',
      valid_until: '2025-12-31T00:00:00Z',
      non_redeemable_test_voucher: false,
      nominal_value: {
        unit: 'Minutos',
        amount: '100',
        abbreviation: 'MIN',
      },
      creator: {
        id: 'did:key:z123',
        first_name: 'John',
        last_name: 'Doe',
      },
      signatures: [],
      transactions: [],
    },
    display_currency: 'MIN',
    display_standard_name: 'Minuto Standard',
    is_test_voucher: false,
  };

  const mockStandards: VoucherStandardInfo[] = [
    {
      id: 'minuto_v1',
      content: `name = "Minuto Standard"`,
    },
  ];

  const mockSettings: AppSettings = {
    bundle_retention_days: 30,
    session_timeout_seconds: 300,
    privacy_default: 'public',
  };

  const mockSignatureImpact: SignatureImpact = {
    is_allowed_role: true,
    fatal_conflicts: [],
    resolved_rules: ['Guarantor signature required'],
    gentle_hints: [],
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
      if (cmd === 'get_allowed_signature_roles_from_standard') {
        return Promise.resolve(['Guarantor', 'Endorser']);
      }
      if (cmd === 'evaluate_signature_suitability') {
        return Promise.resolve(mockSignatureImpact);
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
    });
  });
});
