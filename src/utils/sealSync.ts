import { settingsService } from '../services/settingsService';
import { logger } from './log';
import { isBackendError, stringifyError } from './errorHelper';


let syncInterval: number | null = null;

/**
 * Starts the background sync loop for WalletSeal.
 * Checks for pending uploads and processes them.
 */
export function startSealSyncLoop() {
    if (syncInterval) return;

    logger.info("Starting WalletSeal background sync loop.");
    
    // Initial check
    performSync();

    // Run every 30 seconds
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
            logger.info("Pending seal upload detected. Flushing L2 anchor sync...");
            
            const success = await settingsService.flushPendingAnchorSync(null);
            if (success) {
                logger.info("Wallet L2 anchor successfully synchronized.");
            }
        }
    } catch (e) {
        // Silently fail but log, as this runs in background
        // Errors like "Wallet is locked" are expected when the session is closed
        const msg = stringifyError(e);
        const code = isBackendError(e) ? e.code : '';
        const isExpected = code === 'error.wallet.locked' || code === 'error.auth.sessionTimeout'
            || msg.includes("Wallet is locked") || msg.includes("Session timed out");
        if (!isExpected) {
            logger.warn(`WalletSeal sync cycle encountered an issue: ${msg}`);
        }
    }
}
