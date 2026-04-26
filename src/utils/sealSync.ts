import { invoke } from '@tauri-apps/api/core';
import { logger } from './log';
import { SealUploadData } from '../types';

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
        const status = await invoke<string>("get_seal_sync_status");
        
        if (status === "PendingUpload") {
            logger.info("Pending seal upload detected. Starting sync process...");
            
            const uploadData = await invoke<SealUploadData | null>("get_seal_for_upload");
            
            if (uploadData) {
                // MOCK UPLOAD PROCESS
                // In a production app, this would be a POST request to a coordinating server.
                logger.info(`Uploading seal to coordinator (Mock)... Hash: ${uploadData.seal_hash}`);
                
                // Simulate network latency
                await new Promise(resolve => setTimeout(resolve, 1000));
                
                // After successful "upload", acknowledge it in the core
                await invoke("acknowledge_seal_sync", {
                    uploadedSealHash: uploadData.seal_hash,
                    password: null // Use session key if available (handled in backend)
                });
                
                logger.info("WalletSeal successfully synchronized and acknowledged.");
            }
        }
    } catch (e) {
        // Silently fail but log, as this runs in background
        // Errors like "Wallet is locked" are expected when the session is closed
        const msg = String(e);
        if (!msg.includes("Wallet is locked") && !msg.includes("Session timed out")) {
            logger.warn(`WalletSeal sync cycle encountered an issue: ${e}`);
        }
    }
}
