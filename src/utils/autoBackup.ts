// src/utils/autoBackup.ts
import { backupService } from '../services/backupService';
import { logger } from './log';
import { isBackendError, stringifyError } from './errorHelper';
import { FrontendAutoBackupStatusDto, FrontendBackupSyncResult } from '../types';
import { notifySyncStatusUpdated } from './syncEvents';
import { rateLimitManager } from './rateLimitManager';

let backupInterval: number | null = null;
let initialTimeout: number | null = null;
let debounceTimer: number | null = null;
let syncDebounceTimer: number | null = null;
let isBackupRunning = false;
let isSyncBackupRunning = false;
let lastSyncBackupSuccessAt: number | null = null;

export const SYNC_BACKUP_DEBOUNCE_MS = 30_000;

/**
 * Event name dispatched when backup status updates.
 */
export const BACKUP_UPDATED_EVENT = 'humoco:backup-updated';

/**
 * Dispatches a custom event indicating the backup status has been refreshed.
 */
export function dispatchBackupUpdated(status?: FrontendAutoBackupStatusDto) {
    if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(BACKUP_UPDATED_EVENT, { detail: status }));
    }
}

/**
 * Runs a single auto backup pass if not already running.
 */
export async function runAutoBackupPass(): Promise<FrontendAutoBackupStatusDto | null> {
    if (isBackupRunning) {
        return null;
    }
    isBackupRunning = true;
    try {
        const result = await backupService.runAutoBackupSync();
        dispatchBackupUpdated(result);
        if ((result.status === 'ok' || result.status === 'synced') && (result.changedBlobs ?? 0) > 0) {
            logger.info(`Auto-backup synced successfully: ${result.changedBlobs} blobs updated.`);
        }
        return result;
    } catch (e) {
        const msg = stringifyError(e);
        const code = isBackendError(e) ? e.code : '';
        const isExpected = code === 'error.wallet.locked' || code === 'error.auth.sessionTimeout'
            || msg.includes("Wallet is locked") || msg.includes("Session timed out");
        if (!isExpected) {
            logger?.warn?.(`Auto-backup background run encountered an issue: ${msg}`);
        }
        return null;
    } finally {
        isBackupRunning = false;
    }
}

/**
 * Runs a single sync backup pass if not already running.
 * Point G.3: Background sync runs at most 1x daily (24h) unless `force` is true (after mutations).
 */
export async function runSyncBackupPass(force: boolean = false): Promise<FrontendBackupSyncResult | null> {
    if (isSyncBackupRunning) {
        return null;
    }
    if (rateLimitManager.isCoolingDown()) {
        logger.debug("Skipping sync backup pass due to active 429 rate limit cooldown.");
        return null;
    }
    if (!force && lastSyncBackupSuccessAt !== null) {
        const elapsed = Date.now() - lastSyncBackupSuccessAt;
        if (elapsed < 86400_000) {
            return null; // 24h background throttle
        }
    }

    isSyncBackupRunning = true;
    try {
        const result = await backupService.syncBackup();
        dispatchBackupUpdated();
        notifySyncStatusUpdated();
        if (result) {
            const retrySecs = rateLimitManager.extractRetryAfter(result);
            if (retrySecs) {
                rateLimitManager.setCooldown(retrySecs);
            }
            if (result.uploadedTo > 0) {
                lastSyncBackupSuccessAt = Date.now();
                logger.info(`Sync backup synced: uploaded to ${result.uploadedTo} sync endpoint(s), ${result.paddedBytes} bytes.`);
            }
        }
        return result;
    } catch (e) {
        const retrySecs = rateLimitManager.extractRetryAfter(e);
        if (retrySecs) {
            rateLimitManager.setCooldown(retrySecs);
        }
        const msg = stringifyError(e);
        const code = isBackendError(e) ? e.code : '';
        const isExpected = code === 'error.wallet.locked' || code === 'error.auth.sessionTimeout'
            || msg.includes("Wallet is locked") || msg.includes("Session timed out")
            || msg.includes("Wallet service unavailable");
        if (!isExpected) {
            logger?.warn?.(`Sync backup background run encountered an issue: ${msg}`);
        }
        return null;
    } finally {
        isSyncBackupRunning = false;
    }
}

/**
 * Starts the periodic auto backup loop.
 * Runs shortly after login (3s) and then every 60s.
 */
export function startAutoBackupLoop() {
    if (backupInterval || initialTimeout) return;

    logger.info("Starting auto-backup background loop.");

    // Initial check after 3 seconds
    initialTimeout = window.setTimeout(() => {
        initialTimeout = null;
        runAutoBackupPass();
        runSyncBackupPass();
    }, 3000);

    // Run every 60 seconds (catches reconnected USB drives)
    backupInterval = window.setInterval(runAutoBackupPass, 60000);
}

/**
 * Stops the periodic auto backup loop.
 */
export function stopAutoBackupLoop() {
    if (initialTimeout) {
        window.clearTimeout(initialTimeout);
        initialTimeout = null;
    }
    if (backupInterval) {
        window.clearInterval(backupInterval);
        backupInterval = null;
        logger.info("Stopped auto-backup background loop.");
    }
    if (debounceTimer) {
        window.clearTimeout(debounceTimer);
        debounceTimer = null;
    }
    if (syncDebounceTimer) {
        window.clearTimeout(syncDebounceTimer);
        syncDebounceTimer = null;
    }
    lastSyncBackupSuccessAt = null;
}

/**
 * Triggers debounced backup passes:
 * - 2s debounce for local USB/folder backup pass
 * - 30s debounce for off-site sync backup pass (configurable parameter for tests)
 * Call this after successful mutations (creating vouchers, receiving, signing, contacts).
 */
export function triggerAutoBackup(delayMs = 2000, syncDelayMs = SYNC_BACKUP_DEBOUNCE_MS) {
    if (debounceTimer) {
        window.clearTimeout(debounceTimer);
    }
    debounceTimer = window.setTimeout(async () => {
        debounceTimer = null;
        await runAutoBackupPass();
    }, delayMs);

    if (syncDebounceTimer) {
        window.clearTimeout(syncDebounceTimer);
    }
    syncDebounceTimer = window.setTimeout(async () => {
        syncDebounceTimer = null;
        await runSyncBackupPass(true);
    }, syncDelayMs);
}
