import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { AutoSyncSummaryCard } from '../AutoSyncSummaryCard';
import { settingsService } from '../../../services/settingsService';
import { AppConfigDto } from '../../../types';

vi.mock('../../../services/marketService', () => ({
  marketService: {
    discoverAvailableSyncs: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('../../../services/settingsService', () => ({
  settingsService: {
    getConfig: vi.fn(),
    patchSecurityPolicy: vi.fn(),
    patchUiPreferences: vi.fn(),
  },
}));

const mockProtectAction = vi.fn(async <T,>(cb: (pw?: string | null) => Promise<T>) =>
  cb('mock-wallet-pw')
);

vi.mock('../../../context/SessionContext', () => ({
  useOptionalSession: () => ({
    protectAction: mockProtectAction,
  }),
}));

describe('AutoSyncSummaryCard', () => {
  const defaultConfig: AppConfigDto = {
    uiPreferences: {
      uiMode: 'simple',
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
      sessionTimeoutSeconds: 300,
      privacyDefault: 'public',
      devModeUnlocked: false,
      autoBackupEnabled: true,
      syncBackupEnabled: true,
      backupSyncFolder: null,
      backupProtectionMode: null,
      marketSelectedSyncs: [],
      publicSyncs: [],
    },
    lifecycle: {
      pendingRestoreL2Reconciliation: false,
      backupDirty: false,
      lastBackupAt: null,
      lastSyncBackupAt: null,
    },
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('disables master toggle while in flight so rapid double clicks cannot race', async () => {
    let resolvePatchSec!: (val: AppConfigDto['securityPolicy']) => void;
    vi.mocked(settingsService.patchSecurityPolicy).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolvePatchSec = resolve;
        })
    );
    vi.mocked(settingsService.patchUiPreferences).mockResolvedValue({
      ...defaultConfig.uiPreferences,
      postAcceptFromContacts: false,
      marketSyncEnabled: false,
      wotShareViaSync: false,
    });

    const onConfigUpdated = vi.fn();
    render(<AutoSyncSummaryCard config={defaultConfig} onConfigUpdated={onConfigUpdated} />);

    const toggle = screen.getByTestId('auto-sync-master-toggle') as HTMLInputElement;
    expect(toggle).not.toBeDisabled();

    fireEvent.click(toggle);
    expect(toggle).toBeDisabled();

    fireEvent.click(toggle);
    expect(settingsService.patchSecurityPolicy).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolvePatchSec({
        ...defaultConfig.securityPolicy,
        syncBackupEnabled: false,
      });
    });

    await waitFor(() => {
      expect(toggle).not.toBeDisabled();
      expect(onConfigUpdated).toHaveBeenCalledTimes(1);
    });
  });
});
