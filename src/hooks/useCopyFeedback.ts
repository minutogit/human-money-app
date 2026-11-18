// src/hooks/useCopyFeedback.ts
import { useState, useRef, useEffect, useCallback } from 'react';
import { copyToClipboard } from '../utils/clipboard';

export interface UseCopyFeedbackOptions {
    resetDelayMs?: number;
}

/**
 * Standardized clipboard copy hook with auto-resetting feedback state
 * and automatic timer cleanup on unmount.
 */
export function useCopyFeedback(optionsOrDelay: number | UseCopyFeedbackOptions = 2000) {
    const resetDelayMs = Math.max(
        0,
        typeof optionsOrDelay === 'number'
            ? optionsOrDelay
            : (optionsOrDelay.resetDelayMs ?? 2000)
    );

    const [copied, setCopied] = useState(false);
    const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const isMountedRef = useRef(true);

    const clearResetTimer = useCallback(() => {
        if (timerRef.current !== null) {
            clearTimeout(timerRef.current);
            timerRef.current = null;
        }
    }, []);

    useEffect(() => {
        isMountedRef.current = true;
        return () => {
            isMountedRef.current = false;
            clearResetTimer();
        };
    }, [clearResetTimer]);

    const reset = useCallback(() => {
        clearResetTimer();
        if (isMountedRef.current) {
            setCopied(false);
        }
    }, [clearResetTimer]);

    const copyText = useCallback(
        async (value: string): Promise<boolean> => {
            if (!value) return false;
            const ok = await copyToClipboard(value);
            if (!ok || !isMountedRef.current) {
                return ok;
            }
            clearResetTimer();
            setCopied(true);
            timerRef.current = setTimeout(() => {
                timerRef.current = null;
                if (isMountedRef.current) {
                    setCopied(false);
                }
            }, resetDelayMs);
            return true;
        },
        [clearResetTimer, resetDelayMs]
    );

    return { copied, copyText, reset };
}
