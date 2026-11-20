/* eslint-disable @typescript-eslint/no-require-imports, no-undef, @typescript-eslint/no-explicit-any */
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi, Mock, MockInstance } from 'vitest';
import { SettingsView } from '../SettingsView';
import { UiModeProvider } from '../../../context/UiModeContext';
import { settingsService } from '../../../services/settingsService';
import { authService } from '../../../services/authService';
import { profileService } from '../../../services/profileService';
import { AppConfigDto, UiMode } from '../../../types';

const { mockChangeLanguage, mockNavigate } = vi.hoisted(() => ({
  mockChangeLanguage: vi.fn(),
  mockNavigate: vi.fn(),
}));

let mockAppState: any = { view: 'settings' };

vi.mock('../../../context/NavigationContext', () => ({
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
vi.mock('../../../services/settingsService', () => ({
  settingsService: {
    getConfig: vi.fn(),
    patchUiPreferences: vi.fn(),
    patchSecurityPolicy: vi.fn(),
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

vi.mock('../../../services/recoveryService', () => ({
  recoveryService: {
    getRecoveryProtectionStatus: vi.fn().mockResolvedValue(null),
  },
}));

vi.mock('../../../services/authService', () => ({
  authService: {
    getLocalInstanceId: vi.fn(),
    listProfiles: vi.fn(),
    deleteProfile: vi.fn(),
  },
}));

vi.mock('../../../services/profileService', () => ({
  profileService: {
    getUserId: vi.fn(),
  },
}));

vi.mock('../../../services/marketService', () => ({
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

vi.mock('../../../services/backupService', () => ({
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

const mockProtectAction = vi.fn((callback: any) => callback('test-password'));

vi.mock('../../../context/SessionContext', () => ({
  useSession: vi.fn(() => ({
    protectAction: mockProtectAction,
    profileName: 'Alice',
    notifyLogout: vi.fn(),
    clearLocks: vi.fn(),
  })),
  useOptionalSession: vi.fn(() => ({
    protectAction: mockProtectAction,
    profileName: 'Alice',
    notifyLogout: vi.fn(),
    clearLocks: vi.fn(),
  })),
}));

vi.mock('../../standards/StandardsManager', () => ({
  StandardsManager: () => <div data-testid="standards-manager-mock">Standards Manager Mock</div>,
}));

vi.mock('../../../utils/log', () => ({
  logger: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

describe('SettingsView Component', () => {
  const createMockConfig = (): AppConfigDto => ({
    uiPreferences: {
      uiMode: 'expert',
      lastUsedDirectory: null,
      balanceViewMode: 'spendable',
      marketShowUnknown: true,
      maxStoredCards: 2000,
      marketSendCardOnPayment: false,
      marketSyncEnabled: true,
      shareMinLevel: 1,
      marketPollWifiOnly: false,
      wotShareViaSync: true,
      postAcceptFromContacts: true,
      mediaAutoLoadMaxHops: 2,
    },
    securityPolicy: {
      sessionTimeoutSeconds: 600,
      privacyDefault: 'ask',
      devModeUnlocked: false,
      autoBackupEnabled: true,
      syncBackupEnabled: true,
      backupSyncFolder: null,
      backupProtectionMode: null,
      ownNominatedLockServer: null,
      marketSelectedSyncs: [],
      publicSyncs: [],
    },
    lifecycle: {
      pendingRestoreL2Reconciliation: false,
      backupDirty: false,
      lastBackupAt: null,
      lastSyncBackupAt: null,
      l2ProbeCache: {},
      l2AnchorTemplatesMeta: [],
    },
  });

  const mockOnBack = vi.fn();
  let writeTextSpy: MockInstance;
  let currentConfig: AppConfigDto;

  const renderSettingsView = (initialMode: UiMode = 'expert') =>
    render(
      <UiModeProvider initialMode={initialMode}>
        <SettingsView onBack={mockOnBack} />
      </UiModeProvider>
    );

  beforeEach(() => {
    vi.clearAllMocks();
    mockChangeLanguage.mockClear();
    mockNavigate.mockClear();
    mockAppState = { view: 'settings' };
    localStorage.clear();
    currentConfig = createMockConfig();

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

    (settingsService.getConfig as Mock).mockImplementation(async () => ({
      uiPreferences: { ...currentConfig.uiPreferences },
      securityPolicy: { ...currentConfig.securityPolicy },
      lifecycle: { ...currentConfig.lifecycle },
    }));
    (settingsService.patchUiPreferences as Mock).mockImplementation(async (patch: any) => {
      currentConfig = {
        ...currentConfig,
        uiPreferences: { ...currentConfig.uiPreferences, ...patch },
      };
      return currentConfig.uiPreferences;
    });
    (settingsService.patchSecurityPolicy as Mock).mockImplementation(async (patch: any) => {
      currentConfig = {
        ...currentConfig,
        securityPolicy: { ...currentConfig.securityPolicy, ...patch },
      };
      return currentConfig.securityPolicy;
    });
    (authService.getLocalInstanceId as Mock).mockResolvedValue('test-device-id-1234');
    (authService.listProfiles as Mock).mockResolvedValue([
      { folderName: 'alice-folder', profileName: 'Alice', isEncrypted: true },
    ]);
    (authService.deleteProfile as Mock).mockResolvedValue(undefined);
    (profileService.getUserId as Mock).mockResolvedValue('did:key:z6MkuTestUserId');
  });

  it('renders loading state initially', () => {
    renderSettingsView();
    expect(screen.getByText(/Configuring Environment\.\.\./i)).toBeInTheDocument();
  });

  it('renders 4 tabs and starts on Profile tab', async () => {
    renderSettingsView();

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    expect(screen.getByText('Profile')).toBeInTheDocument();
    expect(screen.getByText('Security & Backup')).toBeInTheDocument();
    expect(screen.getByText('Sync Service')).toBeInTheDocument();
    expect(screen.getByText('Advanced')).toBeInTheDocument();
    
    // Check that unified profile/business card is rendered on Profile tab
    expect(screen.getByRole('button', { name: /Edit business card|Visitenkarte bearbeiten/i })).toBeInTheDocument();
  });

  it('clicking each of the 4 tabs displays its respective components', async () => {
    const user = userEvent.setup();
    renderSettingsView();

    await screen.findByText('Profile');
    expect(screen.getByRole('button', { name: /Edit business card|Visitenkarte bearbeiten/i })).toBeInTheDocument();

    // 1. Security & Backup tab
    const securityTab = screen.getByText('Security & Backup');
    await user.click(securityTab);
    expect(screen.queryByRole('button', { name: /Edit business card|Visitenkarte bearbeiten/i })).not.toBeInTheDocument();
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
    renderSettingsView();

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
    renderSettingsView();

    await screen.findByText('Profile');
    const advancedTab = screen.getByText('Advanced');
    await user.click(advancedTab);

    const copyButton = screen.getByRole('button', { name: /^Copy$/i });
    await user.click(copyButton);

    expect(writeTextSpy).toHaveBeenCalledWith('test-device-id-1234');
  });

  it('saves privacy immediately and saves session timeout only on explicit save click with Full access', async () => {
    const user = userEvent.setup();
    renderSettingsView();

    await screen.findByText('Profile');
    const securityTab = screen.getByText('Security & Backup');
    await user.click(securityTab);

    expect(screen.queryByRole('button', { name: /Save System Changes/i })).not.toBeInTheDocument();

    const stealthRadio = screen.getByLabelText(/Stealth/i);
    await user.click(stealthRadio);

    await waitFor(() => {
      expect(settingsService.patchSecurityPolicy).toHaveBeenCalledWith(
        { privacyDefault: 'stealth' },
        'test-password'
      );
      expect(mockProtectAction).toHaveBeenCalledWith(expect.any(Function), {
        requireFullAccess: true,
      });
    });

    (settingsService.patchSecurityPolicy as Mock).mockClear();

    // Typing session timeout does NOT save automatically
    const inputs = screen.getAllByRole('spinbutton');
    expect(inputs.length).toBeGreaterThanOrEqual(1);
    fireEvent.change(inputs[0], { target: { value: '15' } });
    expect(settingsService.patchSecurityPolicy).not.toHaveBeenCalled();

    // Clicking explicit save commits sessionTimeoutSeconds with Full access
    const saveTimeoutBtn = screen.getByTestId('save-timeout-button');
    expect(saveTimeoutBtn).not.toBeDisabled();
    await user.click(saveTimeoutBtn);

    await waitFor(() => {
      expect(settingsService.patchSecurityPolicy).toHaveBeenCalledWith(
        { sessionTimeoutSeconds: 900 },
        'test-password'
      );
      expect(mockProtectAction).toHaveBeenLastCalledWith(expect.any(Function), {
        requireFullAccess: true,
      });
    });
  });

  it('displays error message on failed auto-save', async () => {
    const user = userEvent.setup();
    (settingsService.patchSecurityPolicy as Mock).mockRejectedValue(new Error('Save failed'));
    renderSettingsView();

    await screen.findByText('Profile');
    const securityTab = screen.getByText('Security & Backup');
    await user.click(securityTab);

    const stealthRadio = screen.getByLabelText(/Stealth/i);
    await user.click(stealthRadio);

    expect(await screen.findByText(/Failed to save settings: Save failed/i)).toBeInTheDocument();
  });

  it('calls onBack prop when back button clicked', async () => {
    const user = userEvent.setup();
    renderSettingsView();

    await screen.findByText('Profile');
    
    // Find back button in PageLayout
    const backButton = screen.getByRole('button', { name: /Back/i });
    await user.click(backButton);

    expect(mockOnBack).toHaveBeenCalled();
  });

  it('renders language selector card on Profile tab and allows changing language', async () => {
    const user = userEvent.setup();
    const setItemSpy = vi.spyOn(Storage.prototype, 'setItem');
    renderSettingsView();

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
    renderSettingsView();

    await screen.findByText('Profile');
    const securityTab = screen.getByText('Security & Backup');
    await user.click(securityTab);

    expect(screen.getByText('Danger Zone')).toBeInTheDocument();
    const deleteButton = screen.getByRole('button', { name: /Delete Profile on this Device/i });
    await user.click(deleteButton);

    // Confirmation modal is open; confirm stays disabled until explicit password is entered
    expect(screen.getByText('Delete Profile Permanently?')).toBeInTheDocument();
    expect(screen.getByText(/All local keys, vouchers, and settings for this profile will be permanently deleted/i)).toBeInTheDocument();

    const confirmBtn = screen.getByRole('button', { name: /^Delete$/i });
    expect(confirmBtn).toBeDisabled();
    await user.type(screen.getByPlaceholderText(/Enter password/i), 'test-password');
    expect(confirmBtn).not.toBeDisabled();
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
    renderSettingsView();

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    // Should be on Security & Backup tab
    expect(document.getElementById('settings-backup-section')).toBeInTheDocument();
  });

  it('deep-links to Sync Service when initialSection is sync', async () => {
    mockAppState = { view: 'settings', initialSection: 'sync' };
    renderSettingsView();

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    // Should be on Sync Service tab
    expect(document.getElementById('settings-sync-section')).toBeInTheDocument();
  });

  it('deep-links to Advanced when initialSection is network', async () => {
    mockAppState = { view: 'settings', initialSection: 'network' };
    renderSettingsView();

    await waitFor(() => {
      expect(screen.queryByText(/Configuring Environment\.\.\./i)).not.toBeInTheDocument();
    });

    // Should be on Advanced tab
    expect(document.getElementById('settings-network-section')).toBeInTheDocument();
  });

  it('renders unified 6-block layout without 4-tab bar or save button in simple mode', async () => {
    renderSettingsView('simple');

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
    const user = userEvent.setup();
    renderSettingsView('simple');

    const masterToggle = await screen.findByTestId('auto-sync-master-toggle');
    expect(masterToggle).toBeChecked();

    await user.click(masterToggle);

    await waitFor(() => {
      expect(settingsService.patchSecurityPolicy).toHaveBeenCalledWith(
        { syncBackupEnabled: false },
        'test-password'
      );
      expect(settingsService.patchUiPreferences).toHaveBeenCalledWith(
        {
          postAcceptFromContacts: false,
          marketSyncEnabled: false,
          wotShareViaSync: false,
        },
        'test-password'
      );
      expect(mockProtectAction).toHaveBeenCalledWith(expect.any(Function), {
        requireFullAccess: true,
      });
    });

    await user.click(screen.getByTestId('auto-sync-master-toggle'));

    await waitFor(() => {
      expect(settingsService.patchSecurityPolicy).toHaveBeenCalledWith(
        { syncBackupEnabled: true },
        'test-password'
      );
      expect(settingsService.patchUiPreferences).toHaveBeenCalledWith(
        {
          postAcceptFromContacts: true,
          marketSyncEnabled: true,
          wotShareViaSync: true,
        },
        'test-password'
      );
    });
  });

  it('H3: requires master password for deleteProfile, skips protectAction, and keeps modal open on wrong password', async () => {
    const user = userEvent.setup();
    (authService.deleteProfile as Mock).mockRejectedValueOnce(new Error('Incorrect password'));
    mockAppState = { view: 'settings', initialSection: 'security' };
    renderSettingsView('simple');

    const deleteButton = await screen.findByRole('button', { name: /Delete Profile on this Device/i });
    await user.click(deleteButton);

    expect(screen.getByText('Delete Profile Permanently?')).toBeInTheDocument();
    const confirmBtn = screen.getByRole('button', { name: /^Delete$/i });
    expect(confirmBtn).toBeDisabled();

    const pwInput = screen.getByPlaceholderText(/Enter password/i);
    await user.type(pwInput, 'wrong-secret');
    expect(confirmBtn).not.toBeDisabled();
    await user.click(confirmBtn);

    await waitFor(() => {
      expect(authService.deleteProfile).toHaveBeenCalledWith({
        folderName: 'alice-folder',
        password: 'wrong-secret',
      });
    });
    expect(mockProtectAction).not.toHaveBeenCalled();
    expect(screen.getByText('Delete Profile Permanently?')).toBeInTheDocument();
    expect(screen.getByText(/Incorrect password/i)).toBeInTheDocument();
  });
});
