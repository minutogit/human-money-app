// src/utils/__tests__/postSync.test.ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { startPostSyncLoop, stopPostSyncLoop, runPostSyncPass } from '../postSync';
import { postService } from '../../services/postService';
import { rateLimitManager } from '../rateLimitManager';

vi.mock('../../services/postService', () => ({
    postService: {
        syncPostChannels: vi.fn(),
    },
}));

vi.mock('../log', () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

describe('postSync utils (Point G.1)', () => {
    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();
        stopPostSyncLoop();
        rateLimitManager.clearCooldown();
    });

    afterEach(() => {
        stopPostSyncLoop();
        rateLimitManager.clearCooldown();
        vi.useRealTimers();
    });

    it('startPostSyncLoop triggers immediate pass and then every 90s', async () => {
        vi.mocked(postService.syncPostChannels).mockResolvedValue({
            uploadedTo: 1,
            polledSyncs: 1,
            importedPayments: 0,
            importedReceipts: 0,
            importedSignatures: 0,
            newSignRequests: 0,
            newWotHandshakes: 0,
            newInquiries: 0,
            newInquiryReplies: 0,
            prunedBoxes: 0,
            errors: [],
        });

        startPostSyncLoop(90_000);

        // Immediate run on startup
        expect(postService.syncPostChannels).toHaveBeenCalledTimes(1);

        // Advance 89s -> no new call
        await vi.advanceTimersByTimeAsync(89_000);
        expect(postService.syncPostChannels).toHaveBeenCalledTimes(1);

        // Advance 1s -> exactly 90s -> 2nd run
        await vi.advanceTimersByTimeAsync(1_000);
        expect(postService.syncPostChannels).toHaveBeenCalledTimes(2);

        // Advance another 90s -> 3rd run
        await vi.advanceTimersByTimeAsync(90_000);
        expect(postService.syncPostChannels).toHaveBeenCalledTimes(3);

        stopPostSyncLoop();
    });

    it('skips background post sync when 429 rate limit cooldown is active', async () => {
        rateLimitManager.setCooldown(60);
        expect(rateLimitManager.isCoolingDown()).toBe(true);

        await runPostSyncPass();
        expect(postService.syncPostChannels).not.toHaveBeenCalled();

        // Advance beyond cooldown
        vi.advanceTimersByTime(61_000);
        expect(rateLimitManager.isCoolingDown()).toBe(false);

        await runPostSyncPass();
        expect(postService.syncPostChannels).toHaveBeenCalledTimes(1);
    });
});
