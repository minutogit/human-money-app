import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { backupService } from '../../services/backupService';
import { postService } from '../../services/postService';
import { rateLimitManager } from '../rateLimitManager';
import {
  BACKUP_UPDATED_EVENT,
  updateNetworkStatus,
  wakeSyncCoordinator,
  afterWalletMutation,
  triggerAutoBackup,
  runSyncBackupPass,
  runPostSyncPass,
} from '../syncBridge';
import { APP_EVENTS } from '../../utils/appEvents';
import { SYNC_STATUS_UPDATED_EVENT } from '../../utils/syncEvents';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../services/backupService', () => ({
  backupService: {
    syncBackup: vi.fn(),
  },
}));

vi.mock('../../services/postService', () => ({
  postService: {
    syncPostChannels: vi.fn(),
  },
}));

describe('syncBridge (H10 thin backend coordinator bridge)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockResolvedValue(undefined);
    rateLimitManager.clearCooldown();
  });

  it('forwards network/visibility state to backend coordinator', async () => {
    await updateNetworkStatus(true, false, true);
    expect(invoke).toHaveBeenCalledWith('update_network_status', {
      isOnline: true,
      isWifi: false,
      isVisible: true,
    });
  });

  it('wakes backend sync coordinator for requested scopes without scheduling TS timers', async () => {
    const intervalSpy = vi.spyOn(window, 'setInterval');
    try {
      await wakeSyncCoordinator({ post: true, market: true });
      expect(invoke).toHaveBeenCalledWith('wake_sync_coordinator', {
        scopes: {
          seal: false,
          localBackup: false,
          syncBackup: false,
          syncBackupForce: false,
          post: true,
          market: true,
          marketForceUpload: false,
          syncMembers: false,
        },
      });

      triggerAutoBackup(true);

      expect(intervalSpy).not.toHaveBeenCalled();
    } finally {
      intervalSpy.mockRestore();
    }
  });

  it('afterWalletMutation notifies backend coordinator, UI events, and syncMembers without member timers', async () => {
    const contactsSpy = vi.fn();
    const wotSpy = vi.fn();
    window.addEventListener(APP_EVENTS.CONTACTS_UPDATED, contactsSpy);
    window.addEventListener(APP_EVENTS.WOT_UPDATED, wotSpy);

    try {
      afterWalletMutation(null, {
        contactsUpdated: true,
        wotUpdated: true,
        scheduleMarket: true,
        syncMembers: true,
      });

      await Promise.resolve();
      expect(invoke).toHaveBeenCalledWith('wake_sync_coordinator', {
        scopes: {
          seal: false,
          localBackup: true,
          syncBackup: true,
          syncBackupForce: true,
          post: false,
          market: true,
          marketForceUpload: true,
          syncMembers: true,
        },
      });
      expect(contactsSpy).toHaveBeenCalledTimes(1);
      expect(wotSpy).toHaveBeenCalledTimes(1);

      vi.mocked(invoke).mockClear();
      afterWalletMutation();
      expect(contactsSpy).toHaveBeenCalledTimes(1);
      expect(wotSpy).toHaveBeenCalledTimes(1);
    } finally {
      window.removeEventListener(APP_EVENTS.CONTACTS_UPDATED, contactsSpy);
      window.removeEventListener(APP_EVENTS.WOT_UPDATED, wotSpy);
    }
  });

  it('runSyncBackupPass respects cooldown unless force=true, dispatches events, and records retryAfterSecs cooldown', async () => {
    const backupSpy = vi.fn();
    const statusSpy = vi.fn();
    window.addEventListener(BACKUP_UPDATED_EVENT, backupSpy);
    window.addEventListener(SYNC_STATUS_UPDATED_EVENT, statusSpy);

    try {
      rateLimitManager.setCooldown(30);
      expect(await runSyncBackupPass(false)).toBeNull();
      expect(backupService.syncBackup).not.toHaveBeenCalled();

      const forcedResult = {
        uploadedFiles: 1,
        downloadedFiles: 0,
        conflictsResolved: 0,
        errors: [],
        retryAfterSecs: 90,
      };
      vi.mocked(backupService.syncBackup).mockResolvedValueOnce(forcedResult as never);
      expect(await runSyncBackupPass(true)).toEqual(forcedResult);
      expect(backupService.syncBackup).toHaveBeenCalledTimes(1);
      expect(backupSpy).toHaveBeenCalledTimes(1);
      expect(statusSpy).toHaveBeenCalledTimes(1);
      expect(rateLimitManager.getRemainingSeconds()).toBeGreaterThanOrEqual(85);

      rateLimitManager.clearCooldown();
      vi.mocked(backupService.syncBackup).mockRejectedValueOnce({
        code: 'error.sync.rateLimited',
        message: 'HTTP 429',
        retryAfterSecs: 120,
      });
      expect(await runSyncBackupPass(false)).toBeNull();
      expect(rateLimitManager.isCoolingDown()).toBe(true);
      expect(rateLimitManager.getRemainingSeconds()).toBeGreaterThanOrEqual(115);
    } finally {
      rateLimitManager.clearCooldown();
      window.removeEventListener(BACKUP_UPDATED_EVENT, backupSpy);
      window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, statusSpy);
    }
  });

  it('runPostSyncPass skips when cooling down, invokes syncPostChannels, and applies retryAfterSecs cooldown', async () => {
    try {
      rateLimitManager.setCooldown(25);
      await runPostSyncPass();
      expect(postService.syncPostChannels).not.toHaveBeenCalled();

      rateLimitManager.clearCooldown();
      vi.mocked(postService.syncPostChannels).mockResolvedValueOnce({
        uploadedTo: 1,
        polledSyncs: 1,
        importedPayments: 0,
        importedReceipts: 0,
        importedSignatures: 0,
        newSignRequests: 0,
        newWotHandshakes: 0,
        newInquiries: 0,
        newInquiryMessages: 0,
        prunedBoxes: 0,
        errors: [],
        retryAfterSecs: 75,
      });
      await runPostSyncPass();
      expect(postService.syncPostChannels).toHaveBeenCalledTimes(1);
      expect(rateLimitManager.getRemainingSeconds()).toBeGreaterThanOrEqual(70);

      rateLimitManager.clearCooldown();
      vi.mocked(postService.syncPostChannels).mockRejectedValueOnce({
        code: 'error.sync.rateLimited',
        message: 'Retry-After: 50s',
      });
      await runPostSyncPass();
      expect(rateLimitManager.getRemainingSeconds()).toBeGreaterThanOrEqual(45);
    } finally {
      rateLimitManager.clearCooldown();
    }
  });

  it('updateNetworkStatus and wakeSyncCoordinator swallow invoke rejections without throwing', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('No Tauri runtime'));
    await expect(updateNetworkStatus(true, true, true)).resolves.toBeUndefined();
    await expect(wakeSyncCoordinator({ post: true })).resolves.toBeUndefined();
  });
});
