import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  updateNetworkStatus,
  wakeSyncCoordinator,
  afterWalletMutation,
  triggerAutoBackup,
} from '../syncBridge';

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
        syncMembers: false,
      },
    });

    triggerAutoBackup(true);

    expect(intervalSpy).not.toHaveBeenCalled();
    intervalSpy.mockRestore();
  });

  it('afterWalletMutation notifies backend coordinator, UI events, and syncMembers without member timers', async () => {
    const contactsSpy = vi.fn();
    window.addEventListener('humoco:contacts-updated', contactsSpy);

    afterWalletMutation(null, {
      contactsUpdated: true,
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

    window.removeEventListener('humoco:contacts-updated', contactsSpy);
  });
});
