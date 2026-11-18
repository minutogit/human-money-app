// src/sync/__tests__/channelMembersSync.test.ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { channelMembersSyncManager } from '../channelMembersSyncManager';
import { stopAutoBackupLoop } from '../autoBackup';
import { contactService } from '../../services/contactService';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

vi.mock('../../services/backupService', () => ({
    backupService: {
        runAutoBackupSync: vi.fn().mockResolvedValue({
            status: 'ok',
            lastBackupAt: null,
            backupDirty: false,
            syncFolder: null,
            changedBlobs: 0,
            syncBackupEnabled: false,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: false,
        }),
        syncBackup: vi.fn().mockResolvedValue({
            uploadedTo: 0,
            authorizedTargets: 0,
            errors: [],
            paddedBytes: 0,
        }),
    },
}));

describe('Debounced Channel Members Sync (Task 3.1)', () => {
    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();
        stopAutoBackupLoop();
        channelMembersSyncManager.reset();
    });

    afterEach(() => {
        stopAutoBackupLoop();
        channelMembersSyncManager.reset();
        vi.useRealTimers();
    });

    it('coalesces rapid consecutive calls within 1.5s into a single upload run', async () => {
        vi.mocked(invoke).mockResolvedValue(1);

        // Rapidly trigger 5 times within 1 second
        channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });
        await vi.advanceTimersByTimeAsync(200);
        channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });
        await vi.advanceTimersByTimeAsync(300);
        channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });
        await vi.advanceTimersByTimeAsync(400);
        channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });
        await vi.advanceTimersByTimeAsync(100);
        channelMembersSyncManager.trigger({ delayMs: 1500, resetQuota: true });

        // At this point (1000ms after first call, but only 100ms after last call), no invoke yet
        expect(invoke).not.toHaveBeenCalled();

        // Advance 1500ms since last trigger
        await vi.advanceTimersByTimeAsync(1500);

        // Must have been invoked exactly once!
        expect(invoke).toHaveBeenCalledTimes(1);
        expect(invoke).toHaveBeenCalledWith('sync_channel_members', {});
    });

    it('saving or deleting a contact triggers debounced channel members sync', async () => {
        vi.mocked(invoke).mockResolvedValue(undefined);

        const dummyContact = {
            did: 'did:key:zTestBob',
            profile: {
                id: 'did:key:zTestBob',
                firstName: 'Bob',
                lastName: 'Builder',
                address: {},
                gender: 'm',
            },
            tags: [],
            addedAt: '2026-01-01T00:00:00Z',
            trustLevel: 2,
        };

        // Saving contact
        await contactService.saveContact(dummyContact as never);
        expect(invoke).toHaveBeenCalledWith('save_contact', { contact: dummyContact, password: undefined });

        // Advance 1500ms
        await vi.advanceTimersByTimeAsync(1500);
        expect(invoke).toHaveBeenCalledWith('sync_channel_members', {});
    });
});
