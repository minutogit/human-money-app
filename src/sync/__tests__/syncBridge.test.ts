import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  updateNetworkStatus,
  wakeSyncCoordinator,
  afterWalletMutation,
  triggerAutoBackup,
  triggerPostSync,
  triggerMarketSync,
} from '../syncBridge';
import { channelMembersSyncManager } from '../channelMembersSyncManager';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

describe('syncBridge (H10 thin backend coordinator bridge)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
      },
    });

    triggerAutoBackup(true);
    triggerPostSync();
    triggerMarketSync(true);

    expect(intervalSpy).not.toHaveBeenCalled();
    intervalSpy.mockRestore();
  });

  it('afterWalletMutation notifies backend coordinator, UI events, and members flush', async () => {
    const flushSpy = vi.spyOn(channelMembersSyncManager, 'flushIfPending').mockResolvedValue();
    const contactsSpy = vi.fn();
    window.addEventListener('humoco:contacts-updated', contactsSpy);

    afterWalletMutation(null, {
      contactsUpdated: true,
      scheduleMarket: true,
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
      },
    });
    expect(contactsSpy).toHaveBeenCalledTimes(1);
    expect(flushSpy).toHaveBeenCalledTimes(1);

    window.removeEventListener('humoco:contacts-updated', contactsSpy);
    flushSpy.mockRestore();
  });
});
