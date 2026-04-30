import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { CreateVoucher } from '../CreateVoucher';
import { invoke } from '@tauri-apps/api/core';
import { VoucherStandardInfo } from '../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-log', () => ({
  info: vi.fn(),
  error: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((callback) => callback('test-password')),
  }),
}));

describe('CreateVoucher Component', () => {
  const mockStandards: VoucherStandardInfo[] = [
    {
      id: 'silver_v1',
      content: `name = "Silver Standard"
issuer_name = "Silver Issuer"
unit = "Silver"
abbreviation = "AG"
default_validity_duration = "P3Y"
amount_decimal_places = 2`,
    },
  ];

  const mockOnVoucherCreated = vi.fn();
  const mockOnCancel = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') {
        return Promise.resolve(mockStandards);
      }
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          first_name: 'John',
          last_name: 'Doe',
          organization: 'Test Org',
          community: 'Test Community',
          gender: '1',
          email: 'john@example.com',
          phone: '123456789',
          url: 'https://example.com',
          coordinates: '51.16, 10.45',
          service_offer: 'Test services',
          needs: 'Test needs',
          address: {
            street: 'Test Street',
            house_number: '123',
            zip_code: '12345',
            city: 'Test City',
            country: 'Germany',
          },
        });
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders voucher type dropdown and form fields', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    expect(await screen.findByLabelText(/Voucher Type/i)).toBeInTheDocument();
    expect(await screen.findByLabelText(/Amount \(e\.g\., 60\)/i)).toBeInTheDocument();
    expect(await screen.findByLabelText(/First Name \(Required\)/i)).toBeInTheDocument();
    expect(await screen.findByLabelText(/Last Name \(Required\)/i)).toBeInTheDocument();
  });

  it('loads and displays voucher standards', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    expect(await screen.findByText(/Silver Standard/i)).toBeInTheDocument();
  });

  it('prevents form submission when required fields are empty', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    await waitFor(() => {
      const createButton = screen.getByRole('button', { name: /Create Voucher/i });
      expect(createButton).toBeDisabled();
    });
  });
});
