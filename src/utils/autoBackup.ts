// src/utils/autoBackup.ts
import { backupService } from '../services/backupService';
import { logger } from './log';
import { isBackendError, stringifyError } from './errorHelper';
import { FrontendAutoBackupStatusDto, FrontendDropBackupSyncResult } from '../types';
import { notifyDropStatusUpdated } from './dropEvents';

let backupInterval: number | null = null;
let initialTimeout: number | null = null;
let debounceTimer: number | null = null;
let dropDebounceTimer: number | null = null;
let isBackupRunning = false;
let isDropBackupRunning = false;

export const DROP_BACKUP_DEBOUNCE_MS = 30_000;

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
        if (result.status === 'synced' && (result.changedBlobs ?? 0) > 0) {
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
 * Runs a single drop backup pass if not already running.
 */
export async function runDropBackupPass(): Promise<FrontendDropBackupSyncResult | null> {
    if (isDropBackupRunning) {
        return null;
    }
    isDropBackupRunning = true;
    try {
        const result = await backupService.syncDropBackup();
        dispatchBackupUpdated();
        notifyDropStatusUpdated();
        if (result && result.uploadedTo > 0) {
            logger.info(`Drop backup synced: uploaded to ${result.uploadedTo} drop(s), ${result.paddedBytes} bytes.`);
        }
        return result;
    } catch (e) {
        const msg = stringifyError(e);
        const code = isBackendError(e) ? e.code : '';
        const isExpected = code === 'error.wallet.locked' || code === 'error.auth.sessionTimeout'
            || msg.includes("Wallet is locked") || msg.includes("Session timed out")
            || msg.includes("Wallet service unavailable");
        if (!isExpected) {
            logger?.warn?.(`Drop backup background run encountered an issue: ${msg}`);
        }
        return null;
    } finally {
        isDropBackupRunning = false;
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
        runDropBackupPass();
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
    if (dropDebounceTimer) {
        window.clearTimeout(dropDebounceTimer);
        dropDebounceTimer = null;
    }
}

/**
 * Triggers debounced backup passes:
 * - 2s debounce for local USB/folder backup pass
 * - 30s debounce for off-site Drop backup pass (configurable parameter for tests)
 * Call this after successful mutations (creating vouchers, receiving, signing, contacts).
 */
export function triggerAutoBackup(delayMs = 2000, dropDelayMs = DROP_BACKUP_DEBOUNCE_MS) {
    if (debounceTimer) {
        window.clearTimeout(debounceTimer);
    }
    debounceTimer = window.setTimeout(async () => {
        debounceTimer = null;
        await runAutoBackupPass();
    }, delayMs);

    if (dropDebounceTimer) {
        window.clearTimeout(dropDebounceTimer);
    }
    dropDebounceTimer = window.setTimeout(async () => {
        dropDebounceTimer = null;
        await runDropBackupPass();
    }, dropDelayMs);
}
