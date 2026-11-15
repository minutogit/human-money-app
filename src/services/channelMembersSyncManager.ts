// src/services/channelMembersSyncManager.ts
import { invoke } from '@tauri-apps/api/core';

export interface ChannelMembersSyncStatus {
    isSyncing: boolean;
    isPending: boolean;
    lastSyncSuccessAt: number | null;
    lastError: string | null;
    quotaExceeded: boolean;
    rateLimitedUntil: number | null;
    retryAttempt: number;
}

export interface TriggerSyncOptions {
    /** Debounce or delay time in ms. Defaults to 250ms for snappy single-contact updates. */
    delayMs?: number;
    /** Whether to trigger immediately bypassing the delay timer. */
    immediate?: boolean;
    /** Whether to reset the quotaExceeded flag (e.g. when contacts/policy change). */
    resetQuota?: boolean;
    /** Optional wallet password for encrypted store access. */
    password?: string;
}

export const BACKOFF_DELAYS_MS = [5000, 15000, 30000, 60000, 120000];

export function parseRetryAfterSeconds(errMsg: string): number | null {
    const match = errMsg.match(/Retry-After:\s*(\d+)s?/i);
    if (match && match[1]) {
        const secs = parseInt(match[1], 10);
        return isNaN(secs) ? null : Math.max(1, secs);
    }
    return null;
}

export function isQuotaError(errMsg: string): boolean {
    const lower = errMsg.toLowerCase();
    return lower.includes('507') || lower.includes('insufficient storage') || lower.includes('max_members');
}

export function isRateLimitError(errMsg: string): boolean {
    const lower = errMsg.toLowerCase();
    return lower.includes('429') || lower.includes('rate limited');
}

export class ChannelMembersSyncManager {
    private status: ChannelMembersSyncStatus = {
        isSyncing: false,
        isPending: false,
        lastSyncSuccessAt: null,
        lastError: null,
        quotaExceeded: false,
        rateLimitedUntil: null,
        retryAttempt: 0,
    };

    private syncTimer: ReturnType<typeof setTimeout> | null = null;
    private listeners = new Set<(status: ChannelMembersSyncStatus) => void>();
    private pendingPassword?: string;

    constructor() {
        if (typeof window !== 'undefined') {
            window.addEventListener('online', () => {
                // When coming back online, immediately retry pending or failed syncs
                if (this.status.lastError !== null || this.status.isPending || this.status.retryAttempt > 0) {
                    this.trigger({ delayMs: 250, immediate: true, resetQuota: false });
                }
            });
        }
    }

    public getStatus(): ChannelMembersSyncStatus {
        return { ...this.status };
    }

    public subscribe(listener: (status: ChannelMembersSyncStatus) => void): () => void {
        this.listeners.add(listener);
        listener(this.getStatus());
        return () => {
            this.listeners.delete(listener);
        };
    }

    private notify(): void {
        const current = this.getStatus();
        for (const listener of this.listeners) {
            try {
                listener(current);
            } catch {
                // ignore listener exceptions
            }
        }
    }

    public trigger(options?: TriggerSyncOptions): void {
        if (typeof window === 'undefined') return;

        if (options?.password) {
            this.pendingPassword = options.password;
        }

        if (options?.resetQuota) {
            this.status.quotaExceeded = false;
        }

        // If quota is exceeded and wasn't reset by this trigger, do not spam retries
        if (this.status.quotaExceeded) {
            return;
        }

        // If a sync is currently executing, flag pending so it runs again after completion
        if (this.status.isSyncing) {
            this.status.isPending = true;
            this.notify();
            return;
        }

        // Clear existing scheduled timer to debounce consecutive edits
        if (this.syncTimer !== null) {
            clearTimeout(this.syncTimer);
            this.syncTimer = null;
        }

        const now = Date.now();
        let delay = options?.delayMs ?? 250;

        // If currently rate-limited, ensure we wait until the window has elapsed
        if (this.status.rateLimitedUntil !== null && this.status.rateLimitedUntil > now) {
            const remaining = this.status.rateLimitedUntil - now;
            delay = Math.max(delay, remaining);
        }

        if (options?.immediate && (!this.status.rateLimitedUntil || this.status.rateLimitedUntil <= now)) {
            delay = 0;
        }

        this.syncTimer = setTimeout(async () => {
            this.syncTimer = null;
            await this.executeSync();
        }, delay);
    }

    public async executeSync(): Promise<number> {
        if (this.status.isSyncing) {
            this.status.isPending = true;
            return 0;
        }

        this.status.isSyncing = true;
        this.status.isPending = false;
        this.notify();

        const pwd = this.pendingPassword;
        this.pendingPassword = undefined;

        try {
            const count = await invoke<number>('sync_channel_members', pwd ? { password: pwd } : {});

            this.status.isSyncing = false;
            this.status.retryAttempt = 0;
            this.status.lastError = null;
            this.status.quotaExceeded = false;
            this.status.rateLimitedUntil = null;
            this.status.lastSyncSuccessAt = Date.now();
            this.notify();

            // If changes arrived while this sync was in-flight, coalesce and run them
            if (this.status.isPending) {
                this.trigger({ delayMs: 100 });
            }

            return count;
        } catch (err: any) {
            this.status.isSyncing = false;
            const errMsg = String(err?.message || err || 'Unknown sync error');
            this.status.lastError = errMsg;

            if (isRateLimitError(errMsg)) {
                const waitSecs = parseRetryAfterSeconds(errMsg) ?? 15;
                this.status.rateLimitedUntil = Date.now() + waitSecs * 1000;
                this.status.retryAttempt++;
                this.scheduleNextAttempt(waitSecs * 1000);
            } else if (isQuotaError(errMsg)) {
                this.status.quotaExceeded = true;
                this.status.retryAttempt = 0;
                // Wait for user to add/remove/change contacts before next retry
            } else {
                this.status.retryAttempt++;
                const backoff = BACKOFF_DELAYS_MS[Math.min(this.status.retryAttempt - 1, BACKOFF_DELAYS_MS.length - 1)];
                this.scheduleNextAttempt(backoff);
            }

            this.notify();
            return 0;
        }
    }

    private scheduleNextAttempt(delayMs: number): void {
        if (typeof window === 'undefined') return;
        if (this.syncTimer !== null) {
            clearTimeout(this.syncTimer);
        }
        this.syncTimer = setTimeout(async () => {
            this.syncTimer = null;
            await this.executeSync();
        }, delayMs);
    }

    public async flushIfPending(): Promise<void> {
        if (
            this.syncTimer !== null
            && (!this.status.rateLimitedUntil || this.status.rateLimitedUntil <= Date.now())
        ) {
            this.cancelTimer();
            await this.executeSync();
        }
    }

    public cancelTimer(): void {
        if (this.syncTimer !== null) {
            clearTimeout(this.syncTimer);
            this.syncTimer = null;
        }
    }

    public reset(): void {
        this.cancelTimer();
        this.status = {
            isSyncing: false,
            isPending: false,
            lastSyncSuccessAt: null,
            lastError: null,
            quotaExceeded: false,
            rateLimitedUntil: null,
            retryAttempt: 0,
        };
        this.pendingPassword = undefined;
        this.notify();
    }
}

export const channelMembersSyncManager = new ChannelMembersSyncManager();
