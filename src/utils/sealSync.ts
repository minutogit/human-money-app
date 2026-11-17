import { settingsService } from '../services/settingsService';
import { backupService } from '../services/backupService';
import { logger } from './log';
import { isBackendError, stringifyError } from './errorHelper';

let syncInterval: number | null = null;

/**
 * Starts the background sync loop for WalletSeal.
 * Triggers automatic Sync backup when a local seal change is pending upload.
 */
export function startSealSyncLoop() {
    if (syncInterval) return;

    logger.info("Starting WalletSeal background sync loop.");
    performSync();
    syncInterval = window.setInterval(performSync, 30000);
}

/**
 * Stops the background sync loop.
 */
export function stopSealSyncLoop() {
    if (syncInterval) {
        window.clearInterval(syncInterval);
        syncInterval = null;
        logger.info("Stopped WalletSeal background sync loop.");
    }
}

async function performSync() {
    try {
        const status = await settingsService.getSealSyncStatus();
        if (status === "PendingUpload") {
            logger.info("Pending WalletSeal upload detected. Triggering auto-backup sync...");
            await backupService.runAutoBackupSync();
        }
    } catch (e) {
        const msg = stringifyError(e);
        const code = isBackendError(e) ? e.code : '';
        const isExpected = code === 'error.wallet.locked' || code === 'error.auth.sessionTimeout'
            || msg.includes("Wallet is locked") || msg.includes("Session timed out");
        if (!isExpected) {
            logger.warn(`WalletSeal sync cycle encountered an issue: ${msg}`);
        }
    }
}
