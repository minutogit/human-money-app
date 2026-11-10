// src/hooks/__tests__/useRateLimitCooldown.test.ts
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useRateLimitCooldown } from '../useRateLimitCooldown';
import { rateLimitManager } from '../../utils/rateLimitManager';

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
        // DTO with retryAfterSecs
        expect(rateLimitManager.extractRetryAfter({ retryAfterSecs: 45 })).toBe(45);
        expect(rateLimitManager.extractRetryAfter({ retry_after_secs: 30 })).toBe(30);

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
});
