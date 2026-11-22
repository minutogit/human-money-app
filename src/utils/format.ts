/**
 * Centralized formatting utilities for the Human Money App.
 */

/**
 * Formats a numeric amount string with localized thousands separators and fixed decimal places.
 * @param amountStr The amount string or number to format
 * @param precision Number of decimal places (default: 2)
 * @returns Formatted string
 */
export function formatAmount(amountStr: string | number, precision: number = 2): string {
    const num = typeof amountStr === 'string' ? parseFloat(amountStr) : amountStr;
    if (isNaN(num)) return amountStr.toString();
    return num.toLocaleString(undefined, { 
        minimumFractionDigits: precision, 
        maximumFractionDigits: precision 
    });
}

/**
 * Formats an ISO date string to a localized medium date format.
 * @param isoString ISO 8601 date string
 * @returns Formatted date string (e.g., "May 3, 2026")
 */
export function formatDate(isoString: string): string {
    if (!isoString) return 'N/A';
    try {
        return new Date(isoString).toLocaleDateString(undefined, {
            year: 'numeric', month: 'short', day: 'numeric'
        });
    } catch {
        return 'Invalid Date';
    }
}

/**
 * Formats an ISO date string to a localized medium date and short time format.
 * @param isoString ISO 8601 date string
 * @returns Formatted date and time string
 */
export function formatDateTime(isoString: string): string {
    if (!isoString) return 'N/A';
    try {
        return new Date(isoString).toLocaleString(undefined, {
            dateStyle: 'medium',
            timeStyle: 'short',
        });
    } catch {
        return 'Invalid Date';
    }
}

/**
 * Formats a transaction summary showing both summable amounts and countable items.
 * @param summable Record of currency/unit to amount strings
 * @param countable Record of currency/unit to counts
 * @returns Combined summary string (e.g., "10.00 EUR, 5 Vouchers")
 */
export function formatSummary(
    summable: Record<string, string> | undefined,
    countable: Record<string, number> | undefined
): string {
    const s = Object.entries(summable || {}).map(([unit, amount]) => {
        return `${formatAmount(amount)} ${unit}`;
    });
    const c = Object.entries(countable || {}).map(([unit, total]) => {
        return `${total} ${unit}${total > 1 ? 's' : ''}`;
    });
    const all = [...s, ...c];
    return all.length > 0 ? all.join(', ') : '0.00';
}

const NON_NEGATIVE_DECIMAL_RE = /^\d+(?:[.,]\d+)?$/;

/**
 * Returns the fractional digit count of a decimal string (`trimmed` ignores trailing zeros).
 */
export function getDecimalFractionLength(raw: string, trimmed: boolean = false): number {
    const normalized = String(raw).trim().replace(',', '.').replace(/^-/, '');
    const m = normalized.match(/^\d+(?:\.(\d+))?$/);
    if (!m?.[1]) return 0;
    const frac = trimmed ? m[1].replace(/0+$/, '') : m[1];
    return frac.length;
}

/**
 * Parses a non-negative decimal string into a scaled `bigint` without floating-point rounding.
 */
export function parseDecimalToScaledBigInt(raw: string, scale: number): bigint | null {
    const trimmed = raw.trim();
    if (!NON_NEGATIVE_DECIMAL_RE.test(trimmed)) return null;
    const normalized = trimmed.replace(',', '.');
    const [intPart, fracPart = ''] = normalized.split('.');
    if (fracPart.length > scale) return null;
    return BigInt(`${intPart}${fracPart.padEnd(scale, '0')}`);
}

/**
 * Formats a signed or unsigned scaled `bigint` back into an exact canonical decimal string.
 */
export function formatScaledBigInt(value: bigint, scale: number): string {
    const sign = value < 0n ? '-' : '';
    const abs = value < 0n ? -value : value;
    if (scale <= 0) return `${sign}${abs.toString()}`;
    const s = abs.toString().padStart(scale + 1, '0');
    return `${sign}${s.slice(0, -scale)}.${s.slice(-scale)}`;
}

/**
 * Sums non-negative decimal strings exactly via scaled `bigint` arithmetic.
 */
export function sumExactDecimalStrings(amounts: string[]): {
    exact: string;
    number: number;
    scale: number;
} {
    const scale = Math.max(0, ...amounts.map((a) => getDecimalFractionLength(a, false)));
    let totalScaled = 0n;
    for (const amt of amounts) {
        totalScaled += parseDecimalToScaledBigInt(amt, scale) ?? 0n;
    }
    const exact = formatScaledBigInt(totalScaled, scale);
    return { exact, number: Number(exact), scale };
}
