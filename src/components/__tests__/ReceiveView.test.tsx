import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi, Mock } from 'vitest';
import { ReceiveView } from '../ReceiveView';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { VoucherStandardInfo, AppSettings } from '../../types';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, params?: Record<string, string>) => {
      const translations: Record<string, string> = {
        'transfer.importFile.title': 'Import File',
        'transfer.importFile.description': 'Securely process incoming files.',
        'transfer.importFile.dropZone': 'Drop Transfer File',
        'transfer.importFile.dropZoneHint': 'Support for .transfer, .ask, and .sig files received from trusted parties.',
        'transfer.importFile.selectFile': 'Select File',
        'transfer.importFile.fileDetected': 'File Detected',
        'transfer.importFile.readyToImport': 'Ready to import',
        'transfer.importFile.discard': 'Discard File',
        'transfer.importFile.loading': 'Loading...',
        'transfer.importFile.importButton': 'Import File',
        'transfer.importFile.localProcessing': 'ALL PROCESSING IS PERFORMED LOCALLY ON THIS DEVICE',
        'transfer.importFile.formatTransfer': 'Transfer',
        'transfer.importFile.formatSignatureRequest': 'Signature Request',
        'transfer.importFile.formatSignatureResponse': 'Signature Response',
        'transfer.importFile.formatTransferDesc': 'Accept assets sent to you directly.',
        'transfer.importFile.formatSigReqDesc': 'Sign an asset for a peer.',
        'transfer.importFile.formatSigRespDesc': 'Attach a peer signature.',
        'transfer.importFile.importModalTitleTransfer': 'Import Transfer',
        'transfer.importFile.importModalTitleAsk': 'Import Signature Request',
        'transfer.importFile.importModalTitleSig': 'Import Signature Response',
        'transfer.importFile.typeLabel': `Type: ${params?.fileType || ''}`,
        'transfer.importFile.passwordLabel': 'Payload Password (Optional)',
        'transfer.importFile.passwordPlaceholder': 'Enter access password if required',
        'transfer.importFile.passwordHint': 'Required only if the sender encrypted this specific payload with a password.',
        'transfer.importFile.confirmImport': 'Import',
        'transfer.importFile.syncFromBackup': 'Sync from Backup',
        'transfer.importFile.criticalConflict': 'CRITICAL: Chronological Conflict',
        'transfer.importFile.acceptResponsibility': 'I accept responsibility for potential double-spend conflicts.',
        'transfer.importFile.mandatoryAffirmation': 'Mandatory Affirmation',
        'transfer.importFile.typeToConfirm': 'TYPE "IMPORT" TO CONFIRM',
        'transfer.importFile.importAnyway': 'Import Anyway',
        'transfer.importFile.noPayload': 'No payload selected.',
        'transfer.importFile.unknownFormat': 'Unknown payload format.',
        'transfer.importFile.invalidFileType': 'Invalid file type dropped.',
      };
      return translations[key] || key;
    },
    i18n: { language: 'en' },
  }),
}));

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: vi.fn(),
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

describe('ReceiveView Component (FreeTaler Standard)', () => {
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

  const mockSettings: AppSettings = {
    bundleRetentionDays: 30,
    sessionTimeoutSeconds: 300,
    privacyDefault: 'public',
  };

  const mockOnBack = vi.fn();
  const mockOnReceiveSuccess = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') {
        return Promise.resolve(mockStandards);
      }
      if (cmd === 'get_app_settings') {
        return Promise.resolve(mockSettings);
      }
      return Promise.resolve(undefined);
    });
  });

  it('renders drop zone and select button', async () => {
    render(
      <ReceiveView onBack={mockOnBack} onReceiveSuccess={mockOnReceiveSuccess} />
    );

    expect(await screen.findByText(/Drop Transfer File/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Select File/i })).toBeInTheDocument();
  });

  it('opens file dialog when select button is clicked', async () => {
    (open as Mock).mockResolvedValue('/path/to/bundle.transfer');

    render(
      <ReceiveView onBack={mockOnBack} onReceiveSuccess={mockOnReceiveSuccess} />
    );

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Select File/i })).toBeInTheDocument();
    });
  });

  it('calls receive_bundle with correct camelCase arguments when importing a transfer', async () => {
    // 1. Mock file selection
    (open as Mock).mockResolvedValue('/path/to/test.transfer');
    
    // 2. Mock file reading (empty array for simplicity)
    const { readFile } = await import('@tauri-apps/plugin-fs');
    (readFile as Mock).mockResolvedValue(new Uint8Array([1, 2, 3]));

    render(
      <ReceiveView onBack={mockOnBack} onReceiveSuccess={mockOnReceiveSuccess} />
    );

    // 3. Click Select File
    const selectButton = screen.getByRole('button', { name: /Select File/i });
    selectButton.click();

    // 4. Verify invoke call happens automatically
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('receive_bundle', expect.objectContaining({
        forceAcceptToleranceBundle: false,
        bundleData: [1, 2, 3]
      }));
    });
  });

  it('opens password modal when decryption fails and leaves Import button enabled', async () => {
    // 1. Mock file selection
    (open as Mock).mockResolvedValue('/path/to/test.transfer');
    
    // 2. Mock file reading
    const { readFile } = await import('@tauri-apps/plugin-fs');
    (readFile as Mock).mockResolvedValue(new Uint8Array([1, 2, 3]));

    // 3. Mock receive_bundle to reject with MacError
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') {
        return Promise.resolve(mockStandards);
      }
      if (cmd === 'get_app_settings') {
        return Promise.resolve(mockSettings);
      }
      if (cmd === 'receive_bundle') {
        return Promise.reject(new Error('MacError'));
      }
      return Promise.resolve(undefined);
    });

    render(
      <ReceiveView onBack={mockOnBack} onReceiveSuccess={mockOnReceiveSuccess} />
    );

    // 4. Click Select File
    const selectButton = screen.getByRole('button', { name: /Select File/i });
    fireEvent.click(selectButton);

    // 5. Assert the password modal opens
    // Title is 'Import Transfer' in translation mock: 'transfer.importFile.importModalTitleTransfer' -> 'Import Transfer'
    const modalTitle = await screen.findByText('Import Transfer');
    expect(modalTitle).toBeInTheDocument();

    // 6. Assert "Import" button (confirmText) is enabled and does not say "Processing..."
    const importButton = screen.getByRole('button', { name: 'Import' });
    expect(importButton).toBeInTheDocument();
    expect(importButton).not.toBeDisabled();
    expect(importButton.textContent).not.toContain('common.processing');
  });

  it('keeps Import button enabled when decryption fails on retry (second wrong password)', async () => {
    // 1. Mock file selection
    (open as Mock).mockResolvedValue('/path/to/test.transfer');
    
    // 2. Mock file reading
    const { readFile } = await import('@tauri-apps/plugin-fs');
    (readFile as Mock).mockResolvedValue(new Uint8Array([1, 2, 3]));

    // 3. Mock receive_bundle to reject with MacError
    let rejectCount = 0;
    (invoke as Mock).mockImplementation((cmd: string) => {
      if (cmd === 'get_voucher_standards') {
        return Promise.resolve(mockStandards);
      }
      if (cmd === 'get_app_settings') {
        return Promise.resolve(mockSettings);
      }
      if (cmd === 'receive_bundle') {
        rejectCount++;
        return Promise.reject(new Error('MacError'));
      }
      return Promise.resolve(undefined);
    });

    render(
      <ReceiveView onBack={mockOnBack} onReceiveSuccess={mockOnReceiveSuccess} />
    );

    // 4. Click Select File
    const selectButton = screen.getByRole('button', { name: /Select File/i });
    fireEvent.click(selectButton);

    // 5. Wait for password modal to open
    await screen.findByText('Import Transfer');

    // 6. Type a password and click confirm
    const passwordInput = screen.getByPlaceholderText('Enter access password if required');
    fireEvent.change(passwordInput, { target: { value: 'wrong-password' } });

    const importButton = screen.getByRole('button', { name: 'Import' });
    fireEvent.click(importButton);

    // 7. Assert that the call to receive_bundle was attempted twice (once initial, once after clicking import)
    await waitFor(() => {
      expect(rejectCount).toBe(2);
    });

    // 8. Assert modal is still open, Import button is still enabled, and does not show "Processing..."
    const reOpenedModalTitle = screen.getByText('Import Transfer');
    expect(reOpenedModalTitle).toBeInTheDocument();

    const reImportButton = screen.getByRole('button', { name: 'Import' });
    expect(reImportButton).not.toBeDisabled();
    expect(reImportButton.textContent).not.toContain('common.processing');
  });
});
