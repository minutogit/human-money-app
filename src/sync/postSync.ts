// src/sync/postSync.ts
import { postService } from '../services/postService';
import { rateLimitManager } from './rateLimitManager';
import { logger } from '../utils/log';

let postSyncInterval: number | null = null;
let isPostSyncRunning = false;

/**
 * Runs a single post sync pass if not already running and not cooling down from a 429.
 */
export async function runPostSyncPass(): Promise<void> {
    if (isPostSyncRunning) {
        return;
    }
    if (rateLimitManager.isCoolingDown()) {
        logger.debug("Skipping background post sync pass due to active 429 rate limit cooldown.");
        return;
    }

    isPostSyncRunning = true;
    try {
        const res = await postService.syncPostChannels();
        if (res) {
            const retrySecs = rateLimitManager.extractRetryAfter(res);
            if (retrySecs) {
                rateLimitManager.setCooldown(retrySecs);
            }
        }
    } catch (err) {
        const retrySecs = rateLimitManager.extractRetryAfter(err);
        if (retrySecs) {
            rateLimitManager.setCooldown(retrySecs);
        }
        logger.debug("Background post sync pass encountered an issue: " + String(err));
    } finally {
        isPostSyncRunning = false;
    }
}

/**
 * Starts the periodic foreground post sync loop (every 90s, within 60–120s range),
 * plus an initial run on app start / session unlock.
 */
export function startPostSyncLoop(intervalMs: number = 90_000): void {
    if (postSyncInterval !== null || typeof window === 'undefined') {
        return;
    }
    logger.info("Starting periodic foreground post sync loop (every 90s).");

    // Immediate initial sync on start
    runPostSyncPass().catch(() => {});

    // Periodic loop every 90s
    postSyncInterval = window.setInterval(() => {
        runPostSyncPass().catch(() => {});
    }, intervalMs);
}

/**
 * Stops the periodic post sync loop (e.g. on logout / lock).
 */
export function stopPostSyncLoop(): void {
    if (postSyncInterval !== null && typeof window !== 'undefined') {
        window.clearInterval(postSyncInterval);
        postSyncInterval = null;
        logger.info("Stopped post sync loop.");
    }
}
