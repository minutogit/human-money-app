// src/hooks/useRateLimitCooldown.ts
import { useState, useEffect, useCallback } from 'react';
import { rateLimitManager } from '../utils/rateLimitManager';

export interface UseRateLimitCooldownOptions {
    onCooldownEnd?: () => void;
}

export function useRateLimitCooldown(options?: UseRateLimitCooldownOptions) {
    const [cooldownSeconds, setCooldownSeconds] = useState<number>(() =>
        rateLimitManager.getRemainingSeconds()
    );

    useEffect(() => {
        let prevRemaining = rateLimitManager.getRemainingSeconds();
        setCooldownSeconds(prevRemaining);

        const unsubscribe = rateLimitManager.subscribe((remaining) => {
            setCooldownSeconds(remaining);
            if (prevRemaining > 0 && remaining === 0 && options?.onCooldownEnd) {
                options.onCooldownEnd();
            }
            prevRemaining = remaining;
        });

        return unsubscribe;
    }, [options]);

    const isCoolingDown = cooldownSeconds > 0;

    const triggerCooldown = useCallback((seconds: number = 60) => {
        rateLimitManager.setCooldown(seconds);
    }, []);

    const checkAndTriggerCooldown = useCallback((errOrResult: unknown): boolean => {
        const secs = rateLimitManager.extractRetryAfter(errOrResult);
        if (secs !== null && secs > 0) {
            rateLimitManager.setCooldown(secs);
            return true;
        }
        return false;
    }, []);

    return {
        cooldownSeconds,
        isCoolingDown,
        triggerCooldown,
        checkAndTriggerCooldown,
    };
}
