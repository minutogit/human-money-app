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
    const s = Object.entries(summable || {}).map(([unit, amount]) => `${formatAmount(amount)} ${unit}`);
    const c = Object.entries(countable || {}).map(([unit, total]) => `${total} ${unit}${total > 1 ? 's' : ''}`);
    const all = [...s, ...c];
    return all.length > 0 ? all.join(', ') : '0.00';
}
