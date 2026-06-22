/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi, Mock, MockInstance } from 'vitest';
import { SettingsView } from '../SettingsView';
import { settingsService } from '../../services/settingsService';
import { authService } from '../../services/authService';

const { mockChangeLanguage } = vi.hoisted(() => ({
  mockChangeLanguage: vi.fn(),
}));

vi.mock('react-i18next', () => {
  const fs = require('fs');
  const path = require('path');
  const enPath = path.resolve(process.cwd(), 'src/locales/en.json');
  let enTranslations = {};
  try {
    enTranslations = JSON.parse(fs.readFileSync(enPath, 'utf-8'));
  } catch (err) {
    console.error(err);
  }

  const stableT = (key: string, options?: any) => {
    let val = (enTranslations as Record<string, string>)[key] || key;
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
      i18n: { language: 'en', changeLanguage: mockChangeLanguage },
      ready: true,
    }),
  };
});

// Mock dependencies
vi.mock('../../services/settingsService', () => ({
  settingsService: {
    getSettings: vi.fn(),
    saveSettings: vi.fn(),
  },
}));

vi.mock('../../services/authService', () => ({
  authService: {
    getLocalInstanceId: vi.fn(),
  },
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: vi.fn(() => ({
    protectAction: vi.fn((callback) => callback('test-password')),
  })),
}));

vi.mock('../ProfileSettings', () => ({
  ProfileSettings: () => <div data-testid="profile-settings-mock">Profile Settings Mock</div>,
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

describe('SettingsView Component', () => {
  const mockSettings = {
    bundleRetentionDays: 30,
    sessionTimeoutSeconds: 600, // 10 minutes
    privacyDefault: 'ask',
  };

  const mockOnBack = vi.fn();
  let writeTextSpy: MockInstance;

  beforeEach(() => {
    vi.clearAllMocks();
    mockChangeLanguage.mockClear();
    localStorage.clear();
    
    // Modern JSDOM has clipboard, but if it doesn't, ensure it exists
    if (!navigator.clipboard) {
      Object.defineProperty(navigator, 'clipboard', {
        value: {
          writeText: vi.fn(),
        },
        configurable: true,
        writable: true,
      });
    }
    
    writeTextSpy = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
    
    (settingsService.getSettings as Mock).mockResolvedValue({ ...mockSettings });
    (settingsService.saveSettings as Mock).mockResolvedValue(undefined);
    (authService.getLocalInstanceId as Mock).mockResolvedValue('test-device-id-1234');
  });

  it('renders loading state initially', () => {
    render(<SettingsView onBack={mockOnBack} />);
    expect(screen.getByText(/Configuring Environment\.\.\./i)).toBeInTheDocument();
  });

  it('renders tabs and starts on Identity Profile tab', async () => {
    render(<SettingsView onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    expect(screen.getByText('Identity Profile')).toBeInTheDocument();
    expect(screen.getByText('Security & Ops')).toBeInTheDocument();
    
    // Check that ProfileSettings sub-component is rendered
    expect(screen.getByTestId('profile-settings-mock')).toBeInTheDocument();
  });

  it('switches to Security & Ops tab and displays configuration fields', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Identity Profile');
    const securityTab = screen.getByText('Security & Ops');
    await user.click(securityTab);

    // Profile Settings should be hidden
    expect(screen.queryByTestId('profile-settings-mock')).not.toBeInTheDocument();

    // Configuration elements should be visible - query by role
    const inputs = screen.getAllByRole('spinbutton');
    expect(inputs[0]).toHaveValue(30); // History Storage
    expect(inputs[1]).toHaveValue(10); // Session Timeout

    expect(screen.getByText('Interactive')).toBeInTheDocument();
    expect(screen.getByText('Stealth')).toBeInTheDocument();
    expect(screen.getByText('Public')).toBeInTheDocument();
    expect(screen.getByDisplayValue('test-device-id-1234')).toBeInTheDocument();
  });

  it('allows copying device ID to clipboard', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Identity Profile');
    const securityTab = screen.getByText('Security & Ops');
    await user.click(securityTab);

    const copyButton = screen.getByRole('button', { name: /Copy/i });
    await user.click(copyButton);

    expect(writeTextSpy).toHaveBeenCalledWith('test-device-id-1234');
  });

  it('allows editing settings and calls saveSettings on form submit', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Identity Profile');
    const securityTab = screen.getByText('Security & Ops');
    await user.click(securityTab);

    // Edit fields using spinbutton roles
    const inputs = screen.getAllByRole('spinbutton');
    fireEvent.change(inputs[0], { target: { value: '45' } });
    fireEvent.change(inputs[1], { target: { value: '15' } });

    // Select "Stealth" option
    const stealthRadio = screen.getByLabelText(/Stealth/i);
    await user.click(stealthRadio);

    // Submit form
    const saveButton = screen.getByRole('button', { name: /Save System Changes/i });
    await user.click(saveButton);

    await waitFor(() => {
      expect(settingsService.saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          bundleRetentionDays: 45,
          sessionTimeoutSeconds: 900, // 15 minutes * 60 seconds
          privacyDefault: 'stealth',
        }),
        'test-password'
      );
    });

    expect(screen.getByText(/Configuration synchronized!/i)).toBeInTheDocument();
  });

  it('displays error message on failed save', async () => {
    const user = userEvent.setup();
    (settingsService.saveSettings as Mock).mockRejectedValue(new Error('Save failed'));
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Identity Profile');
    const securityTab = screen.getByText('Security & Ops');
    await user.click(securityTab);

    const saveButton = screen.getByRole('button', { name: /Save System Changes/i });
    await user.click(saveButton);

    expect(await screen.findByText(/Failed to save settings: Save failed/i)).toBeInTheDocument();
  });

  it('calls onBack prop when back button clicked', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Identity Profile');
    
    // Find back button in PageLayout
    const backButton = screen.getByRole('button', { name: /Back/i });
    await user.click(backButton);

    expect(mockOnBack).toHaveBeenCalled();
  });

  it('renders language selector card on Security & Ops tab and allows changing language', async () => {
    const user = userEvent.setup();
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Identity Profile');
    const securityTab = screen.getByText('Security & Ops');
    await user.click(securityTab);

    // Verify Language Card elements are present
    expect(screen.getByText('Language')).toBeInTheDocument();
    expect(screen.getByText('Choose the language for the user interface.')).toBeInTheDocument();
    
    // Find dropdown toggle button and click it to open
    const dropdownToggle = screen.getByRole('button', { name: /🇺🇸\s*English/i });
    expect(dropdownToggle).toBeInTheDocument();
    await user.click(dropdownToggle);

    // Once open, options are visible
    const englishBtnsBefore = screen.getAllByRole('button', { name: /🇺🇸\s*English/i });
    const englishBtn = englishBtnsBefore[1]; // Index 1 is the menu option
    const germanBtn = screen.getByRole('button', { name: /🇩🇪\s*Deutsch/i });
    
    expect(englishBtn).toBeInTheDocument();
    expect(germanBtn).toBeInTheDocument();

    // Click German language button
    await user.click(germanBtn);

    // Verify changeLanguage was called and localStorage was updated
    expect(mockChangeLanguage).toHaveBeenCalledWith('de');
    expect(setItemSpy).toHaveBeenCalledWith('app_language', 'de');

    // Click English language button (must reopen the dropdown first)
    await user.click(dropdownToggle);
    const englishBtnsAfter = screen.getAllByRole('button', { name: /🇺🇸\s*English/i });
    await user.click(englishBtnsAfter[1]);
    expect(mockChangeLanguage).toHaveBeenCalledWith('en');
    expect(setItemSpy).toHaveBeenCalledWith('app_language', 'en');
    
    setItemSpy.mockRestore();
  });
});
