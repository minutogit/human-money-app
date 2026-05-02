import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
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

// Mock scrollIntoView as it's not implemented in JSDOM
window.HTMLElement.prototype.scrollIntoView = vi.fn();

describe('CreateVoucher Component', () => {
  const mockStandards: VoucherStandardInfo[] = [
    {
      id: 'freetaler_v1',
      content: `name = "FreeTaler"
issuer_name = "Human Money Project"
unit = "Taler"
abbreviation = "Taler"
default_validity_duration = "P1Y"
amount_decimal_places = 4`,
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

    expect(await screen.findByText(/FreeTaler/i)).toBeInTheDocument();
  });

  it('validates required fields when form is submitted without data', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    const createButton = await screen.findByRole('button', { name: /Create Voucher/i });
    await userEvent.click(createButton);

    // Should not show confirmation modal (validation failed)
    await waitFor(() => {
      expect(screen.queryByText(/Create Voucher\?/i)).not.toBeInTheDocument();
    });
  });

  it('focuses the first invalid field when form is submitted without data', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    const createButton = await screen.findByRole('button', { name: /Create Voucher/i });
    await userEvent.click(createButton);

    // First required field is "Voucher Type"
    const voucherTypeSelect = await screen.findByLabelText(/Voucher Type/i);
    expect(document.activeElement).toBe(voucherTypeSelect);
    expect(voucherTypeSelect).toHaveClass('border-rose-500');

    // Other required fields should also be highlighted
    const amountInput = await screen.findByLabelText(/Amount \(e\.g\., 60\)/i);
    const firstNameInput = await screen.findByLabelText(/First Name \(Required\)/i);
    const lastNameInput = await screen.findByLabelText(/Last Name \(Required\)/i);

    expect(amountInput).toHaveClass('border-rose-500');
    expect(firstNameInput).toHaveClass('border-rose-500');
    expect(lastNameInput).toHaveClass('border-rose-500');

    // Both top feedback and bottom error message should be visible
    const errorMessages = await screen.findAllByText(/Please fill in all required fields\./i);
    expect(errorMessages.length).toBeGreaterThanOrEqual(1);

    // scrollIntoView should have been called
    expect(window.HTMLElement.prototype.scrollIntoView).toHaveBeenCalled();
  });
});
