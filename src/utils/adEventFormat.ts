// src/utils/adEventFormat.ts
import { AdKind } from '../types';

export const MAX_AD_TEXT_CHARS = 500;
export const MAX_ADS_PER_CARD = 10;

export function pad2(n: number): string {
    return String(n).padStart(2, '0');
}

/**
 * Formats an event timestamp for display on AdCard and MineTab.
 * All-day events are stored as noon UTC (`12:00:00Z`) and formatted in UTC
 * so the calendar date stays identical across all client timezones.
 */
export function formatEventDateTime(
    eventAtSecs: number,
    eventAllDay?: boolean,
    locale?: string,
): string {
    const date = new Date(eventAtSecs * 1000);
    const now = new Date();
    const tz = eventAllDay ? 'UTC' : undefined;
    const includeYear =
        (eventAllDay ? date.getUTCFullYear() : date.getFullYear()) !== now.getFullYear();

    const datePart = new Intl.DateTimeFormat(locale || undefined, {
        weekday: 'short',
        day: '2-digit',
        month: '2-digit',
        ...(includeYear ? { year: 'numeric' } : {}),
        ...(tz ? { timeZone: tz } : {}),
    }).format(date);

    if (eventAllDay) {
        return datePart;
    }

    const timePart = new Intl.DateTimeFormat(locale || undefined, {
        hour: '2-digit',
        minute: '2-digit',
    }).format(date);

    return `${datePart} · ${timePart}`;
}

/**
 * Converts a stored Unix timestamp to a `<input type="date">` string (`YYYY-MM-DD`).
 */
export function toEventDateInputValue(eventAtSecs: number, eventAllDay?: boolean): string {
    const d = new Date(eventAtSecs * 1000);
    if (eventAllDay) {
        return `${d.getUTCFullYear()}-${pad2(d.getUTCMonth() + 1)}-${pad2(d.getUTCDate())}`;
    }
    return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/**
 * Converts a stored Unix timestamp to a `<input type="time">` string (`HH:MM`),
 * or returns `""` for all-day events.
 */
export function toEventTimeInputValue(eventAtSecs: number, eventAllDay?: boolean): string {
    if (eventAllDay) return '';
    const d = new Date(eventAtSecs * 1000);
    return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

/**
 * Parses date (`YYYY-MM-DD`) and optional time (`HH:MM`) inputs into a Unix timestamp.
 * All-day events normalize to noon UTC (`12:00:00Z`) to remain timezone-invariant.
 */
export function parseEventInputsToTimestamp(
    dateStr: string,
    timeStr: string,
): { eventAt: number; eventAllDay: boolean } | null {
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dateStr.trim());
    if (!m) return null;
    const year = Number(m[1]);
    const month = Number(m[2]);
    const day = Number(m[3]);
    if (month < 1 || month > 12 || day < 1 || day > 31) return null;

    const trimmedTime = timeStr.trim();
    if (!trimmedTime) {
        const utcMs = Date.UTC(year, month - 1, day, 12, 0, 0, 0);
        const check = new Date(utcMs);
        if (
            check.getUTCFullYear() !== year ||
            check.getUTCMonth() !== month - 1 ||
            check.getUTCDate() !== day
        ) {
            return null;
        }
        return { eventAt: Math.floor(utcMs / 1000), eventAllDay: true };
    }

    const tm = /^(\d{2}):(\d{2})$/.exec(trimmedTime);
    if (!tm) return null;
    const hours = Number(tm[1]);
    const minutes = Number(tm[2]);
    if (hours > 23 || minutes > 59) return null;

    const local = new Date(year, month - 1, day, hours, minutes, 0, 0);
    if (
        local.getFullYear() !== year ||
        local.getMonth() !== month - 1 ||
        local.getDate() !== day
    ) {
        return null;
    }
    return { eventAt: Math.floor(local.getTime() / 1000), eventAllDay: false };
}

export function offsetDaysDateInputValue(daysFromToday: number): string {
    const d = new Date();
    d.setDate(d.getDate() + daysFromToday);
    return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

export function getAdKindBadgeClass(kind: AdKind): string {
    if (kind === 'offer') {
        return 'bg-emerald-50 text-emerald-700 border border-emerald-200';
    }
    if (kind === 'request') {
        return 'bg-sky-50 text-sky-700 border border-sky-200';
    }
    return 'bg-violet-50 text-violet-700 border border-violet-200';
}
