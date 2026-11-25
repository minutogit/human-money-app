import { render, act, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SessionProvider } from '../SessionContext';
import { rateLimitManager } from '../../sync/rateLimitManager';
import { dispatchBackupUpdated } from '../../sync/syncBridge';
import { APP_EVENTS } from '../../utils/appEvents';
import { APP_NOTICES_UPDATED_EVENT, SYNC_STATUS_UPDATED_EVENT } from '../../utils/syncEvents';

const eventListeners = new Map<string, (ev: { payload: unknown }) => void>();

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((event: string, cb: (ev: { payload: unknown }) => void) => {
    eventListeners.set(event, cb);
    return Promise.resolve(() => {
      eventListeners.delete(event);
    });
  }),
}));

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('0.5.16'),
}));

vi.mock('../../services/authService', () => ({
  authService: {
    getSessionStatus: vi.fn().mockResolvedValue({
      isUnlocked: false,
      hasCachedSession: false,
      accessMode: 'full',
      baseAccessMode: 'full',
    }),
    lockSession: vi.fn().mockResolvedValue(undefined),
    refreshSessionActivity: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../../services/settingsService', () => ({
  settingsService: {
    getConfig: vi.fn().mockResolvedValue(null),
  },
}));

vi.mock('../../sync/syncBridge', () => ({
  updateNetworkStatus: vi.fn().mockResolvedValue(undefined),
  wakeSyncCoordinator: vi.fn().mockResolvedValue(undefined),
  triggerAutoBackup: vi.fn(),
  dispatchBackupUpdated: vi.fn(),
  afterWalletMutation: vi.fn(),
  runSyncBackupPass: vi.fn().mockResolvedValue(null),
  runPostSyncPass: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../hooks/useWindowTitleSync', () => ({
  useWindowTitleSync: vi.fn(),
}));

vi.mock('../../hooks/useIntegrityCheck', () => ({
  useIntegrityCheck: () => ({
    integrityReport: null,
    checkIntegrity: vi.fn().mockResolvedValue(undefined),
    resetIntegrity: vi.fn(),
  }),
}));

describe('SessionContext Tauri event bridge', () => {
  beforeEach(() => {
    eventListeners.clear();
    vi.clearAllMocks();
  });

  it('S10-X4: emits POST_INBOX_UPDATED, SYNC_COMPLETED, and WOT_UPDATED when humoco://post-synced reports importedWotLists > 0', async () => {
    render(
      <SessionProvider>
        <div />
      </SessionProvider>
    );

    await waitFor(() => {
      expect(eventListeners.has('humoco://post-synced')).toBe(true);
    });

    const received: string[] = [];
    const onPost = () => received.push(APP_EVENTS.POST_INBOX_UPDATED);
    const onSync = () => received.push(APP_EVENTS.SYNC_COMPLETED);
    const onWot = () => received.push(APP_EVENTS.WOT_UPDATED);
    window.addEventListener(APP_EVENTS.POST_INBOX_UPDATED, onPost);
    window.addEventListener(APP_EVENTS.SYNC_COMPLETED, onSync);
    window.addEventListener(APP_EVENTS.WOT_UPDATED, onWot);

    try {
      await act(async () => {
        eventListeners.get('humoco://post-synced')!({
          payload: {
            uploadedTo: 0,
            polledSyncs: 1,
            importedPayments: 0,
            importedReceipts: 0,
            importedSignatures: 0,
            newSignRequests: 0,
            newWotHandshakes: 0,
            importedWotLists: 1,
            newInquiries: 0,
            newInquiryMessages: 0,
            prunedBoxes: 0,
            errors: [],
          },
        });
      });

      expect(received).toContain(APP_EVENTS.POST_INBOX_UPDATED);
      expect(received).toContain(APP_EVENTS.SYNC_COMPLETED);
      expect(received).toContain(APP_EVENTS.WOT_UPDATED);
    } finally {
      window.removeEventListener(APP_EVENTS.POST_INBOX_UPDATED, onPost);
      window.removeEventListener(APP_EVENTS.SYNC_COMPLETED, onSync);
      window.removeEventListener(APP_EVENTS.WOT_UPDATED, onWot);
    }
  });

  it('S09-X3: triggers notifySyncStatusUpdated and SYNC_COMPLETED when humoco://market-synced reports revokedOwnSyncs', async () => {
    render(
      <SessionProvider>
        <div />
      </SessionProvider>
    );

    await waitFor(() => {
      expect(eventListeners.has('humoco://market-synced')).toBe(true);
    });

    const syncCompletedSources: Array<string | undefined> = [];
    let syncStatusUpdatedCount = 0;
    const onSyncStatus = () => {
      syncStatusUpdatedCount += 1;
    };
    const onSyncCompleted = (ev: Event) => {
      syncCompletedSources.push((ev as CustomEvent<{ source?: string } | undefined>).detail?.source);
    };
    window.addEventListener(SYNC_STATUS_UPDATED_EVENT, onSyncStatus);
    window.addEventListener(APP_EVENTS.SYNC_COMPLETED, onSyncCompleted);

    try {
      await act(async () => {
        eventListeners.get('humoco://market-synced')!({
          payload: {
            uploadedTo: 0,
            polledSyncs: 0,
            importedCards: 0,
            importedWotLists: 0,
            revokedOwnSyncs: [
              {
                writeUrl: 'https://revoked.example/sync.php',
                readUrl: 'https://revoked.example/data',
                dropPub: 'aa'.repeat(32),
                status: 'revoked',
              },
            ],
            errors: [],
          },
        });
      });

      expect(syncStatusUpdatedCount).toBe(1);
      expect(syncCompletedSources).toContain('market');
    } finally {
      window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, onSyncStatus);
      window.removeEventListener(APP_EVENTS.SYNC_COMPLETED, onSyncCompleted);
    }
  });

  it('bridges humoco://backup-updated, humoco://sync-status-updated with retryAfterSecs cooldown, and humoco://conflicts-updated', async () => {
    rateLimitManager.clearCooldown();
    render(
      <SessionProvider>
        <div />
      </SessionProvider>
    );

    await waitFor(() => {
      expect(eventListeners.has('humoco://backup-updated')).toBe(true);
      expect(eventListeners.has('humoco://sync-status-updated')).toBe(true);
      expect(eventListeners.has('humoco://conflicts-updated')).toBe(true);
    });

    let syncUpdated = 0;
    let conflictsUpdated = 0;
    const onSyncStatus = () => {
      syncUpdated += 1;
    };
    const onConflicts = () => {
      conflictsUpdated += 1;
    };
    window.addEventListener(SYNC_STATUS_UPDATED_EVENT, onSyncStatus);
    window.addEventListener(APP_EVENTS.CONFLICTS_UPDATED, onConflicts);

    try {
      await act(async () => {
        eventListeners.get('humoco://backup-updated')!({ payload: { dirty: false } });
        eventListeners.get('humoco://sync-status-updated')!({ payload: { retryAfterSecs: 45 } });
        eventListeners.get('humoco://conflicts-updated')!({ payload: null });
      });

      expect(dispatchBackupUpdated).toHaveBeenCalledWith({ dirty: false });
      expect(syncUpdated).toBe(1);
      expect(conflictsUpdated).toBe(1);
      expect(rateLimitManager.isCoolingDown()).toBe(true);
      expect(rateLimitManager.getRemainingSeconds()).toBeGreaterThanOrEqual(40);
    } finally {
      rateLimitManager.clearCooldown();
      window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, onSyncStatus);
      window.removeEventListener(APP_EVENTS.CONFLICTS_UPDATED, onConflicts);
    }
  });

  it('does not emit POST_INBOX_UPDATED or SYNC_COMPLETED when humoco://post-synced has zero inbox changes', async () => {
    render(
      <SessionProvider>
        <div />
      </SessionProvider>
    );

    await waitFor(() => {
      expect(eventListeners.has('humoco://post-synced')).toBe(true);
    });

    const received: string[] = [];
    const onPost = () => received.push(APP_EVENTS.POST_INBOX_UPDATED);
    const onSync = () => received.push(APP_EVENTS.SYNC_COMPLETED);
    const onNotices = () => received.push(APP_NOTICES_UPDATED_EVENT);
    window.addEventListener(APP_EVENTS.POST_INBOX_UPDATED, onPost);
    window.addEventListener(APP_EVENTS.SYNC_COMPLETED, onSync);
    window.addEventListener(APP_NOTICES_UPDATED_EVENT, onNotices);

    try {
      await act(async () => {
        eventListeners.get('humoco://post-synced')!({
          payload: {
            uploadedTo: 0,
            polledSyncs: 0,
            importedPayments: 0,
            importedReceipts: 0,
            importedSignatures: 0,
            newSignRequests: 0,
            newWotHandshakes: 0,
            importedWotLists: 0,
            newInquiries: 0,
            newInquiryMessages: 0,
            prunedBoxes: 0,
            errors: [],
          },
        });
      });

      expect(received).toEqual([]);

      await act(async () => {
        eventListeners.get('humoco://post-synced')!({
          payload: {
            uploadedTo: 0,
            polledSyncs: 1,
            importedPayments: 0,
            importedReceipts: 0,
            importedSignatures: 0,
            newSignRequests: 0,
            newWotHandshakes: 0,
            importedWotLists: 0,
            newInquiries: 0,
            newInquiryMessages: 0,
            prunedBoxes: 0,
            errors: [],
          },
        });
      });

      expect(received).toEqual([APP_NOTICES_UPDATED_EVENT]);
    } finally {
      window.removeEventListener(APP_EVENTS.POST_INBOX_UPDATED, onPost);
      window.removeEventListener(APP_EVENTS.SYNC_COMPLETED, onSync);
      window.removeEventListener(APP_NOTICES_UPDATED_EVENT, onNotices);
    }
  });

  it('emits MARKET_CARD_UPDATED, CONTACTS_UPDATED, and WOT_UPDATED selectively on humoco://market-synced', async () => {
    render(
      <SessionProvider>
        <div />
      </SessionProvider>
    );

    await waitFor(() => {
      expect(eventListeners.has('humoco://market-synced')).toBe(true);
    });

    const received: string[] = [];
    const onCard = () => received.push(APP_EVENTS.MARKET_CARD_UPDATED);
    const onContacts = () => received.push(APP_EVENTS.CONTACTS_UPDATED);
    const onWot = () => received.push(APP_EVENTS.WOT_UPDATED);
    const onSync = () => received.push(APP_EVENTS.SYNC_COMPLETED);
    window.addEventListener(APP_EVENTS.MARKET_CARD_UPDATED, onCard);
    window.addEventListener(APP_EVENTS.CONTACTS_UPDATED, onContacts);
    window.addEventListener(APP_EVENTS.WOT_UPDATED, onWot);
    window.addEventListener(APP_EVENTS.SYNC_COMPLETED, onSync);

    try {
      await act(async () => {
        eventListeners.get('humoco://market-synced')!({
          payload: {
            uploadedTo: 0,
            polledSyncs: 0,
            importedCards: 0,
            newAds: 1,
            importedWotLists: 0,
            errors: [],
          },
        });
      });

      expect(received).toEqual([APP_EVENTS.MARKET_CARD_UPDATED]);
      received.length = 0;

      await act(async () => {
        eventListeners.get('humoco://market-synced')!({
          payload: {
            uploadedTo: 0,
            polledSyncs: 1,
            importedCards: 1,
            newAds: 0,
            importedWotLists: 1,
            errors: [],
          },
        });
      });

      expect(received).toContain(APP_EVENTS.MARKET_CARD_UPDATED);
      expect(received).toContain(APP_EVENTS.CONTACTS_UPDATED);
      expect(received).toContain(APP_EVENTS.WOT_UPDATED);
      expect(received).toContain(APP_EVENTS.SYNC_COMPLETED);
    } finally {
      window.removeEventListener(APP_EVENTS.MARKET_CARD_UPDATED, onCard);
      window.removeEventListener(APP_EVENTS.CONTACTS_UPDATED, onContacts);
      window.removeEventListener(APP_EVENTS.WOT_UPDATED, onWot);
      window.removeEventListener(APP_EVENTS.SYNC_COMPLETED, onSync);
    }
  });

  it('cleans up all registered Tauri event listeners on unmount', async () => {
    const { unmount } = render(
      <SessionProvider>
        <div />
      </SessionProvider>
    );

    await waitFor(() => {
      expect(eventListeners.size).toBe(5);
    });

    unmount();
    expect(eventListeners.size).toBe(0);
  });
});
