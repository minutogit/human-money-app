// src/sync/marketSync.ts
import { channelMembersSyncManager } from './channelMembersSyncManager';
import { marketService } from '../services/marketService';
import { rateLimitManager } from './rateLimitManager';
import { logger } from '../utils/log';

export const DEFAULT_MARKET_SYNC_INTERVAL_MS = 20 * 60_000;
export const DEFAULT_ONBOARDING_DELAY_MS = 3_000;

let marketSyncInterval: number | null = null;
let onboardingTimer: number | null = null;
let isMarketSyncRunning = false;
let pendingForceUpload = false;
let pendingForcePassword: string | undefined;

function recordRateLimit(val: unknown): void {
    const retrySecs = rateLimitManager.extractRetryAfter(val);
    if (retrySecs) {
        rateLimitManager.setCooldown(retrySecs);
    }
}

/**
 * Runs a single market (`card` + `wot`) sync pass.
 * Background runs enable the backend 20-minute poll cooldown (`respectCooldown: true`).
 */
export async function runMarketSyncPass(options?: {
    forceUpload?: boolean;
    password?: string;
}): Promise<void> {
    if (isMarketSyncRunning) {
        if (options?.forceUpload) {
            pendingForceUpload = true;
            pendingForcePassword = options.password;
        }
        return;
    }

    if (!options?.forceUpload && rateLimitManager.isCoolingDown()) {
        logger.debug('Skipping background market sync pass due to active 429 rate limit cooldown.');
        return;
    }

    isMarketSyncRunning = true;
    try {
        const res = await marketService.syncMarketChannels(
            options?.forceUpload ? { forceUpload: true } : { background: true },
            options?.password,
        );
        if (res) {
            recordRateLimit(res);
        }
    } catch (err) {
        recordRateLimit(err);
        logger.debug('Background market sync pass encountered an issue: ' + String(err));
    } finally {
        isMarketSyncRunning = false;
        if (pendingForceUpload) {
            const pwd = pendingForcePassword;
            pendingForceUpload = false;
            pendingForcePassword = undefined;
            runMarketSyncPass({ forceUpload: true, password: pwd }).catch(() => {});
        }
    }
}

/**
 * Schedules a debounced forced card + WoT publish after onboarding/upgrading a contact,
 * flushing any pending channel-members update first so the newcomer key is authorized beforehand.
 */
export function scheduleContactOnboardingMarketSync(
    delayMs: number = DEFAULT_ONBOARDING_DELAY_MS,
    password?: string,
): void {
    if (typeof window === 'undefined') {
        return;
    }
    if (onboardingTimer !== null) {
        window.clearTimeout(onboardingTimer);
    }
    onboardingTimer = window.setTimeout(async () => {
        onboardingTimer = null;
        await channelMembersSyncManager.flushIfPending();
        await runMarketSyncPass({ forceUpload: true, password });
    }, delayMs);
}

/**
 * Cancels any pending debounced onboarding market sync timer.
 */
export function cancelContactOnboardingMarketSync(): void {
    if (onboardingTimer !== null && typeof window !== 'undefined') {
        window.clearTimeout(onboardingTimer);
        onboardingTimer = null;
    }
}

/**
 * Starts the periodic foreground market (`card` + `wot`) sync loop (default every 20 min),
 * plus an initial background pass honoring the 20-min backend poll cooldown.
 */
export function startMarketSyncLoop(intervalMs: number = DEFAULT_MARKET_SYNC_INTERVAL_MS): void {
    if (marketSyncInterval !== null || typeof window === 'undefined') {
        return;
    }
    logger.info('Starting periodic foreground market sync loop (every 20 min).');

    runMarketSyncPass().catch(() => {});

    marketSyncInterval = window.setInterval(() => {
        runMarketSyncPass().catch(() => {});
    }, intervalMs);
}

/**
 * Stops the periodic market sync loop and clears pending onboarding timers (e.g. on logout/lock).
 */
export function stopMarketSyncLoop(): void {
    cancelContactOnboardingMarketSync();
    pendingForceUpload = false;
    pendingForcePassword = undefined;
    if (marketSyncInterval !== null && typeof window !== 'undefined') {
        window.clearInterval(marketSyncInterval);
        marketSyncInterval = null;
        logger.info('Stopped market sync loop.');
    }
}
