import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
    triggerAutoBackup,
    runAutoBackupPass,
    runDropBackupPass,
    startAutoBackupLoop,
    stopAutoBackupLoop,
    DROP_BACKUP_DEBOUNCE_MS,
    BACKUP_UPDATED_EVENT,
} from '../autoBackup';
import { backupService } from '../../services/backupService';
import { notifyDropStatusUpdated } from '../dropEvents';

vi.mock('../../services/backupService', () => ({
    backupService: {
        runAutoBackupSync: vi.fn(),
        syncDropBackup: vi.fn(),
    },
}));

vi.mock('../dropEvents', () => ({
    notifyDropStatusUpdated: vi.fn(),
}));

vi.mock('../log', () => ({
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
    });

    afterEach(() => {
        stopAutoBackupLoop();
        vi.useRealTimers();
    });

    it('runs single auto backup pass and dispatches update event', async () => {
        const mockResult = {
            status: 'synced' as const,
            syncFolder: '/media/my-drive',
            lastBackupAt: '2026-11-08T10:00:00Z',
            backupDirty: false,
            changedBlobs: 2,
        };
        vi.mocked(backupService.runAutoBackupSync).mockResolvedValue(mockResult);

        const eventListener = vi.fn();
        window.addEventListener(BACKUP_UPDATED_EVENT, eventListener);

        const res = await runAutoBackupPass();
        expect(res).toEqual(mockResult);
        expect(eventListener).toHaveBeenCalledTimes(1);

        window.removeEventListener(BACKUP_UPDATED_EVENT, eventListener);
    });

    it('runs single drop backup pass, dispatches update event and notifies drop event', async () => {
        const mockDropResult = {
            uploadedTo: 2,
            authorizedTargets: 2,
            errors: [],
            paddedBytes: 65536,
        };
        vi.mocked(backupService.syncDropBackup).mockResolvedValue(mockDropResult);

        const eventListener = vi.fn();
        window.addEventListener(BACKUP_UPDATED_EVENT, eventListener);

        const res = await runDropBackupPass();
        expect(res).toEqual(mockDropResult);
        expect(eventListener).toHaveBeenCalledTimes(1);
        expect(notifyDropStatusUpdated).toHaveBeenCalledTimes(1);

        window.removeEventListener(BACKUP_UPDATED_EVENT, eventListener);
    });

    it('debounces local pass with 2s and drop pass with 30s', async () => {
        vi.mocked(backupService.runAutoBackupSync).mockResolvedValue({
            status: 'synced',
            syncFolder: '/media/my-folder',
            lastBackupAt: '2026-11-08T10:00:00Z',
            backupDirty: false,
            changedBlobs: 1,
        });
        vi.mocked(backupService.syncDropBackup).mockResolvedValue({
            uploadedTo: 1,
            authorizedTargets: 1,
            errors: [],
            paddedBytes: 65536,
        });

        // Trigger multiple rapid mutations
        triggerAutoBackup(2000, DROP_BACKUP_DEBOUNCE_MS);
        await vi.advanceTimersByTimeAsync(500);
        triggerAutoBackup(2000, DROP_BACKUP_DEBOUNCE_MS);
        await vi.advanceTimersByTimeAsync(500);
        triggerAutoBackup(2000, DROP_BACKUP_DEBOUNCE_MS);

        expect(backupService.runAutoBackupSync).not.toHaveBeenCalled();
        expect(backupService.syncDropBackup).not.toHaveBeenCalled();

        // Advance 2s from last trigger
        await vi.advanceTimersByTimeAsync(2000);
        expect(backupService.runAutoBackupSync).toHaveBeenCalledTimes(1);
        expect(backupService.syncDropBackup).not.toHaveBeenCalled();

        // Advance remaining 28s to hit the 30s drop debounce
        await vi.advanceTimersByTimeAsync(28000);
        expect(backupService.syncDropBackup).toHaveBeenCalledTimes(1);
    });

    it('startAutoBackupLoop schedules initial 3s trigger and 60s interval', async () => {
        vi.mocked(backupService.runAutoBackupSync).mockResolvedValue({
            status: 'synced',
            syncFolder: '/media/my-folder',
            lastBackupAt: '2026-11-08T10:00:00Z',
            backupDirty: false,
            changedBlobs: 0,
        });
        vi.mocked(backupService.syncDropBackup).mockResolvedValue({
            uploadedTo: 1,
            authorizedTargets: 1,
            errors: [],
            paddedBytes: 65536,
        });

        startAutoBackupLoop();

        // Before 3s
        await vi.advanceTimersByTimeAsync(2900);
        expect(backupService.runAutoBackupSync).not.toHaveBeenCalled();
        expect(backupService.syncDropBackup).not.toHaveBeenCalled();

        // At 3s
        await vi.advanceTimersByTimeAsync(100);
        expect(backupService.runAutoBackupSync).toHaveBeenCalledTimes(1);
        expect(backupService.syncDropBackup).toHaveBeenCalledTimes(1);

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
        expect(backupService.syncDropBackup).not.toHaveBeenCalled();
    });
});
