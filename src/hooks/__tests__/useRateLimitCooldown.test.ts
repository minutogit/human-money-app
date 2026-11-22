// src/hooks/__tests__/useRateLimitCooldown.test.ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useRateLimitCooldown } from '../useRateLimitCooldown';
import { rateLimitManager } from '../../sync/rateLimitManager';

describe('useRateLimitCooldown & rateLimitManager', () => {
    beforeEach(() => {
        vi.useFakeTimers();
        rateLimitManager.clearCooldown();
    });

    afterEach(() => {
        rateLimitManager.clearCooldown();
        vi.useRealTimers();
    });

    it('extracts Retry-After integer seconds from DTO and string errors', () => {
        // DTO with canonical camelCase retryAfterSecs
        expect(rateLimitManager.extractRetryAfter({ retryAfterSecs: 45 })).toBe(45);

        // Errors array on DTO
        expect(rateLimitManager.extractRetryAfter({
            errors: ['Normal error', 'Rate limited by sync server (HTTP 429). Retry-After: 90s']
        })).toBe(90);

        // String error with Retry-After
        expect(rateLimitManager.extractRetryAfter('Too many requests (HTTP 429). Retry-After: 120s')).toBe(120);

        // Generic 429 without Retry-After falls back to 60s
        expect(rateLimitManager.extractRetryAfter('Server returned HTTP 429: Too Many Requests')).toBe(60);

        // Non-rate-limit error returns null
        expect(rateLimitManager.extractRetryAfter('Connection refused')).toBeNull();
    });

    it('starts countdown on trigger and ticks down each second to 0', () => {
        const onEnd = vi.fn();
        const { result } = renderHook(() => useRateLimitCooldown({ onCooldownEnd: onEnd }));

        expect(result.current.isCoolingDown).toBe(false);
        expect(result.current.cooldownSeconds).toBe(0);

        // Trigger 5-second cooldown
        act(() => {
            result.current.triggerCooldown(5);
        });

        expect(result.current.isCoolingDown).toBe(true);
        expect(result.current.cooldownSeconds).toBe(5);

        // Advance 2 seconds
        act(() => {
            vi.advanceTimersByTime(2000);
        });
        expect(result.current.cooldownSeconds).toBe(3);
        expect(result.current.isCoolingDown).toBe(true);

        // Advance 3 more seconds -> hits 0
        act(() => {
            vi.advanceTimersByTime(3000);
        });
        expect(result.current.cooldownSeconds).toBe(0);
        expect(result.current.isCoolingDown).toBe(false);
        expect(onEnd).toHaveBeenCalledTimes(1);
    });

    it('does not re-subscribe on inline options changes and invokes the latest onCooldownEnd callback', () => {
        const subscribeSpy = vi.spyOn(rateLimitManager, 'subscribe');
        const firstCb = vi.fn();
        const secondCb = vi.fn();

        const { result, rerender } = renderHook(
            ({ cb }: { cb: () => void }) => useRateLimitCooldown({ onCooldownEnd: cb }),
            { initialProps: { cb: firstCb } }
        );

        expect(subscribeSpy).toHaveBeenCalledTimes(1);

        rerender({ cb: secondCb });
        expect(subscribeSpy).toHaveBeenCalledTimes(1);

        act(() => {
            result.current.triggerCooldown(2);
        });
        act(() => {
            vi.advanceTimersByTime(2000);
        });

        expect(firstCb).not.toHaveBeenCalled();
        expect(secondCb).toHaveBeenCalledTimes(1);
        subscribeSpy.mockRestore();
    });

    it('avoids 1-second re-renders when trackCountdown is false while still updating isCoolingDown', () => {
        let renderCount = 0;
        const { result } = renderHook(() => {
            renderCount++;
            return useRateLimitCooldown({ trackCountdown: false });
        });

        expect(renderCount).toBe(1);
        expect(result.current.isCoolingDown).toBe(false);

        act(() => {
            result.current.triggerCooldown(4);
        });
        expect(result.current.isCoolingDown).toBe(true);
        const countOnStart = renderCount;

        act(() => {
            vi.advanceTimersByTime(3000);
        });
        expect(result.current.isCoolingDown).toBe(true);
        expect(renderCount).toBe(countOnStart);

        act(() => {
            vi.advanceTimersByTime(1000);
        });
        expect(result.current.isCoolingDown).toBe(false);
        expect(renderCount).toBe(countOnStart + 1);
    });

    it('checkAndTriggerCooldown automatically starts cooldown on 429 error and returns true', () => {
        const { result } = renderHook(() => useRateLimitCooldown());

        let triggered = false;
        act(() => {
            triggered = result.current.checkAndTriggerCooldown(
                'Rate limited by sync server (HTTP 429). Retry-After: 25s'
            );
        });

        expect(triggered).toBe(true);
        expect(result.current.isCoolingDown).toBe(true);
        expect(result.current.cooldownSeconds).toBe(25);
    });

    it('clamps cooldown durations to 1..=3600 and ignores invalid non-positive/non-finite inputs (S08-X5)', () => {
        // setCooldown ignores 0, negative, NaN, Infinity
        rateLimitManager.setCooldown(0);
        expect(rateLimitManager.isCoolingDown()).toBe(false);
        expect(rateLimitManager.getRemainingSeconds()).toBe(0);

        rateLimitManager.setCooldown(-5);
        expect(rateLimitManager.isCoolingDown()).toBe(false);

        rateLimitManager.setCooldown(Number.NaN);
        expect(rateLimitManager.isCoolingDown()).toBe(false);

        rateLimitManager.setCooldown(Number.POSITIVE_INFINITY);
        expect(rateLimitManager.isCoolingDown()).toBe(false);

        // Positive fractional (< 1s) clamps up to 1s
        rateLimitManager.setCooldown(0.4);
        expect(rateLimitManager.isCoolingDown()).toBe(true);
        expect(rateLimitManager.getRemainingSeconds()).toBe(1);
        rateLimitManager.clearCooldown();

        // Excessively large duration clamps down to 3600s
        rateLimitManager.setCooldown(999_999);
        expect(rateLimitManager.isCoolingDown()).toBe(true);
        expect(rateLimitManager.getRemainingSeconds()).toBe(3600);
        rateLimitManager.clearCooldown();

        // DTO retryAfterSecs & Retry-After header parsing also clamp to [1, 3600]
        expect(rateLimitManager.extractRetryAfter({ retryAfterSecs: 86_400 })).toBe(3600);
        expect(rateLimitManager.extractRetryAfter({ retryAfterSecs: 0.2 })).toBe(1);
        expect(rateLimitManager.extractRetryAfter({ retryAfterSecs: -5 })).toBeNull();
        expect(rateLimitManager.extractRetryAfter({ retryAfterSecs: Number.NaN })).toBeNull();
        expect(rateLimitManager.parseRetryAfterSeconds('Retry-After: 999999s')).toBe(3600);
        expect(rateLimitManager.parseRetryAfterSeconds('Retry-After: 0s')).toBe(1);
    });

    it('rejects incidental "429" inside 64-hex slot IDs or URL ports while honoring real 429 markers (S08-X5)', () => {
        const hexSlotWith429 = `${'ab'.repeat(15)}429${'cd'.repeat(16)}`;
        expect(
            rateLimitManager.extractRetryAfter(
                `Failed to get slot ${hexSlotWith429} from https://sync.example.org:4290/d: Connection refused`
            )
        ).toBeNull();
        expect(
            rateLimitManager.extractRetryAfter(
                'Network error fetching https://node-429.example.org:14290/sync.php: timeout'
            )
        ).toBeNull();
        expect(
            rateLimitManager.extractRetryAfter(
                'Server returned HTTP 500 fetching index: Retry-After: 9999 HTTP 429'
            )
        ).toBeNull();

        expect(rateLimitManager.extractRetryAfter('HTTP 429')).toBe(60);
        expect(rateLimitManager.extractRetryAfter('status: 429')).toBe(60);
        expect(rateLimitManager.extractRetryAfter('status code: 429')).toBe(60);
        expect(rateLimitManager.extractRetryAfter('Retry-After: 120s')).toBe(120);
        expect(rateLimitManager.extractRetryAfter('Too Many Requests')).toBe(60);
        expect(rateLimitManager.extractRetryAfter('Rate limit exceeded')).toBe(60);
    });
});
