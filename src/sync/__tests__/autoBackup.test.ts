// src/sync/__tests__/autoBackup.test.ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
    triggerAutoBackup,
    runAutoBackupPass,
    runSyncBackupPass,
    startAutoBackupLoop,
    stopAutoBackupLoop,
    afterWalletMutation,
    SYNC_BACKUP_DEBOUNCE_MS,
    BACKUP_UPDATED_EVENT,
} from '../autoBackup';
import { backupService } from '../../services/backupService';
import { notifySyncStatusUpdated } from '../../utils/syncEvents';
import { APP_EVENTS } from '../../utils/appEvents';
import { channelMembersSyncManager } from '../channelMembersSyncManager';
import * as marketSync from '../marketSync';

vi.mock('../../services/backupService', () => ({
    backupService: {
        runAutoBackupSync: vi.fn(),
        syncBackup: vi.fn(),
    },
}));

vi.mock('../../utils/syncEvents', () => ({
    notifySyncStatusUpdated: vi.fn(),
}));

vi.mock('../../utils/log', () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

describe('autoBackup utils', () => {
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

    it('runs single auto backup pass and dispatches update event', async () => {
        const mockResult = {
            status: 'ok' as const,
            syncFolder: '/media/my-drive',
            lastBackupAt: '2026-11-08T10:00:00Z',
            backupDirty: false,
            changedBlobs: 2,
            syncBackupEnabled: false,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: false,
        };
        vi.mocked(backupService.runAutoBackupSync).mockResolvedValue(mockResult);

        const eventListener = vi.fn();
        window.addEventListener(BACKUP_UPDATED_EVENT, eventListener);

        const res = await runAutoBackupPass();
        expect(res).toEqual(mockResult);
        expect(eventListener).toHaveBeenCalledTimes(1);

        window.removeEventListener(BACKUP_UPDATED_EVENT, eventListener);
    });

    it('runs single sync backup pass, dispatches update event and notifies sync event', async () => {
        const mockSyncResult = {
            uploadedTo: 2,
            authorizedTargets: 2,
            errors: [],
            paddedBytes: 65536,
        };
        vi.mocked(backupService.syncBackup).mockResolvedValue(mockSyncResult);

        const eventListener = vi.fn();
        window.addEventListener(BACKUP_UPDATED_EVENT, eventListener);

        const res = await runSyncBackupPass();
        expect(res).toEqual(mockSyncResult);
        expect(eventListener).toHaveBeenCalledTimes(1);
        expect(notifySyncStatusUpdated).toHaveBeenCalledTimes(1);

        window.removeEventListener(BACKUP_UPDATED_EVENT, eventListener);
    });

    it('debounces local pass with 2s and sync pass with 30s', async () => {
        vi.mocked(backupService.runAutoBackupSync).mockResolvedValue({
            status: 'ok',
            syncFolder: '/media/my-folder',
            lastBackupAt: '2026-11-08T10:00:00Z',
            backupDirty: false,
            changedBlobs: 1,
            syncBackupEnabled: false,
            lastSyncBackupAt: null,
            hasAuthorizedSyncBackupTarget: false,
        });
        vi.mocked(backupService.syncBackup).mockResolvedValue({
            uploadedTo: 1,
            authorizedTargets: 1,
            errors: [],
            paddedBytes: 65536,
        });

        // Trigger multiple rapid mutations
        triggerAutoBackup(2000, SYNC_BACKUP_DEBOUNCE_MS);
        await vi.advanceTimersByTimeAsync(500);
        triggerAutoBackup(2000, SYNC_BACKUP_DEBOUNCE_MS);
        await vi.advanceTimersByTimeAsync(500);
        triggerAutoBackup(2000, SYNC_BACKUP_DEBOUNCE_MS);

        expect(backupService.runAutoBackupSync).not.toHaveBeenCalled();
        expect(backupService.syncBackup).not.toHaveBeenCalled();

        // Advance 2s from last trigger
        await vi.advanceTimersByTimeAsync(2000);
        expect(backupService.runAutoBackupSync).toHaveBeenCalledTimes(1);
        expect(backupService.syncBackup).not.toHaveBeenCalled();

        // Advance remaining 28s to hit the 30s sync debounce
        await vi.advanceTimersByTimeAsync(28000);
        expect(backupService.syncBackup).toHaveBeenCalledTimes(1);
    });

    it('startAutoBackupLoop schedules initial 3s trigger and 60s interval', async () => {
        vi.mocked(backupService.runAutoBackupSync).mockResolvedValue({
            status: 'synced',
            syncFolder: '/media/my-folder',
            lastBackupAt: '2026-11-08T10:00:00Z',
            backupDirty: false,
            changedBlobs: 0,
            syncBackupEnabled: true,
            hasAuthorizedSyncBackupTarget: true,
        });
        vi.mocked(backupService.syncBackup).mockResolvedValue({
            uploadedTo: 1,
            authorizedTargets: 1,
            errors: [],
            paddedBytes: 65536,
        });

        startAutoBackupLoop();

        // Before 3s
        await vi.advanceTimersByTimeAsync(2900);
        expect(backupService.runAutoBackupSync).not.toHaveBeenCalled();
        expect(backupService.syncBackup).not.toHaveBeenCalled();

        // At 3s
        await vi.advanceTimersByTimeAsync(100);
        expect(backupService.runAutoBackupSync).toHaveBeenCalledTimes(1);
        expect(backupService.syncBackup).toHaveBeenCalledTimes(1);

        // At 63s (first 60s interval tick was at 60s)
        await vi.advanceTimersByTimeAsync(60000);
        expect(backupService.runAutoBackupSync).toHaveBeenCalledTimes(2);

        stopAutoBackupLoop();
    });

    it('stopAutoBackupLoop cancels pending debounces and intervals', async () => {
        triggerAutoBackup(2000, 30000);
        stopAutoBackupLoop();

        vi.advanceTimersByTime(60000);
        expect(backupService.runAutoBackupSync).not.toHaveBeenCalled();
        expect(backupService.syncBackup).not.toHaveBeenCalled();
    });

    it('afterWalletMutation emits requested events, syncs members, and schedules market onboarding', () => {
        const contactsSpy = vi.fn();
        const wotSpy = vi.fn();
        window.addEventListener(APP_EVENTS.CONTACTS_UPDATED, contactsSpy);
        window.addEventListener(APP_EVENTS.WOT_UPDATED, wotSpy);

        const triggerSpy = vi.spyOn(channelMembersSyncManager, 'trigger').mockImplementation(() => {});
        const marketSpy = vi.spyOn(marketSync, 'scheduleContactOnboardingMarketSync').mockImplementation(() => {});

        afterWalletMutation('pw-123', {
            syncMembers: true,
            scheduleMarket: true,
            contactsUpdated: true,
            wotUpdated: true,
        });

        expect(contactsSpy).toHaveBeenCalledTimes(1);
        expect(wotSpy).toHaveBeenCalledTimes(1);
        expect(triggerSpy).toHaveBeenCalledWith({
            delayMs: 250,
            resetQuota: true,
            password: 'pw-123',
        });
        expect(marketSpy).toHaveBeenCalledWith(marketSync.DEFAULT_ONBOARDING_DELAY_MS, 'pw-123');

        triggerSpy.mockRestore();
        marketSpy.mockRestore();
        window.removeEventListener(APP_EVENTS.CONTACTS_UPDATED, contactsSpy);
        window.removeEventListener(APP_EVENTS.WOT_UPDATED, wotSpy);
    });
});
