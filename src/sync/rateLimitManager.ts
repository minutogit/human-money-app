// src/sync/rateLimitManager.ts

type RateLimitListener = (remainingSeconds: number) => void;

class RateLimitManager {
    private cooldownUntil: number = 0;
    private listeners: Set<RateLimitListener> = new Set();
    private tickerInterval: number | null = null;

    /**
     * Checks if an HTTP 429 rate limit cooldown is currently active.
     */
    public isCoolingDown(): boolean {
        return Date.now() < this.cooldownUntil;
    }

    /**
     * Returns the remaining cooldown duration in integer seconds (or 0 if not cooling down).
     */
    public getRemainingSeconds(): number {
        const remainingMs = this.cooldownUntil - Date.now();
        return remainingMs > 0 ? Math.ceil(remainingMs / 1000) : 0;
    }

    /**
     * Sets or extends the rate-limit cooldown duration by `seconds`.
     */
    public setCooldown(seconds: number): void {
        const safeSeconds = Math.max(1, Math.floor(seconds));
        const target = Date.now() + safeSeconds * 1000;
        if (target > this.cooldownUntil) {
            this.cooldownUntil = target;
        }
        this.startTickerIfNeeded();
        this.notifyListeners();
    }

    /**
     * Clears any active cooldown immediately.
     */
    public clearCooldown(): void {
        this.cooldownUntil = 0;
        this.stopTicker();
        this.notifyListeners();
    }

    /**
     * Subscribes to 1-second interval cooldown updates.
     */
    public subscribe(listener: RateLimitListener): () => void {
        this.listeners.add(listener);
        listener(this.getRemainingSeconds());
        this.startTickerIfNeeded();
        return () => {
            this.listeners.delete(listener);
            if (this.listeners.size === 0) {
                this.stopTicker();
            }
        };
    }

    /**
     * Extracts `retryAfterSecs` from a sync response DTO, Error object, or error string.
     * Returns the parsed duration in seconds, or `null` if no 429 / Retry-After was detected.
     */
    public extractRetryAfter(errOrResult: unknown): number | null {
        if (!errOrResult) return null;

        // 1. Direct DTO property (canonical camelCase)
        if (typeof errOrResult === 'object' && errOrResult !== null) {
            const obj = errOrResult as Record<string, unknown>;
            if (typeof obj.retryAfterSecs === 'number' && obj.retryAfterSecs > 0) {
                return obj.retryAfterSecs;
            }

            // Inspect errors array on sync result DTOs
            if (Array.isArray(obj.errors)) {
                let maxFound: number | null = null;
                for (const item of obj.errors) {
                    if (typeof item === 'string') {
                        const parsed = this.parseErrorString(item);
                        if (parsed !== null) {
                            maxFound = maxFound !== null ? Math.max(maxFound, parsed) : parsed;
                        }
                    }
                }
                if (maxFound !== null) {
                    return maxFound;
                }
            }
        }

        // 2. String or Error inspection
        const text = typeof errOrResult === 'string'
            ? errOrResult
            : (errOrResult instanceof Error ? errOrResult.message : String(errOrResult));

        return this.parseErrorString(text);
    }

    public parseRetryAfterSeconds(text: string): number | null {
        const retryMatch = text.match(/Retry-After:\s*(\d+)s?/i);
        if (retryMatch && retryMatch[1]) {
            const secs = parseInt(retryMatch[1], 10);
            if (!isNaN(secs) && secs > 0) {
                return secs;
            }
        }
        return null;
    }

    private parseErrorString(text: string): number | null {
        const parsed = this.parseRetryAfterSeconds(text);
        if (parsed !== null) {
            return parsed;
        }

        if (text.includes('429') || text.toLowerCase().includes('rate limit') || text.toLowerCase().includes('too many requests')) {
            return 60; // Sensible default fallback
        }

        return null;
    }

    private startTickerIfNeeded(): void {
        if (this.isCoolingDown() && this.tickerInterval === null && typeof window !== 'undefined') {
            this.tickerInterval = window.setInterval(() => {
                const remaining = this.getRemainingSeconds();
                this.notifyListeners();
                if (remaining <= 0) {
                    this.stopTicker();
                }
            }, 1000);
        }
    }

    private stopTicker(): void {
        if (this.tickerInterval !== null && typeof window !== 'undefined') {
            window.clearInterval(this.tickerInterval);
            this.tickerInterval = null;
        }
    }

    private notifyListeners(): void {
        const rem = this.getRemainingSeconds();
        this.listeners.forEach((listener) => {
            try {
                listener(rem);
            } catch {
                // Ignore listener error
            }
        });
    }
}

export const rateLimitManager = new RateLimitManager();
