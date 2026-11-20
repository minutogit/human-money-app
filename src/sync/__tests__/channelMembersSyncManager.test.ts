// src/sync/__tests__/channelMembersSyncManager.test.ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
    channelMembersSyncManager,
    parseRetryAfterSeconds,
    isQuotaError,
    isRateLimitError,
} from '../channelMembersSyncManager';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

describe('ChannelMembersSyncManager', () => {
    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();
        channelMembersSyncManager.reset();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    describe('Helper Parsers', () => {
        it('parses Retry-After seconds correctly', () => {
            expect(parseRetryAfterSeconds('Rate limited pushing members (HTTP 429). Retry-After: 30s')).toBe(30);
            expect(parseRetryAfterSeconds('Retry-After: 45')).toBe(45);
            expect(parseRetryAfterSeconds('No header here')).toBeNull();
        });

        it('detects quota errors correctly', () => {
            expect(isQuotaError('Server returned HTTP 507 pushing members: Insufficient Storage')).toBe(true);
            expect(isQuotaError('Insufficient Storage: Exceeds max_members limit')).toBe(true);
            expect(isQuotaError('Network connection lost')).toBe(false);
        });

        it('detects rate limit errors correctly', () => {
            expect(isRateLimitError('Rate limited pushing members (HTTP 429). Retry-After: 15s')).toBe(true);
            expect(isRateLimitError('Server returned HTTP 429')).toBe(true);
            expect(isRateLimitError('Unknown internal error')).toBe(false);
        });
    });

    describe('Snappy Single Contact Updates', () => {
        it('executes sync quickly after default 250ms without waiting for 1.5s', async () => {
            vi.mocked(invoke).mockResolvedValue(1);

            channelMembersSyncManager.trigger({ delayMs: 250 });

            // After 100ms, not yet called
            await vi.advanceTimersByTimeAsync(100);
            expect(invoke).not.toHaveBeenCalled();

            // After 250ms, must have executed
            await vi.advanceTimersByTimeAsync(150);
            expect(invoke).toHaveBeenCalledTimes(1);
            expect(invoke).toHaveBeenCalledWith('sync_channel_members', {});

            const status = channelMembersSyncManager.getStatus();
            expect(status.isSyncing).toBe(false);
            expect(status.lastError).toBeNull();
            expect(status.lastSyncSuccessAt).toBeGreaterThan(0);
        });

        it('passes password if provided on trigger', async () => {
            vi.mocked(invoke).mockResolvedValue(1);

            channelMembersSyncManager.trigger({ delayMs: 100, password: 'secret_master_pw' });
            await vi.advanceTimersByTimeAsync(100);

            expect(invoke).toHaveBeenCalledWith('sync_channel_members', { password: 'secret_master_pw' });
        });
    });

    describe('Rate Limiting (HTTP 429) & Auto-Retry', () => {
        it('respects Retry-After header and schedules auto-retry after specified seconds', async () => {
            vi.mocked(invoke).mockRejectedValueOnce(
                new Error('Rate limited pushing members (HTTP 429). Retry-After: 20s')
            );

            channelMembersSyncManager.trigger({ delayMs: 100 });
            await vi.advanceTimersByTimeAsync(100);

            expect(invoke).toHaveBeenCalledTimes(1);
            let status = channelMembersSyncManager.getStatus();
            expect(status.retryAttempt).toBe(1);
            expect(status.rateLimitedUntil).not.toBeNull();
            expect(status.lastError).toContain('HTTP 429');

            // Set up success for the retry
            vi.mocked(invoke).mockResolvedValueOnce(2);

            // Advance 19 seconds - should not have retried yet
            await vi.advanceTimersByTimeAsync(19000);
            expect(invoke).toHaveBeenCalledTimes(1);

            // Advance 1 more second (20s total)
            await vi.advanceTimersByTimeAsync(1000);
            expect(invoke).toHaveBeenCalledTimes(2);

            status = channelMembersSyncManager.getStatus();
            expect(status.retryAttempt).toBe(0);
            expect(status.rateLimitedUntil).toBeNull();
            expect(status.lastError).toBeNull();
        });
    });

    describe('Quota Limits (HTTP 507)', () => {
        it('marks quotaExceeded and stops polling until resetQuota is passed', async () => {
            vi.mocked(invoke).mockRejectedValueOnce(
                new Error('Insufficient Storage: Exceeds max_members limit (HTTP 507)')
            );

            channelMembersSyncManager.trigger({ delayMs: 100 });
            await vi.advanceTimersByTimeAsync(100);

            expect(invoke).toHaveBeenCalledTimes(1);
            let status = channelMembersSyncManager.getStatus();
            expect(status.quotaExceeded).toBe(true);

            // Next standard triggers without resetQuota are ignored
            channelMembersSyncManager.trigger({ delayMs: 100 });
            await vi.advanceTimersByTimeAsync(200);
            expect(invoke).toHaveBeenCalledTimes(1);

            // Trigger with resetQuota (e.g. user deleted or modified a contact)
            vi.mocked(invoke).mockResolvedValueOnce(1);
            channelMembersSyncManager.trigger({ delayMs: 100, resetQuota: true });
            await vi.advanceTimersByTimeAsync(100);

            expect(invoke).toHaveBeenCalledTimes(2);
            status = channelMembersSyncManager.getStatus();
            expect(status.quotaExceeded).toBe(false);
        });
    });

    describe('Network Error & Exponential Backoff', () => {
        it('retries with exponential backoff on connection errors', async () => {
            vi.mocked(invoke).mockRejectedValueOnce(new Error('Network error: connect ECONNREFUSED'));

            channelMembersSyncManager.trigger({ delayMs: 100 });
            await vi.advanceTimersByTimeAsync(100);

            expect(invoke).toHaveBeenCalledTimes(1);
            let status = channelMembersSyncManager.getStatus();
            expect(status.retryAttempt).toBe(1);

            // First backoff is 5000ms
            await vi.advanceTimersByTimeAsync(4900);
            expect(invoke).toHaveBeenCalledTimes(1);

            vi.mocked(invoke).mockResolvedValueOnce(1);
            await vi.advanceTimersByTimeAsync(200);
            expect(invoke).toHaveBeenCalledTimes(2);

            status = channelMembersSyncManager.getStatus();
            expect(status.retryAttempt).toBe(0);
            expect(status.lastError).toBeNull();
        });
    });

    describe('Online Reconnect Trigger', () => {
        it('immediately re-triggers sync when window online event fires if an error occurred', async () => {
            vi.mocked(invoke).mockRejectedValueOnce(new Error('Network offline'));

            channelMembersSyncManager.trigger({ delayMs: 100 });
            await vi.advanceTimersByTimeAsync(100);

            expect(invoke).toHaveBeenCalledTimes(1);
            expect(channelMembersSyncManager.getStatus().lastError).not.toBeNull();

            vi.mocked(invoke).mockResolvedValueOnce(1);

            // Fire window online event
            window.dispatchEvent(new Event('online'));

            // Advance minimal delay (250ms)
            await vi.advanceTimersByTimeAsync(250);
            expect(invoke).toHaveBeenCalledTimes(2);
            expect(channelMembersSyncManager.getStatus().lastError).toBeNull();
        });
    });

    describe('Concurrency and In-flight Pending Updates', () => {
        it('queues a pending sync if another trigger arrives while a sync is executing', async () => {
            let resolveFirstInvoke!: (value: number) => void;
            const firstPromise = new Promise<number>((resolve) => {
                resolveFirstInvoke = resolve;
            });

            vi.mocked(invoke).mockReturnValueOnce(firstPromise as unknown as Promise<number>);

            channelMembersSyncManager.trigger({ delayMs: 50 });
            await vi.advanceTimersByTimeAsync(50);

            expect(invoke).toHaveBeenCalledTimes(1);
            expect(channelMembersSyncManager.getStatus().isSyncing).toBe(true);

            // New trigger arrives while first is still in flight
            channelMembersSyncManager.trigger({ delayMs: 50 });
            expect(channelMembersSyncManager.getStatus().isPending).toBe(true);

            // Second invoke setup
            vi.mocked(invoke).mockResolvedValueOnce(1);

            // Finish first invoke
            resolveFirstInvoke(1);
            await vi.advanceTimersByTimeAsync(0);

            // Pending trigger should fire after 100ms
            await vi.advanceTimersByTimeAsync(100);
            expect(invoke).toHaveBeenCalledTimes(2);
            expect(channelMembersSyncManager.getStatus().isPending).toBe(false);
            expect(channelMembersSyncManager.getStatus().isSyncing).toBe(false);
        });

        it('F5: discards stale in-flight sync completion/failure after reset() on profile switch', async () => {
            let resolveStale!: (value: number) => void;
            let rejectStale!: (reason?: unknown) => void;
            const firstPromise = new Promise<number>((resolve) => {
                resolveStale = resolve;
            });
            vi.mocked(invoke).mockReturnValueOnce(firstPromise as unknown as Promise<number>);

            channelMembersSyncManager.trigger({ delayMs: 25 });
            await vi.advanceTimersByTimeAsync(25);
            expect(channelMembersSyncManager.getStatus().isSyncing).toBe(true);

            // User switches profile while Profile A sync is still awaiting IPC
            channelMembersSyncManager.reset();
            expect(channelMembersSyncManager.getStatus().isSyncing).toBe(false);

            // Queue a follow-up in generation 0 would not happen after reset; even resolving stale promise leaves clean status
            channelMembersSyncManager.trigger({ delayMs: 25 });
            resolveStale(5);
            await vi.advanceTimersByTimeAsync(0);
            expect(channelMembersSyncManager.getStatus().lastSyncSuccessAt).toBeNull();

            vi.mocked(invoke).mockResolvedValueOnce(1);
            await vi.advanceTimersByTimeAsync(25);
            expect(invoke).toHaveBeenCalledTimes(2);
            expect(channelMembersSyncManager.getStatus().lastSyncSuccessAt).not.toBeNull();

            // Also verify stale rejection from an earlier generation cannot poison retryAttempt/lastError
            const failingPromise = new Promise<number>((_resolve, reject) => {
                rejectStale = reject;
            });
            vi.mocked(invoke).mockReturnValueOnce(failingPromise as unknown as Promise<number>);
            channelMembersSyncManager.trigger({ delayMs: 25 });
            await vi.advanceTimersByTimeAsync(25);
            channelMembersSyncManager.reset();

            rejectStale(new Error('Stale profile error'));
            await vi.advanceTimersByTimeAsync(0);
            expect(channelMembersSyncManager.getStatus().lastError).toBeNull();
            expect(channelMembersSyncManager.getStatus().retryAttempt).toBe(0);
        });
    });
});
