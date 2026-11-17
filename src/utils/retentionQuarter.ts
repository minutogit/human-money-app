// src/utils/retentionQuarter.ts

export const MIN_QUARTER_YEAR = 2020;
export const MAX_QUARTER_YEAR = 9999;

/** Encodes (year, quarter 1..4) identically to humoco-proto::retention. */
export function encodeQuarter(year: number, quarter: number): number | null {
    if (
        !Number.isInteger(year) ||
        !Number.isInteger(quarter) ||
        year < MIN_QUARTER_YEAR ||
        year > MAX_QUARTER_YEAR ||
        quarter < 1 ||
        quarter > 4
    ) {
        return null;
    }
    return year * 4 + (quarter - 1);
}

/** Parses "YYYY-QN" (e.g. "2031-Q4") into encoded quarter u16, or null. */
export function parseQuarterStr(raw: string): number | null {
    const trimmed = raw.trim().toUpperCase();
    const match = /^(\d{4})-Q([1-4])$/.exec(trimmed);
    if (!match) return null;
    const year = Number.parseInt(match[1], 10);
    const quarter = Number.parseInt(match[2], 10);
    return encodeQuarter(year, quarter);
}

/** Formats encoded quarter u16 into "YYYY-QN". */
export function formatQuarter(encoded: number | null | undefined): string {
    if (encoded === null || encoded === undefined || !Number.isFinite(encoded) || encoded < MIN_QUARTER_YEAR * 4) {
        return '—';
    }
    const year = Math.floor(encoded / 4);
    const quarter = (encoded % 4) + 1;
    return `${String(year).padStart(4, '0')}-Q${quarter}`;
}

/** Returns UTC end-of-quarter timestamp in milliseconds (inclusive). */
export function quarterEndUtcMs(encoded: number | null | undefined): number | null {
    if (encoded === null || encoded === undefined || !Number.isFinite(encoded) || encoded < MIN_QUARTER_YEAR * 4) {
        return null;
    }
    const year = Math.floor(encoded / 4);
    const quarter = (encoded % 4) + 1;
    return Date.UTC(year, quarter * 3, 0, 23, 59, 59, 999);
}

/** Parses simple ISO-8601 duration strings like "P3Y", "P6M", "P30D" into ms (conservative upper bound). */
export function estimateIsoDurationMs(iso: string | null | undefined): number {
    if (!iso) return 0;
    const match = /^P(?:(\d+)Y)?(?:(\d+)M)?(?:(\d+)D)?$/i.exec(iso.trim());
    if (!match) return 0;
    const years = match[1] ? Number.parseInt(match[1], 10) : 0;
    const months = match[2] ? Number.parseInt(match[2], 10) : 0;
    const days = match[3] ? Number.parseInt(match[3], 10) : 0;
    const dayMs = 86_400_000;
    return (years * 366 + months * 31 + days) * dayMs;
}

/** Suggests a default quarter string ("YYYY-QN") N years from now. */
export function defaultQuarterYearsAhead(yearsAhead: number): string {
    const now = new Date();
    const targetYear = Math.min(MAX_QUARTER_YEAR, Math.max(MIN_QUARTER_YEAR, now.getUTCFullYear() + yearsAhead));
    const currentQuarter = Math.floor(now.getUTCMonth() / 3) + 1;
    return `${targetYear}-Q${currentQuarter}`;
}
