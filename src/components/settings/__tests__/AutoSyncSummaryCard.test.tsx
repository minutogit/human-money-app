import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { AutoSyncSummaryCard } from '../AutoSyncSummaryCard';
import { settingsService } from '../../../services/settingsService';
import { marketService } from '../../../services/marketService';
import { openUrl } from '@tauri-apps/plugin-opener';
import { copyToClipboard } from '../../../utils/clipboard';
import { AppConfigDto } from '../../../types';

vi.mock('../../../services/marketService', () => ({
  marketService: {
    discoverAvailableSyncs: vi.fn().mockResolvedValue([]),
    probePublicSync: vi.fn(),
    savePublicSync: vi.fn(),
  },
}));

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../utils/clipboard', () => ({
  copyToClipboard: vi.fn().mockResolvedValue(true),
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
    vi.mocked(marketService.discoverAvailableSyncs).mockResolvedValue([]);
    vi.mocked(settingsService.getConfig).mockResolvedValue(defaultConfig);
    vi.mocked(openUrl).mockResolvedValue(undefined);
    vi.mocked(copyToClipboard).mockResolvedValue(true);
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
    expect(settingsService.patchSecurityPolicy).toHaveBeenCalledWith(
      { syncBackupEnabled: false },
      'mock-wallet-pw'
    );

    await act(async () => {
      resolvePatchSec({
        ...defaultConfig.securityPolicy,
        syncBackupEnabled: false,
      });
    });

    await waitFor(() => {
      expect(settingsService.patchUiPreferences).toHaveBeenCalledWith(
        {
          postAcceptFromContacts: false,
          marketSyncEnabled: false,
          wotShareViaSync: false,
        },
        'mock-wallet-pw'
      );
      expect(toggle).not.toBeDisabled();
      expect(onConfigUpdated).toHaveBeenCalledTimes(1);
    });
  });

  it('re-enables master toggle, renders error feedback, and refreshes config when master toggle patch fails', async () => {
    const refreshedConfig: AppConfigDto = {
      ...defaultConfig,
      securityPolicy: { ...defaultConfig.securityPolicy, syncBackupEnabled: true },
    };
    vi.mocked(settingsService.patchSecurityPolicy).mockRejectedValue({
      code: 'error.auth.sessionTimeout',
      message: 'raw session error',
    });
    vi.mocked(settingsService.getConfig).mockResolvedValue(refreshedConfig);

    const onConfigUpdated = vi.fn();
    render(<AutoSyncSummaryCard config={defaultConfig} onConfigUpdated={onConfigUpdated} />);

    const toggle = screen.getByTestId('auto-sync-master-toggle') as HTMLInputElement;
    fireEvent.click(toggle);

    await waitFor(() => {
      expect(screen.getByText('Session timed out. Please provide your password.')).toBeInTheDocument();
      expect(toggle).not.toBeDisabled();
      expect(onConfigUpdated).toHaveBeenCalledWith(refreshedConfig);
    });
    expect(screen.queryByText('raw session error')).not.toBeInTheDocument();
  });

  it('excludes revoked, disabled, and unauthorized syncs from connected hosts and deduplicates shared hostnames', async () => {
    vi.mocked(marketService.discoverAvailableSyncs).mockResolvedValue([
      {
        descriptor: {
          writeUrl: 'https://unauthorized.example/sync.php',
          readUrl: 'https://unauthorized.example/data',
          dropPub: '11'.repeat(32),
          status: 'pending_auth',
          failCount: 0,
        },
        isAuthorized: false,
        isSelected: false,
        sourceLabel: 'Bob',
        status: 'pending_auth',
      },
      {
        descriptor: {
          writeUrl: 'https://revoked-friend.example/sync.php',
          readUrl: 'https://revoked-friend.example/data',
          dropPub: '22'.repeat(32),
          status: 'revoked',
          failCount: 0,
        },
        isAuthorized: true,
        isSelected: true,
        sourceLabel: 'Carol',
        status: 'revoked',
      },
      {
        descriptor: {
          writeUrl: 'https://shared.example/friend/sync.php',
          readUrl: 'https://shared.example/friend/data',
          dropPub: '33'.repeat(32),
          status: 'active',
          failCount: 0,
        },
        isAuthorized: true,
        isSelected: true,
        sourceLabel: 'Dave',
        status: 'active',
      },
      {
        descriptor: {
          writeUrl: 'https://second.example/sync.php',
          readUrl: 'https://second.example/data',
          dropPub: '44'.repeat(32),
          status: 'active',
          failCount: 0,
        },
        isAuthorized: true,
        isSelected: true,
        sourceLabel: 'Eve',
        status: 'active',
      },
    ]);

    const config: AppConfigDto = {
      ...defaultConfig,
      securityPolicy: {
        ...defaultConfig.securityPolicy,
        publicSyncs: [
          {
            writeUrl: 'https://disabled.example/sync.php',
            readUrl: 'https://disabled.example/data',
            dropPub: 'aa'.repeat(32),
            accessPubkey: 'bb'.repeat(32),
            enabled: false,
            authorizedChannels: ['card'],
            status: 'active',
          },
          {
            writeUrl: 'https://revoked-public.example/sync.php',
            readUrl: 'https://revoked-public.example/data',
            dropPub: 'cc'.repeat(32),
            accessPubkey: 'dd'.repeat(32),
            enabled: true,
            authorizedChannels: ['card'],
            status: 'revoked',
          },
          {
            writeUrl: 'https://shared.example/public/sync.php',
            readUrl: 'https://shared.example/public/data',
            dropPub: 'ee'.repeat(32),
            accessPubkey: 'ff'.repeat(32),
            enabled: true,
            authorizedChannels: ['card'],
            status: 'active',
          },
        ],
      },
    };

    render(<AutoSyncSummaryCard config={config} onConfigUpdated={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByTestId('auto-sync-connection-status')).toHaveTextContent(
        'Connected to shared.example (+1 more)'
      );
    });
    expect(screen.queryByTestId('auto-sync-quick-connect-btn')).not.toBeInTheDocument();
  });

  it('shows partial-active custom badge and unchecked master toggle when only a subset of channels is enabled', () => {
    const partialConfig: AppConfigDto = {
      ...defaultConfig,
      securityPolicy: {
        ...defaultConfig.securityPolicy,
        syncBackupEnabled: false,
      },
    };

    render(<AutoSyncSummaryCard config={partialConfig} onConfigUpdated={vi.fn()} />);

    expect(screen.getByTestId('auto-sync-master-toggle')).not.toBeChecked();
    expect(screen.getByTestId('auto-sync-custom-badge')).toHaveTextContent('Partially active');
  });

  it('blocks unsafe non-https registerUrl schemes and supports safe register, copy key, and auth recheck', async () => {
    const unsafeConfig: AppConfigDto = {
      ...defaultConfig,
      securityPolicy: {
        ...defaultConfig.securityPolicy,
        publicSyncs: [
          {
            writeUrl: 'https://community.example/sync.php',
            readUrl: 'https://community.example/data',
            dropPub: 'aa'.repeat(32),
            registerUrl: 'javascript:alert(1)',
            accessPubkey: 'key-123',
            enabled: false,
            authorizedChannels: [],
            status: 'pending_auth',
          },
        ],
      },
    };

    const { rerender } = render(<AutoSyncSummaryCard config={unsafeConfig} onConfigUpdated={vi.fn()} />);
    expect(screen.getByTestId('auto-sync-pending-auth-actions')).toBeInTheDocument();
    expect(screen.queryByTestId('auto-sync-open-register-btn')).not.toBeInTheDocument();

    const safeConfig: AppConfigDto = {
      ...defaultConfig,
      securityPolicy: {
        ...defaultConfig.securityPolicy,
        publicSyncs: [
          {
            writeUrl: 'https://community.example/sync.php',
            readUrl: 'https://community.example/data',
            dropPub: 'aa'.repeat(32),
            registerUrl: 'https://community.example/register',
            accessPubkey: 'key-123',
            enabled: false,
            authorizedChannels: [],
            status: 'pending_auth',
          },
        ],
      },
    };

    vi.mocked(marketService.probePublicSync).mockResolvedValue({
      writeUrl: 'https://community.example/sync.php',
      readUrl: 'https://community.example/data',
      dropPub: 'aa'.repeat(32),
      registerUrl: 'https://community.example/register',
      accessPubkey: 'key-123',
      authorizedChannels: ['card', 'post'],
    });
    vi.mocked(marketService.savePublicSync).mockResolvedValue({
      writeUrl: 'https://community.example/sync.php',
      readUrl: 'https://community.example/data',
      dropPub: 'aa'.repeat(32),
      registerUrl: 'https://community.example/register',
      accessPubkey: 'key-123',
      authorizedChannels: ['card', 'post'],
    });

    const onConfigUpdated = vi.fn();
    rerender(<AutoSyncSummaryCard config={safeConfig} onConfigUpdated={onConfigUpdated} />);

    fireEvent.click(screen.getByTestId('auto-sync-open-register-btn'));
    await waitFor(() => {
      expect(openUrl).toHaveBeenCalledWith('https://community.example/register?key=key-123');
    });

    fireEvent.click(screen.getByTestId('auto-sync-copy-access-key-btn'));
    await waitFor(() => {
      expect(copyToClipboard).toHaveBeenCalledWith('key-123');
    });

    fireEvent.click(screen.getByTestId('auto-sync-recheck-auth-btn'));
    await waitFor(() => {
      expect(marketService.probePublicSync).toHaveBeenCalledWith(
        'https://community.example/sync.php',
        'mock-wallet-pw'
      );
      expect(marketService.savePublicSync).toHaveBeenCalledWith(
        expect.objectContaining({
          writeUrl: 'https://community.example/sync.php',
          enabled: true,
        }),
        'mock-wallet-pw'
      );
      expect(screen.getByText('Sync services successfully probed.')).toBeInTheDocument();
    });
  });

  it('renders revocation alert with replacement URL and adopts replacement endpoint', async () => {
    const revokedConfig: AppConfigDto = {
      ...defaultConfig,
      securityPolicy: {
        ...defaultConfig.securityPolicy,
        publicSyncs: [
          {
            writeUrl: 'https://old.example/sync.php',
            readUrl: 'https://old.example/data',
            dropPub: 'aa'.repeat(32),
            accessPubkey: 'bb'.repeat(32),
            enabled: false,
            authorizedChannels: [],
            status: 'revoked',
            revocationReason: 'Host shutting down',
            replacement: {
              writeUrl: 'https://new.example/sync.php',
              readUrl: 'https://new.example/data',
              dropPub: 'cc'.repeat(32),
            },
          },
        ],
      },
    };

    vi.mocked(marketService.probePublicSync).mockResolvedValue({
      writeUrl: 'https://new.example/sync.php',
      readUrl: 'https://new.example/data',
      dropPub: 'cc'.repeat(32),
      accessPubkey: 'bb'.repeat(32),
      authorizedChannels: ['card'],
    });
    vi.mocked(marketService.savePublicSync).mockResolvedValue({
      writeUrl: 'https://new.example/sync.php',
      readUrl: 'https://new.example/data',
      dropPub: 'cc'.repeat(32),
      accessPubkey: 'bb'.repeat(32),
      authorizedChannels: ['card'],
    });

    render(<AutoSyncSummaryCard config={revokedConfig} onConfigUpdated={vi.fn()} />);

    const alert = screen.getByTestId('auto-sync-revocation-alert');
    expect(alert).toHaveTextContent('Host shutting down');
    expect(alert).toHaveTextContent('https://new.example/sync.php');

    fireEvent.click(screen.getByRole('button', { name: 'Use new sync service' }));
    await waitFor(() => {
      expect(marketService.probePublicSync).toHaveBeenCalledWith(
        'https://new.example/sync.php',
        'mock-wallet-pw'
      );
      expect(marketService.savePublicSync).toHaveBeenCalledWith(
        expect.objectContaining({
          writeUrl: 'https://new.example/sync.php',
          enabled: true,
        }),
        'mock-wallet-pw'
      );
    });
  });

  it('patches WiFi-only toggle and media auto-load max hops', async () => {
    vi.mocked(settingsService.patchUiPreferences).mockImplementation(async (patch) => ({
      ...defaultConfig.uiPreferences,
      ...patch,
    }));
    const onConfigUpdated = vi.fn();

    render(<AutoSyncSummaryCard config={defaultConfig} onConfigUpdated={onConfigUpdated} />);

    fireEvent.click(screen.getByTestId('settings-wifi-only-toggle'));
    await waitFor(() => {
      expect(settingsService.patchUiPreferences).toHaveBeenCalledWith(
        { marketPollWifiOnly: true },
        'mock-wallet-pw'
      );
    });

    fireEvent.change(screen.getByTestId('settings-media-auto-load-select'), {
      target: { value: '1' },
    });
    await waitFor(() => {
      expect(settingsService.patchUiPreferences).toHaveBeenCalledWith(
        { mediaAutoLoadMaxHops: 1 },
        'mock-wallet-pw'
      );
    });
  });
});
