// src/hooks/useRateLimitCooldown.ts
import { useState, useEffect, useCallback, useRef } from 'react';
import { rateLimitManager } from '../sync/rateLimitManager';

export interface UseRateLimitCooldownOptions {
    onCooldownEnd?: () => void;
    /**
     * When `false`, only state transitions between cooling down and idle
     * update component state, avoiding 1-second re-renders when countdown
     * seconds are not displayed. Defaults to `true`.
     */
    trackCountdown?: boolean;
}

export function useRateLimitCooldown(options?: UseRateLimitCooldownOptions) {
    const trackCountdown = options?.trackCountdown ?? true;
    const onCooldownEndRef = useRef(options?.onCooldownEnd);
    onCooldownEndRef.current = options?.onCooldownEnd;

    const [cooldownSeconds, setCooldownSeconds] = useState<number>(() =>
        rateLimitManager.getRemainingSeconds()
    );
    const isCoolingRef = useRef(cooldownSeconds > 0);
    isCoolingRef.current = cooldownSeconds > 0;

    useEffect(() => {
        let prevRemaining = rateLimitManager.getRemainingSeconds();
        if (trackCountdown) {
            setCooldownSeconds(prevRemaining);
        } else {
            const nowCooling = prevRemaining > 0;
            if (isCoolingRef.current !== nowCooling) {
                isCoolingRef.current = nowCooling;
                setCooldownSeconds(nowCooling ? prevRemaining : 0);
            }
        }

        const unsubscribe = rateLimitManager.subscribe((remaining) => {
            if (trackCountdown) {
                setCooldownSeconds(remaining);
            } else {
                const nowCooling = remaining > 0;
                if (isCoolingRef.current !== nowCooling) {
                    isCoolingRef.current = nowCooling;
                    setCooldownSeconds(nowCooling ? remaining : 0);
                }
            }
            if (prevRemaining > 0 && remaining === 0) {
                onCooldownEndRef.current?.();
            }
            prevRemaining = remaining;
        });

        return unsubscribe;
    }, [trackCountdown]);

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
