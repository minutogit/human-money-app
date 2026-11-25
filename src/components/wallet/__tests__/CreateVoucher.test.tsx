import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { CreateVoucher } from '../CreateVoucher';
import { UiModeProvider } from '../../../context/UiModeContext';
import { invoke } from '@tauri-apps/api/core';
import { VoucherStandardInfo } from '../../../types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-log', () => ({
  info: vi.fn(),
  error: vi.fn(),
}));

vi.mock('../../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../../../context/SessionContext', () => ({
  useSession: () => ({
    protectAction: vi.fn((callback) => callback('test-password')),
  }),
  useOptionalSession: () => null,
}));

// Mock scrollIntoView as it's not implemented in JSDOM
window.HTMLElement.prototype.scrollIntoView = vi.fn();

describe('CreateVoucher Component', () => {
  const mockStandards: VoucherStandardInfo[] = [
    {
      id: 'freetaler_v1',
      displayName: 'FreeTaler',
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
    localStorage.clear();
    (invoke as Mock).mockImplementation((cmd: string, _args?: Record<string, unknown>) => {
      if (cmd === 'get_voucher_standards') {
        return Promise.resolve(mockStandards);
      }
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          firstName: 'John',
          lastName: 'Doe',
          organization: 'Test Org',
          community: 'Test Community',
          gender: '1',
          email: 'john@example.com',
          phone: '123456789',
          url: 'https://example.com',
          coordinates: '51.16, 10.45',
          serviceOffer: 'Test services',
          needs: 'Test needs',
          address: {
            street: 'Test Street',
            houseNumber: '123',
            zipCode: '12345',
            city: 'Test City',
            country: 'Germany',
          },
        });
      }
      if (cmd === 'parse_standard_toml') {
        // Return a mocked VoucherStandardDefinition based on the tomlContent
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'freetaler-uuid', name: 'FreeTaler', abbreviation: 'Taler' },
            blueprint: { unit: 'Taler', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 4, privacyMode: 'public', allowedTTypes: [] },
            issuance: { validityDurationRange: [], issuanceMinimumValidityDuration: 'P1Y', additionalSignaturesRange: [], allowedSignatureRoles: [] },
            customRules: {},
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P1Y' },
            i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
          }
        });
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders voucher type dropdown and simplified simple-mode form fields', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    expect(await screen.findByLabelText(/Voucher Type/i)).toBeInTheDocument();
    expect(await screen.findByLabelText(/Amount \(e\.g\., 60\)/i)).toBeInTheDocument();

    const summary = await screen.findByTestId('creator-profile-summary');
    expect(summary).toHaveTextContent('Your details (John Doe, Test City) are used');

    expect(screen.queryByTestId('manage-standards-button')).not.toBeInTheDocument();
    expect(screen.queryByTestId('voucher-validity-toggle')).not.toBeInTheDocument();
    expect(screen.queryByTestId('creator-gender-select')).not.toBeInTheDocument();

    await userEvent.click(screen.getByTestId('creator-profile-edit-btn'));

    expect(await screen.findByLabelText(/First Name \(Required\)/i)).toBeInTheDocument();
    expect(await screen.findByLabelText(/Last Name \(Required\)/i)).toBeInTheDocument();
    expect(await screen.findByLabelText(/City/i)).toBeInTheDocument();
  });

  it('loads and displays voucher standards', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    // After refactor, it shows displayName in the dropdown
    expect(await screen.findByText(/FreeTaler/i)).toBeInTheDocument();
  });

  it('validates required fields when form is submitted without data', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    const createButton = await screen.findByRole('button', { name: /Create Voucher/i });
    await userEvent.click(createButton);

    // Should not show confirmation modal or invoke backend creation
    await waitFor(() => {
      expect(screen.queryByText(/Create Voucher\?/i)).not.toBeInTheDocument();
      const missingBanner = screen.getByText(/Missing:/i);
      expect(missingBanner).toHaveTextContent('Voucher Type');
      expect(missingBanner).toHaveTextContent('Amount');
    });
    expect(invoke).not.toHaveBeenCalledWith('create_new_voucher', expect.anything());
  });

  it('focuses the first invalid field when form is submitted without data', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_user_profile') return Promise.resolve({});
      return Promise.resolve(undefined);
    });

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

    // Bottom error message should be visible with specific field names
    const errorMessages = await screen.findAllByText(/Missing:/i);
    expect(errorMessages.length).toBeGreaterThanOrEqual(1);

    // scrollIntoView should have been called
    expect(window.HTMLElement.prototype.scrollIntoView).toHaveBeenCalled();
  });

  it('automatically pre-populates creator details from existing profile on mount', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    const summary = await screen.findByTestId('creator-profile-summary');
    await waitFor(() => {
      expect(summary).toHaveTextContent('John Doe, Test City');
    });

    await userEvent.click(screen.getByTestId('creator-profile-edit-btn'));
    expect(await screen.findByLabelText(/First Name \(Required\)/i)).toHaveValue('John');
    expect(screen.getByLabelText(/Last Name \(Required\)/i)).toHaveValue('Doe');
  });

  it('updates validity duration when standard is selected', async () => {
    render(
      <UiModeProvider initialMode="expert">
        <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
      </UiModeProvider>
    );

    // Wait for standards to load
    const select = await screen.findByLabelText(/Voucher Type/i) as HTMLSelectElement;

    // Expand expert validity disclosure
    await userEvent.click(await screen.findByTestId('voucher-validity-toggle'));

    // Initial validity is 3 years
    const validityInput = screen.getByDisplayValue('3') as HTMLInputElement;
    expect(validityInput).toBeInTheDocument();

    // Select the standard
    await userEvent.selectOptions(select, 'freetaler_v1');

    // It should update to 1 year (from the mock parsed standard)
    await waitFor(() => {
      expect(screen.getByDisplayValue('1')).toBeInTheDocument();
    });
  });

  it('does not display empty "Missing:" text when selecting a standard before validation', async () => {
    render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    // Wait for standards to load
    const select = await screen.findByLabelText(/Voucher Type/i) as HTMLSelectElement;
    
    // Select standard (sets errors.standard = false)
    await userEvent.selectOptions(select, 'freetaler_v1');

    // The "Missing:" text should NOT be visible
    expect(screen.queryByText(/Missing:/i)).not.toBeInTheDocument();
  });

  it('shows a short standard description on selection and interpolates i18n description when available', async () => {
    const firstRender = render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    const select = await screen.findByLabelText(/Voucher Type/i) as HTMLSelectElement;
    await userEvent.selectOptions(select, 'freetaler_v1');

    expect(await screen.findByTestId('standard-short-description')).toHaveTextContent(
      'FreeTaler — Voucher denominated in Taler.'
    );
    firstRender.unmount();

    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          firstName: 'John',
          lastName: 'Doe',
          gender: '0',
          serviceOffer: 'Test services',
          address: { city: 'Test City' },
        });
      }
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'freetaler-uuid', name: 'FreeTaler', abbreviation: 'Taler' },
            blueprint: { unit: 'Taler', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 4, privacyMode: 'public', allowedTTypes: [] },
            issuance: { validityDurationRange: [], issuanceMinimumValidityDuration: 'P1Y', additionalSignaturesRange: [], allowedSignatureRoles: [] },
            customRules: {},
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P1Y' },
            i18n: {
              descriptions: { en: 'Represents {{amount}} units of regional value.' },
              footnotes: {},
              collateralDescriptions: {},
            },
          },
        });
      }
      return Promise.resolve(undefined);
    });

    render(<CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />);
    await userEvent.selectOptions(await screen.findByLabelText(/Voucher Type/i), 'freetaler_v1');
    await userEvent.type(await screen.findByLabelText(/Amount \(e\.g\., 60\)/i), '15');

    expect(await screen.findByTestId('standard-short-description')).toHaveTextContent(
      'Represents 15 units of regional value.'
    );
  });

  it('hides creator gender for guarantor-only rules and submits simple-mode Test voucher with gender "0"', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          firstName: 'John',
          lastName: 'Doe',
          gender: '1',
          serviceOffer: 'Test services',
          address: { city: 'Test City' },
        });
      }
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'minuto-uuid', name: 'Minuto', abbreviation: 'MIN' },
            blueprint: { unit: 'Minuto', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 0, privacyMode: 'public', allowedTTypes: [] },
            issuance: { validityDurationRange: [], issuanceMinimumValidityDuration: 'P3Y', additionalSignaturesRange: [2, 5], allowedSignatureRoles: ['guarantor'] },
            customRules: {
              guarantor_gender_male: {
                expression: "voucher.signatures.exists(s, s.role != 'creator' && s.details.gender == '1')",
              },
            },
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P3Y' },
            i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
          },
        });
      }
      if (cmd === 'create_new_voucher') {
        return Promise.resolve({ voucherId: 'v-1' });
      }
      return Promise.resolve(undefined);
    });

    const { container } = render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    expect(await screen.findByTestId('creator-profile-summary')).toHaveTextContent('John Doe, Test City');
    await userEvent.selectOptions(await screen.findByLabelText(/Voucher Type/i), 'freetaler_v1');
    await screen.findByTestId('standard-short-description');

    expect(screen.queryByTestId('creator-gender-select')).not.toBeInTheDocument();

    await userEvent.type(screen.getByLabelText(/Amount \(e\.g\., 60\)/i), '50');
    const submitButton = container.querySelector('form button[type="submit"]') as HTMLButtonElement;
    await userEvent.click(submitButton);

    const dialog = await screen.findByRole('dialog');
    await userEvent.click(within(dialog).getByRole('button', { name: /Create Voucher/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'create_new_voucher',
        expect.objectContaining({
          data: expect.objectContaining({
            nonRedeemableTestVoucher: true,
            creator: expect.objectContaining({ gender: '0' }),
          }),
        })
      );
    });
  });

  it('shows and validates creator gender when standard rules require creator gender', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          firstName: 'John',
          lastName: 'Doe',
          gender: '0',
          serviceOffer: 'Test services',
          address: { city: 'Test City' },
        });
      }
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'strict-uuid', name: 'Strict', abbreviation: 'STR' },
            blueprint: { unit: 'Strict', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 2, privacyMode: 'public', allowedTTypes: [] },
            issuance: { validityDurationRange: [], issuanceMinimumValidityDuration: 'P1Y', additionalSignaturesRange: [], allowedSignatureRoles: [] },
            customRules: {
              creator_gender: {
                expression: "voucher.creator.gender == '1' || voucher.creator.gender == '2'",
              },
            },
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P1Y' },
            i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
          },
        });
      }
      return Promise.resolve(undefined);
    });

    const { container } = render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    await userEvent.selectOptions(await screen.findByLabelText(/Voucher Type/i), 'freetaler_v1');

    const genderSelect = await screen.findByTestId('creator-gender-select');
    expect(genderSelect).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText(/Amount \(e\.g\., 60\)/i), '20');
    const submitButton = container.querySelector('form button[type="submit"]') as HTMLButtonElement;
    await userEvent.click(submitButton);

    await waitFor(() => {
      expect(genderSelect).toHaveClass('border-rose-500');
    });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    await userEvent.selectOptions(genderSelect, '2');
    await waitFor(() => {
      expect(genderSelect).not.toHaveClass('border-rose-500');
    });

    await userEvent.click(submitButton);
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
  });

  it('H10: requires explicit offline-only confirmation before issuing a real voucher with N=0', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_lock_protection_overview') {
        return Promise.resolve({
          status: 'none',
          readyTrustedCount: 0,
          servers: [],
          hasOfflineCachedAnchor: false,
          unresolvedIntentsCount: 0,
          pendingRestoreReconciliation: false,
        });
      }
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          firstName: 'John',
          lastName: 'Doe',
          gender: '1',
          serviceOffer: 'Test services',
          address: { city: 'Test City' },
        });
      }
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'freetaler-uuid', name: 'FreeTaler', abbreviation: 'Taler' },
            blueprint: { unit: 'Taler', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 4, privacyMode: 'public', allowedTTypes: [] },
            issuance: { validityDurationRange: [], issuanceMinimumValidityDuration: 'P1Y', additionalSignaturesRange: [], allowedSignatureRoles: [] },
            customRules: {},
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P1Y' },
            i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
          },
        });
      }
      if (cmd === 'create_new_voucher') {
        return Promise.resolve({ localInstanceId: 'v-offline', status: 'Active' });
      }
      return Promise.resolve(undefined);
    });

    const { container } = render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    await userEvent.selectOptions(await screen.findByLabelText(/Voucher Type/i), 'freetaler_v1');
    await userEvent.type(screen.getByLabelText(/Amount \(e\.g\., 60\)/i), '15');

    // Switch to Real Voucher mode
    await userEvent.click(screen.getByTestId('voucher-mode-real'));
    const warningDialog = await screen.findByRole('dialog');
    await userEvent.click(within(warningDialog).getByRole('button', { name: /Create Real Voucher/i }));

    const offlineCheckbox = await screen.findByTestId('voucher-confirm-offline-only');
    expect(offlineCheckbox).not.toBeChecked();

    // Attempting to submit without ticking offline confirmation must block submission
    const submitButton = container.querySelector('form button[type="submit"]') as HTMLButtonElement;
    await userEvent.click(submitButton);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    // Ticking offline-only confirmation unlocks submission with allowOfflineOnly=true
    await userEvent.click(offlineCheckbox);
    expect(offlineCheckbox).toBeChecked();

    await userEvent.click(submitButton);
    const confirmDialog = await screen.findByRole('dialog');
    await userEvent.click(within(confirmDialog).getByRole('button', { name: /Create Voucher/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'create_new_voucher',
        expect.objectContaining({
          data: expect.objectContaining({
            nonRedeemableTestVoucher: false,
            allowOfflineOnly: true,
          }),
        }),
      );
    });
  });

  it('H10: opens TOCTOU L2-unavailable modal on error.voucher.l2AnchorUnavailable and allows offline-only retry', async () => {
    let createAttempt = 0;
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_lock_protection_overview') {
        return Promise.resolve({
          status: 'ready',
          readyTrustedCount: 1,
          servers: [],
          hasOfflineCachedAnchor: false,
          unresolvedIntentsCount: 0,
          pendingRestoreReconciliation: false,
        });
      }
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          firstName: 'John',
          lastName: 'Doe',
          gender: '1',
          serviceOffer: 'Test services',
          address: { city: 'Test City' },
        });
      }
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'freetaler-uuid', name: 'FreeTaler', abbreviation: 'Taler' },
            blueprint: { unit: 'Taler', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 4, privacyMode: 'public', allowedTTypes: [] },
            issuance: { validityDurationRange: [], issuanceMinimumValidityDuration: 'P1Y', additionalSignaturesRange: [], allowedSignatureRoles: [] },
            customRules: {},
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P1Y' },
            i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
          },
        });
      }
      if (cmd === 'create_new_voucher') {
        createAttempt += 1;
        if (createAttempt === 1) {
          return Promise.reject({
            code: 'error.voucher.l2AnchorUnavailable',
            message: 'All lock servers unreachable',
          });
        }
        return Promise.resolve({ localInstanceId: 'v-fallback', status: 'Active' });
      }
      return Promise.resolve(undefined);
    });

    const { container } = render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    await userEvent.selectOptions(await screen.findByLabelText(/Voucher Type/i), 'freetaler_v1');
    await userEvent.type(screen.getByLabelText(/Amount \(e\.g\., 60\)/i), '25');

    await userEvent.click(screen.getByTestId('voucher-mode-real'));
    const warningDialog = await screen.findByRole('dialog');
    await userEvent.click(within(warningDialog).getByRole('button', { name: /Create Real Voucher/i }));

    await waitFor(() => {
      expect(screen.queryByTestId('voucher-confirm-offline-only')).not.toBeInTheDocument();
    });

    const submitButton = container.querySelector('form button[type="submit"]') as HTMLButtonElement;
    await userEvent.click(submitButton);
    const confirmDialog = await screen.findByRole('dialog');
    await userEvent.click(within(confirmDialog).getByRole('button', { name: /Create Voucher/i }));

    const retryBtn = await screen.findByTestId('l2-unavailable-retry-btn');
    const offlineBtn = await screen.findByTestId('l2-unavailable-offline-btn');
    expect(retryBtn).toBeInTheDocument();
    expect(offlineBtn).toBeInTheDocument();

    await userEvent.click(offlineBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'create_new_voucher',
        expect.objectContaining({
          data: expect.objectContaining({
            nonRedeemableTestVoucher: false,
            allowOfflineOnly: true,
          }),
        }),
      );
    });
  });

  it('H10: handles error.voucher.l2CommitteeChanged by showing quorum diff modal and retrying with actualCommittee', async () => {
    const updatedPub = 'aa'.repeat(32);
    let createAttempt = 0;
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_lock_protection_overview') {
        return Promise.resolve({
          status: 'ready',
          readyTrustedCount: 3,
          servers: [],
          hasOfflineCachedAnchor: false,
          unresolvedIntentsCount: 0,
          pendingRestoreReconciliation: false,
        });
      }
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          firstName: 'John',
          lastName: 'Doe',
          gender: '1',
          serviceOffer: 'Test services',
          address: { city: 'Test City' },
        });
      }
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'freetaler-uuid', name: 'FreeTaler', abbreviation: 'Taler' },
            blueprint: { unit: 'Taler', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 4, privacyMode: 'public', allowedTTypes: [] },
            issuance: { validityDurationRange: [], issuanceMinimumValidityDuration: 'P1Y', additionalSignaturesRange: [], allowedSignatureRoles: [] },
            customRules: {},
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P1Y' },
            i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
          },
        });
      }
      if (cmd === 'create_new_voucher') {
        createAttempt += 1;
        if (createAttempt === 1) {
          return Promise.reject({
            code: 'error.voucher.l2CommitteeChanged',
            message: 'Committee changed',
            details: {
              expectedThreshold: '2',
              expectedCount: '3',
              actualThreshold: '1',
              actualCount: '1',
              actualLockPubs: updatedPub,
              actualSource: 'live',
            },
          });
        }
        return Promise.resolve({ localInstanceId: 'v-updated', status: 'Active' });
      }
      return Promise.resolve(undefined);
    });

    const { container } = render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    await userEvent.selectOptions(await screen.findByLabelText(/Voucher Type/i), 'freetaler_v1');
    await userEvent.type(screen.getByLabelText(/Amount \(e\.g\., 60\)/i), '25');
    await userEvent.click(screen.getByTestId('voucher-mode-real'));
    const warningDialog = await screen.findByRole('dialog');
    await userEvent.click(within(warningDialog).getByRole('button', { name: /Create Real Voucher/i }));

    const submitButton = container.querySelector('form button[type="submit"]') as HTMLButtonElement;
    await userEvent.click(submitButton);
    const confirmDialog = await screen.findByRole('dialog');
    await userEvent.click(within(confirmDialog).getByRole('button', { name: /Create Voucher/i }));

    const diffDialog = await screen.findByRole('dialog');
    expect(within(diffDialog).getByText('Online protection servers changed')).toBeInTheDocument();
    expect(within(diffDialog).getByText(/2 of 3.*1 of 1/)).toBeInTheDocument();

    await userEvent.click(
      within(diffDialog).getByRole('button', { name: 'Create with updated protection' })
    );

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'create_new_voucher',
        expect.objectContaining({
          data: expect.objectContaining({
            allowOfflineOnly: false,
            expectedCommittee: {
              lockPubs: [updatedPub],
              threshold: 1,
              source: 'live',
            },
          }),
        }),
      );
    });
  });

  it('H10: falls back to L2-unavailable modal when l2CommitteeChanged has empty actualLockPubs', async () => {
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_lock_protection_overview') {
        return Promise.resolve({
          status: 'ready',
          readyTrustedCount: 1,
          servers: [],
          hasOfflineCachedAnchor: false,
          unresolvedIntentsCount: 0,
          pendingRestoreReconciliation: false,
        });
      }
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          firstName: 'John',
          lastName: 'Doe',
          gender: '1',
          address: { city: 'Test City' },
        });
      }
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'freetaler-uuid', name: 'FreeTaler', abbreviation: 'Taler' },
            blueprint: { unit: 'Taler', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 4, privacyMode: 'public', allowedTTypes: [] },
            issuance: { validityDurationRange: [], issuanceMinimumValidityDuration: 'P1Y', additionalSignaturesRange: [], allowedSignatureRoles: [] },
            customRules: {},
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P1Y' },
            i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
          },
        });
      }
      if (cmd === 'create_new_voucher') {
        return Promise.reject({
          code: 'error.voucher.l2CommitteeChanged',
          message: 'All lock servers gone',
          details: {
            expectedThreshold: '1',
            expectedCount: '1',
            actualThreshold: '0',
            actualCount: '0',
            actualLockPubs: '',
            actualSource: 'none',
          },
        });
      }
      return Promise.resolve(undefined);
    });

    const { container } = render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    await userEvent.selectOptions(await screen.findByLabelText(/Voucher Type/i), 'freetaler_v1');
    await userEvent.type(screen.getByLabelText(/Amount \(e\.g\., 60\)/i), '25');
    await userEvent.click(screen.getByTestId('voucher-mode-real'));
    const warningDialog = await screen.findByRole('dialog');
    await userEvent.click(within(warningDialog).getByRole('button', { name: /Create Real Voucher/i }));

    const submitButton = container.querySelector('form button[type="submit"]') as HTMLButtonElement;
    await userEvent.click(submitButton);
    const confirmDialog = await screen.findByRole('dialog');
    await userEvent.click(within(confirmDialog).getByRole('button', { name: /Create Voucher/i }));

    expect(await screen.findByTestId('l2-unavailable-offline-btn')).toBeInTheDocument();
  });

  it('H10: retries online with forceReprobe=true from L2-unavailable modal', async () => {
    let createAttempt = 0;
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') return Promise.resolve(mockStandards);
      if (cmd === 'get_lock_protection_overview') {
        return Promise.resolve({
          status: 'ready',
          readyTrustedCount: 1,
          servers: [],
          hasOfflineCachedAnchor: false,
          unresolvedIntentsCount: 0,
          pendingRestoreReconciliation: false,
        });
      }
      if (cmd === 'get_user_profile') {
        return Promise.resolve({
          firstName: 'John',
          lastName: 'Doe',
          gender: '1',
          address: { city: 'Test City' },
        });
      }
      if (cmd === 'parse_standard_toml') {
        return Promise.resolve({
          immutable: {
            identity: { uuid: 'freetaler-uuid', name: 'FreeTaler', abbreviation: 'Taler' },
            blueprint: { unit: 'Taler', primaryRedemptionType: 'goodsOrServices', collateralType: 'personalGuarantee' },
            features: { allowPartialTransfers: true, balancesAreSummable: true, amountDecimalPlaces: 4, privacyMode: 'public', allowedTTypes: [] },
            issuance: { validityDurationRange: [], issuanceMinimumValidityDuration: 'P1Y', additionalSignaturesRange: [], allowedSignatureRoles: [] },
            customRules: {},
          },
          mutable: {
            metadata: { issuerName: 'Human Money Project', keywords: [] },
            appConfig: { defaultValidityDuration: 'P1Y' },
            i18n: { descriptions: {}, footnotes: {}, collateralDescriptions: {} },
          },
        });
      }
      if (cmd === 'create_new_voucher') {
        createAttempt += 1;
        if (createAttempt === 1) {
          return Promise.reject({
            code: 'error.voucher.l2AnchorUnavailable',
            message: 'All lock servers unreachable',
          });
        }
        return Promise.resolve({ localInstanceId: 'v-reprobed', status: 'Active' });
      }
      return Promise.resolve(undefined);
    });

    const { container } = render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    await userEvent.selectOptions(await screen.findByLabelText(/Voucher Type/i), 'freetaler_v1');
    await userEvent.type(screen.getByLabelText(/Amount \(e\.g\., 60\)/i), '25');
    await userEvent.click(screen.getByTestId('voucher-mode-real'));
    const warningDialog = await screen.findByRole('dialog');
    await userEvent.click(within(warningDialog).getByRole('button', { name: /Create Real Voucher/i }));

    const submitButton = container.querySelector('form button[type="submit"]') as HTMLButtonElement;
    await userEvent.click(submitButton);
    const confirmDialog = await screen.findByRole('dialog');
    await userEvent.click(within(confirmDialog).getByRole('button', { name: /Create Voucher/i }));

    const retryBtn = await screen.findByTestId('l2-unavailable-retry-btn');
    await userEvent.click(retryBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'create_new_voucher',
        expect.objectContaining({
          data: expect.objectContaining({
            allowOfflineOnly: false,
            forceReprobe: true,
          }),
        }),
      );
    });
  });

  it('shows translated error alert and closes confirmation modal on generic creation failure', async () => {
    const baseImpl = (invoke as Mock).getMockImplementation();
    (invoke as Mock).mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'create_new_voucher') {
        return Promise.reject({
          code: 'error.auth.sessionTimeout',
          message: 'raw timeout message',
        });
      }
      return baseImpl ? baseImpl(cmd, args) : Promise.resolve(undefined);
    });

    const { container } = render(
      <CreateVoucher onVoucherCreated={mockOnVoucherCreated} onCancel={mockOnCancel} />
    );

    await userEvent.selectOptions(await screen.findByLabelText(/Voucher Type/i), 'freetaler_v1');
    await userEvent.type(screen.getByLabelText(/Amount \(e\.g\., 60\)/i), '30');

    const submitButton = container.querySelector('form button[type="submit"]') as HTMLButtonElement;
    await userEvent.click(submitButton);
    const confirmDialog = await screen.findByRole('dialog');
    await userEvent.click(within(confirmDialog).getByRole('button', { name: /Create Voucher/i }));

    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
      expect(
        screen.getByText('Creation Failed: Session timed out. Please provide your password.')
      ).toBeInTheDocument();
    });
    expect(screen.queryByText(/raw timeout message/)).not.toBeInTheDocument();
    expect(mockOnVoucherCreated).not.toHaveBeenCalled();
  });
});
