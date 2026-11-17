/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi, Mock, MockInstance } from 'vitest';
import { SettingsView } from '../SettingsView';
import { settingsService } from '../../services/settingsService';
import { authService } from '../../services/authService';
import { profileService } from '../../services/profileService';

const { mockChangeLanguage, mockNavigate } = vi.hoisted(() => ({
  mockChangeLanguage: vi.fn(),
  mockNavigate: vi.fn(),
}));

let mockAppState: any = { view: 'settings' };

vi.mock('../../context/NavigationContext', () => ({
  useNavigation: () => ({
    appState: mockAppState,
    navigate: mockNavigate,
    navigateToSettingsSection: vi.fn(),
    goBack: vi.fn(),
    isSidebarOpen: false,
    setSidebarOpen: vi.fn(),
    appVersion: '0.3.0',
    checkProfile: vi.fn(),
  }),
  useOptionalNavigation: () => ({
    appState: mockAppState,
    navigate: mockNavigate,
    navigateToSettingsSection: vi.fn(),
    goBack: vi.fn(),
    isSidebarOpen: false,
    setSidebarOpen: vi.fn(),
    appVersion: '0.3.0',
    checkProfile: vi.fn(),
  }),
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
    probeLockServer: vi.fn(),
    setOwnNominatedLockServer: vi.fn(),
    assembleAnchorTemplate: vi.fn(),
    prewarmL2Checkout: vi.fn().mockResolvedValue(undefined),
    checkL2LockStatus: vi.fn().mockResolvedValue({
      status: 'NoServers',
      unresolvedIntentsCount: 0,
      pendingRestoreReconciliation: false,
      templatesCount: 0,
      hasOwnNominatedServer: false,
      staleServersCount: 0,
      maxProbeAgeSecs: null,
    }),
    getL2LifecycleStatus: vi.fn().mockResolvedValue(null),
    runL2LifecycleTick: vi.fn().mockResolvedValue(null),
    registerGuarantorAnchorRole: vi.fn(),
    getReachPreset: vi.fn().mockResolvedValue('medium'),
    applyReachPreset: vi.fn().mockResolvedValue({}),
  },
}));

vi.mock('../../services/recoveryService', () => ({
  recoveryService: {
    getRecoveryProtectionStatus: vi.fn().mockResolvedValue(null),
  },
}));

vi.mock('../../services/authService', () => ({
  authService: {
    getLocalInstanceId: vi.fn(),
    listProfiles: vi.fn(),
    deleteProfile: vi.fn(),
  },
}));

vi.mock('../../services/profileService', () => ({
  profileService: {
    getUserId: vi.fn(),
  },
}));

vi.mock('../../services/marketService', () => ({
  marketService: {
    discoverAvailableSyncs: vi.fn().mockResolvedValue([]),
    getSyncAdminStatus: vi.fn().mockResolvedValue(null),
    probePublicSync: vi.fn(),
    savePublicSync: vi.fn(),
    probeContactSyncs: vi.fn().mockResolvedValue([]),
    prepareSyncSetup: vi.fn(),
    getRustSyncConfig: vi.fn().mockResolvedValue(''),
  },
}));

vi.mock('../../services/backupService', () => ({
  backupService: {
    getBackupStatus: vi.fn().mockResolvedValue({
      status: 'ok',
      lastBackupAt: null,
      backupDirty: false,
      syncFolder: null,
      changedBlobs: 0,
      syncBackupEnabled: true,
      lastSyncBackupAt: null,
      hasAuthorizedSyncBackupTarget: true,
    }),
    exportBackupFile: vi.fn(),
    syncBackupFolder: vi.fn(),
  },
}));

vi.mock('../../context/SessionContext', () => ({
  useSession: vi.fn(() => ({
    protectAction: vi.fn((callback) => callback('test-password')),
    profileName: 'Alice',
    notifyLogout: vi.fn(),
    clearLocks: vi.fn(),
  })),
}));

vi.mock('../StandardsManager', () => ({
  StandardsManager: () => <div data-testid="standards-manager-mock">Standards Manager Mock</div>,
}));

vi.mock('../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

describe('SettingsView Component', () => {
  const mockSettings = {
    sessionTimeoutSeconds: 600, // 10 minutes
    privacyDefault: 'ask',
    l2AnchorTemplatesMeta: [],
    ownNominatedLockServer: null,
    l2ProbeCache: {},
  };

  const mockOnBack = vi.fn();
  let writeTextSpy: MockInstance;

  beforeEach(() => {
    vi.clearAllMocks();
    mockChangeLanguage.mockClear();
    mockNavigate.mockClear();
    mockAppState = { view: 'settings' };
    localStorage.clear();
    localStorage.setItem('humoco_ui_mode', 'expert');
    
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
    (authService.listProfiles as Mock).mockResolvedValue([
      { folderName: 'alice-folder', profileName: 'Alice', isEncrypted: true },
    ]);
    (authService.deleteProfile as Mock).mockResolvedValue(undefined);
    (profileService.getUserId as Mock).mockResolvedValue('did:key:z6MkuTestUserId');
  });

  it('renders loading state initially', () => {
    render(<SettingsView onBack={mockOnBack} />);
    expect(screen.getByText(/Configuring Environment\.\.\./i)).toBeInTheDocument();
  });

  it('renders 4 tabs and starts on Profile tab', async () => {
    render(<SettingsView onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    expect(screen.getByText('Profile')).toBeInTheDocument();
    expect(screen.getByText('Security & Backup')).toBeInTheDocument();
    expect(screen.getByText('Sync Service')).toBeInTheDocument();
    expect(screen.getByText('Advanced')).toBeInTheDocument();
    
    // Check that marketplace redirect card is rendered on Profile tab
    expect(screen.getByRole('button', { name: /Open in Marketplace|Im Marktplatz öffnen/i })).toBeInTheDocument();
  });

  it('clicking each of the 4 tabs displays its respective components', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Profile');
    expect(screen.getByRole('button', { name: /Open in Marketplace|Im Marktplatz öffnen/i })).toBeInTheDocument();

    // 1. Security & Backup tab
    const securityTab = screen.getByText('Security & Backup');
    await user.click(securityTab);
    expect(screen.queryByRole('button', { name: /Open in Marketplace|Im Marktplatz öffnen/i })).not.toBeInTheDocument();
    expect(document.getElementById('settings-backup-section')).toBeInTheDocument();
    expect(screen.getByText('Privacy Settings')).toBeInTheDocument();
    expect(screen.getByText('Danger Zone')).toBeInTheDocument();

    // 2. Sync Service / Connection tab
    const syncTab = screen.getByText('Sync Service');
    await user.click(syncTab);
    expect(document.getElementById('settings-sync-section')).toBeInTheDocument();
    expect(screen.getByText('Add Sync Service')).toBeInTheDocument();
    expect(screen.getByText(/Host Own Service/i)).toBeInTheDocument();

    // 3. Advanced tab (no host section duplicate anymore)
    const advancedTab = screen.getByText('Advanced');
    await user.click(advancedTab);
    expect(document.getElementById('settings-network-section')).toBeInTheDocument();
    expect(document.getElementById('settings-standards-section')).toBeInTheDocument();
    expect(document.getElementById('settings-host-section')).not.toBeInTheDocument();
    expect(screen.getByText('Network & L2 Protection')).toBeInTheDocument();
  });

  it('switches to Security & Backup tab and displays privacy and backup options', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Profile');
    const securityTab = screen.getByText('Security & Backup');
    await user.click(securityTab);

    expect(screen.getByText('Interactive')).toBeInTheDocument();
    expect(screen.getByText('Stealth')).toBeInTheDocument();
    expect(screen.getByText('Public')).toBeInTheDocument();
    expect(document.getElementById('settings-backup-section')).toBeInTheDocument();
  });

  it('allows copying device ID to clipboard in Advanced tab', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Profile');
    const advancedTab = screen.getByText('Advanced');
    await user.click(advancedTab);

    const copyButton = screen.getByRole('button', { name: /^Copy$/i });
    await user.click(copyButton);

    expect(writeTextSpy).toHaveBeenCalledWith('test-device-id-1234');
  });

  it('auto-saves privacy and session timeout changes immediately without a save button', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Profile');
    const securityTab = screen.getByText('Security & Backup');
    await user.click(securityTab);

    expect(screen.queryByRole('button', { name: /Save System Changes/i })).not.toBeInTheDocument();

    // Select "Stealth" option triggers immediate auto-save
    const stealthRadio = screen.getByLabelText(/Stealth/i);
    await user.click(stealthRadio);

    await waitFor(() => {
      expect(settingsService.saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          privacyDefault: 'stealth',
        }),
        'test-password'
      );
    });

    // Edit session timeout field triggers immediate auto-save preserving stealth
    const inputs = screen.getAllByRole('spinbutton');
    expect(inputs.length).toBeGreaterThanOrEqual(1);
    fireEvent.change(inputs[0], { target: { value: '15' } });

    await waitFor(() => {
      expect(settingsService.saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          sessionTimeoutSeconds: 900, // 15 minutes * 60 seconds
          privacyDefault: 'stealth',
        }),
        'test-password'
      );
    });
  });

  it('displays error message on failed auto-save', async () => {
    const user = userEvent.setup();
    (settingsService.saveSettings as Mock).mockRejectedValue(new Error('Save failed'));
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Profile');
    const securityTab = screen.getByText('Security & Backup');
    await user.click(securityTab);

    const stealthRadio = screen.getByLabelText(/Stealth/i);
    await user.click(stealthRadio);

    expect(await screen.findByText(/Failed to save settings: Save failed/i)).toBeInTheDocument();
  });

  it('calls onBack prop when back button clicked', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Profile');
    
    // Find back button in PageLayout
    const backButton = screen.getByRole('button', { name: /Back/i });
    await user.click(backButton);

    expect(mockOnBack).toHaveBeenCalled();
  });

  it('renders language selector card on Profile tab and allows changing language', async () => {
    const user = userEvent.setup();
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Profile');

    // Verify Language Card elements are present on Profile tab
    expect(screen.getByText('Language')).toBeInTheDocument();
    
    // Find dropdown toggle button and click it to open
    const dropdownToggle = screen.getByRole('button', { name: /🇺🇸\s*English/i });
    expect(dropdownToggle).toBeInTheDocument();
    await user.click(dropdownToggle);

    // Click German language button
    const germanBtn = screen.getByRole('button', { name: /🇩🇪\s*Deutsch/i });
    await user.click(germanBtn);

    expect(mockChangeLanguage).toHaveBeenCalledWith('de');
    expect(setItemSpy).toHaveBeenCalledWith('app_language', 'de');
    
    setItemSpy.mockRestore();
  });

  it('opens confirmation modal and purges profile in Gefahrenzone', async () => {
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    await screen.findByText('Profile');
    const securityTab = screen.getByText('Security & Backup');
    await user.click(securityTab);

    expect(screen.getByText('Danger Zone')).toBeInTheDocument();
    const deleteButton = screen.getByRole('button', { name: /Delete Profile on this Device/i });
    await user.click(deleteButton);

    // Confirmation modal is open
    expect(screen.getByText('Delete Profile Permanently?')).toBeInTheDocument();
    expect(screen.getByText(/All local keys, vouchers, and settings for this profile will be permanently deleted/i)).toBeInTheDocument();

    // Confirm deletion
    const confirmBtn = screen.getByRole('button', { name: /^Delete$/i });
    await user.click(confirmBtn);

    await waitFor(() => {
      expect(authService.deleteProfile).toHaveBeenCalledWith({
        folderName: 'alice-folder',
        password: 'test-password',
      });
    });
  });

  it('deep-links to Security & Backup when initialSection is backup', async () => {
    mockAppState = { view: 'settings', initialSection: 'backup' };
    render(<SettingsView onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    // Should be on Security & Backup tab
    expect(document.getElementById('settings-backup-section')).toBeInTheDocument();
  });

  it('deep-links to Sync Service when initialSection is sync', async () => {
    mockAppState = { view: 'settings', initialSection: 'sync' };
    render(<SettingsView onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    // Should be on Sync Service tab
    expect(document.getElementById('settings-sync-section')).toBeInTheDocument();
  });

  it('deep-links to Advanced when initialSection is network', async () => {
    mockAppState = { view: 'settings', initialSection: 'network' };
    render(<SettingsView onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    // Should be on Advanced tab
    expect(document.getElementById('settings-network-section')).toBeInTheDocument();
  });

  it('renders unified 6-block layout without 4-tab bar or save button in simple mode', async () => {
    localStorage.setItem('humoco_ui_mode', 'simple');
    render(<SettingsView onBack={mockOnBack} />);

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    expect(screen.getByTestId('simple-blocks-layout')).toBeInTheDocument();
    expect(screen.queryByTestId('expert-tabs-layout')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Save System Changes/i })).not.toBeInTheDocument();

    expect(document.getElementById('settings-profile-section')).toBeInTheDocument();
    expect(document.getElementById('settings-security-section')).toBeInTheDocument();
    expect(document.getElementById('settings-reach-section')).toBeInTheDocument();
    expect(document.getElementById('settings-sync-section')).toBeInTheDocument();
    expect(document.getElementById('settings-view-section')).toBeInTheDocument();
    expect(document.getElementById('settings-danger-section')).toBeInTheDocument();
    expect(screen.getByTestId('auto-sync-master-toggle')).toBeInTheDocument();
  });

  it('toggles all 4 sync channels via auto-sync-master-toggle in simple mode', async () => {
    localStorage.setItem('humoco_ui_mode', 'simple');
    const user = userEvent.setup();
    render(<SettingsView onBack={mockOnBack} />);

    const masterToggle = await screen.findByTestId('auto-sync-master-toggle');
    expect(masterToggle).toBeChecked();

    await user.click(masterToggle);

    await waitFor(() => {
      expect(settingsService.saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          postAcceptFromContacts: false,
          marketSyncEnabled: false,
          wotShareViaSync: false,
          syncBackupEnabled: false,
        }),
        'test-password'
      );
    });

    await user.click(screen.getByTestId('auto-sync-master-toggle'));

    await waitFor(() => {
      expect(settingsService.saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          postAcceptFromContacts: true,
          marketSyncEnabled: true,
          wotShareViaSync: true,
          syncBackupEnabled: true,
        }),
        'test-password'
      );
    });
  });
});
