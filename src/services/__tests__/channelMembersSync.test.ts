// src/services/__tests__/channelMembersSync.test.ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { triggerDebouncedChannelMembersSync } from '../marketService';
import { contactService } from '../contactService';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

vi.mock('../../utils/autoBackup', () => ({
    triggerAutoBackup: vi.fn(),
}));

describe('Debounced Channel Members Sync (Task 3.1)', () => {
    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it('coalesces rapid consecutive calls within 1.5s into a single upload run', async () => {
        vi.mocked(invoke).mockResolvedValue(1);

        // Rapidly trigger 5 times within 1 second
        triggerDebouncedChannelMembersSync(1500);
        await vi.advanceTimersByTimeAsync(200);
        triggerDebouncedChannelMembersSync(1500);
        await vi.advanceTimersByTimeAsync(300);
        triggerDebouncedChannelMembersSync(1500);
        await vi.advanceTimersByTimeAsync(400);
        triggerDebouncedChannelMembersSync(1500);
        await vi.advanceTimersByTimeAsync(100);
        triggerDebouncedChannelMembersSync(1500);

        // At this point (1000ms after first call, but only 100ms after last call), no invoke yet
        expect(invoke).not.toHaveBeenCalled();

        // Advance 1400ms (100ms + 1400ms = 1500ms since last trigger)
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
        await contactService.saveContact(dummyContact as any);
        expect(invoke).toHaveBeenCalledWith('save_contact', { contact: dummyContact, password: undefined });

        // Advance 1500ms
        await vi.advanceTimersByTimeAsync(1500);
        expect(invoke).toHaveBeenCalledWith('sync_channel_members', {});
    });
});
