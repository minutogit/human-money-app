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
            logger.info("Pending seal upload detected. Starting sync process...");
            
            const uploadData = await settingsService.getSealForUpload();
            
            if (uploadData) {
                // MOCK UPLOAD PROCESS
                // In a production app, this would be a POST request to a coordinating server.
                logger.info(`Uploading seal to coordinator (Mock)... Hash: ${uploadData.sealHash}`);
                
                // Simulate network latency
                await new Promise(resolve => setTimeout(resolve, 1000));
                
                // After successful "upload", acknowledge it in the core
                await settingsService.acknowledgeSealSync(uploadData.sealHash);
                
                logger.info("WalletSeal successfully synchronized and acknowledged.");
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
