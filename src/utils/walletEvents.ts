// src/utils/walletEvents.ts
import type { TransactionRecord, WalletEvent } from '../types';
import {
    formatAmount,
    formatScaledBigInt,
    getDecimalFractionLength,
    parseDecimalToScaledBigInt,
    sumExactDecimalStrings,
} from './format';
import { truncateUserId } from './userIdHelper';

export type WalletEventKind =
    | 'created'
    | 'sent'
    | 'received'
    | 'quarantined'
    | 'activated'
    | 'voided'
    | 'expired'
    | 'unknown';

export type WalletEventDirection = 'in' | 'out' | 'neutral';

export type TranslateFn = (key: string, params?: Record<string, unknown>) => string;
export type ResolveIdentityFn = (did?: string | null, providedName?: string | null) => string;

export interface WalletEventPresentation {
    kind: WalletEventKind;
    direction: WalletEventDirection;
    isTransfer: boolean;
    label: string;
    color: string;
    bgColor: string;
    amountDisplay: string;
}

export interface WalletEventDateGroup {
    dateKey: string;
    events: WalletEvent[];
}

export interface CurrencyFlowSummary {
    inbound: number;
    outbound: number;
    net: number;
}

export interface TransactionHistorySummary {
    totalBookings: number;
    inboundSums: Record<string, number>;
    outboundSums: Record<string, number>;
    summaryUnits: string[];
}

const EVENT_TYPE_TO_KIND: Record<string, WalletEventKind> = {
    voucherCreated: 'created',
    transferSent: 'sent',
    transferReceived: 'received',
    voucherQuarantined: 'quarantined',
    voucherActivated: 'activated',
    voucherVoided: 'voided',
    voucherExpired: 'expired',
};

const EVENT_STYLE_BY_KIND: Record<WalletEventKind, { color: string; bgColor: string }> = {
    created: { color: 'text-blue-600', bgColor: 'bg-blue-50' },
    sent: { color: 'text-rose-600', bgColor: 'bg-rose-50' },
    received: { color: 'text-emerald-600', bgColor: 'bg-emerald-50' },
    quarantined: { color: 'text-amber-600', bgColor: 'bg-amber-50' },
    activated: { color: 'text-emerald-600', bgColor: 'bg-emerald-50' },
    voided: { color: 'text-gray-600', bgColor: 'bg-gray-50' },
    expired: { color: 'text-orange-600', bgColor: 'bg-orange-50' },
    unknown: { color: 'text-gray-600', bgColor: 'bg-gray-50' },
};

export function normalizeWalletEventKind(eventType: WalletEvent['eventType']): WalletEventKind {
    if (typeof eventType === 'string' && eventType in EVENT_TYPE_TO_KIND) {
        return EVENT_TYPE_TO_KIND[eventType];
    }
    return 'unknown';
}

export function getWalletEventDirection(event: WalletEvent): WalletEventDirection {
    const kind = normalizeWalletEventKind(event.eventType);
    if (kind === 'received' || kind === 'created' || kind === 'activated') return 'in';
    if (kind === 'sent' || kind === 'voided' || kind === 'expired') return 'out';
    return 'neutral';
}

export function isTransferWalletEvent(event: WalletEvent): boolean {
    const kind = normalizeWalletEventKind(event.eventType);
    return kind === 'sent' || kind === 'received';
}

export function resolveEventCounterpartyLabel(
    event: WalletEvent,
    resolveIdentity?: ResolveIdentityFn,
): string {
    const { counterpartyId, counterpartyName } = event.bffData;
    if (resolveIdentity) {
        return resolveIdentity(counterpartyId, counterpartyName);
    }
    if (
        counterpartyName &&
        counterpartyName.trim() !== '' &&
        counterpartyName.toLowerCase() !== 'anonymous'
    ) {
        return counterpartyName;
    }
    if (counterpartyId) {
        return truncateUserId(counterpartyId);
    }
    return 'Unknown';
}

export function formatWalletEventAmount(event: WalletEvent): string {
    const { amount, displayCurrency } = event.bffData;
    if (!displayCurrency) return '';
    return `${amount} ${displayCurrency}`;
}

/**
 * Formats a WalletEvent into display metadata (label, direction, Tailwind palette, amount text)
 * for either the Dashboard or Transaction History views.
 */
export function getWalletEventPresentation(
    event: WalletEvent,
    context: 'dashboard' | 'history',
    translate: TranslateFn,
    resolveIdentity?: ResolveIdentityFn,
): WalletEventPresentation {
    const kind = normalizeWalletEventKind(event.eventType);
    const direction = getWalletEventDirection(event);
    const { color, bgColor } = EVENT_STYLE_BY_KIND[kind];
    const counterparty = resolveEventCounterpartyLabel(event, resolveIdentity);
    const amountDisplay = formatWalletEventAmount(event);

    let label: string;
    if (context === 'dashboard') {
        switch (kind) {
            case 'created':
                label = translate('dashboard.eventVoucherCreated');
                break;
            case 'sent':
                label = translate('dashboard.eventSent', { name: counterparty });
                break;
            case 'received':
                label = translate('dashboard.eventReceived', { name: counterparty });
                break;
            case 'quarantined':
                label = translate('dashboard.eventVoucherQuarantined');
                break;
            case 'activated':
                label = translate('dashboard.eventVoucherActivated');
                break;
            case 'voided':
                label = translate('dashboard.eventVoucherVoided');
                break;
            case 'expired':
                label = translate('dashboard.eventVoucherExpired');
                break;
            default:
                label = translate('dashboard.eventUnknown');
        }
    } else {
        switch (kind) {
            case 'created':
                label = translate('history.eventCreated');
                break;
            case 'sent':
                label = translate('history.eventSent', { recipient: counterparty });
                break;
            case 'received':
                label = translate('history.eventReceived', { sender: counterparty });
                break;
            case 'quarantined':
                label = translate('history.eventQuarantine');
                break;
            case 'activated':
                label = translate('history.eventActivated');
                break;
            case 'voided':
                label = translate('history.eventVoided');
                break;
            case 'expired':
                label = translate('history.eventExpired');
                break;
            default:
                label = translate('history.eventDefault');
        }
    }

    return {
        kind,
        direction,
        isTransfer: kind === 'sent' || kind === 'received',
        label,
        color,
        bgColor,
        amountDisplay,
    };
}

/**
 * Groups wallet events by UTC calendar day (YYYY-MM-DD), ordered newest day first.
 */
export function groupWalletEventsByDate(events: WalletEvent[]): WalletEventDateGroup[] {
    const sorted = [...events].sort(
        (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime(),
    );
    const groups = new Map<string, WalletEvent[]>();

    for (const ev of sorted) {
        const parsed = new Date(ev.timestamp);
        const dateKey = Number.isNaN(parsed.getTime())
            ? 'unknown'
            : parsed.toISOString().slice(0, 10);
        const existing = groups.get(dateKey);
        if (existing) {
            existing.push(ev);
        } else {
            groups.set(dateKey, [ev]);
        }
    }

    return Array.from(groups.entries()).map(([dateKey, groupEvents]) => ({
        dateKey,
        events: groupEvents,
    }));
}

/**
 * Aggregates transfer inbound/outbound/net totals per currency across a list of wallet events.
 */
export function summarizeWalletEventAmounts(
    events: WalletEvent[],
): Record<string, CurrencyFlowSummary> {
    const grouped: Record<string, { inbound: string[]; outbound: string[] }> = {};

    for (const ev of events) {
        const kind = normalizeWalletEventKind(ev.eventType);
        if (kind !== 'sent' && kind !== 'received') continue;
        const unit = ev.bffData.displayCurrency;
        const amt = String(ev.bffData.amount ?? '').trim();
        if (!unit || parseDecimalToScaledBigInt(amt, getDecimalFractionLength(amt, false)) === null) {
            continue;
        }

        if (!grouped[unit]) {
            grouped[unit] = { inbound: [], outbound: [] };
        }
        if (kind === 'received') {
            grouped[unit].inbound.push(amt);
        } else {
            grouped[unit].outbound.push(amt);
        }
    }

    const totals: Record<string, CurrencyFlowSummary> = {};
    for (const [unit, { inbound, outbound }] of Object.entries(grouped)) {
        const all = [...inbound, ...outbound];
        const scale = Math.max(0, ...all.map((a) => getDecimalFractionLength(a, false)));
        let inboundScaled = 0n;
        for (const a of inbound) {
            inboundScaled += parseDecimalToScaledBigInt(a, scale) ?? 0n;
        }
        let outboundScaled = 0n;
        for (const a of outbound) {
            outboundScaled += parseDecimalToScaledBigInt(a, scale) ?? 0n;
        }
        const netScaled = inboundScaled - outboundScaled;
        totals[unit] = {
            inbound: Number(formatScaledBigInt(inboundScaled, scale)),
            outbound: Number(formatScaledBigInt(outboundScaled, scale)),
            net: Number(formatScaledBigInt(netScaled, scale)),
        };
    }

    return totals;
}

/**
 * Finds cryptographic wallet events matching a transaction history record by
 * voucher instance ID or same-direction 60-second timestamp proximity.
 */
export function matchEventsForTransactionRecord(
    record: TransactionRecord,
    events: WalletEvent[],
): WalletEvent[] {
    const recordMs = new Date(record.timestamp).getTime();
    return events.filter((ev) => {
        if (record.involvedVouchers?.includes(ev.localInstanceId)) return true;
        if (record.involvedVouchers?.includes(ev.voucherId)) return true;
        if (record.involvedSourcesDetails?.some((d) => d.localInstanceId === ev.localInstanceId)) {
            return true;
        }
        const matchesDirection =
            (record.direction === 'sent' && ev.eventType === 'transferSent') ||
            (record.direction === 'received' && ev.eventType === 'transferReceived');
        if (matchesDirection && !Number.isNaN(recordMs)) {
            const evMs = new Date(ev.timestamp).getTime();
            if (!Number.isNaN(evMs) && Math.abs(evMs - recordMs) <= 60_000) return true;
        }
        return false;
    });
}

/**
 * Computes per-currency inbound/outbound totals for a filtered TransactionRecord list.
 */
export function summarizeTransactionHistory(
    records: TransactionRecord[],
): TransactionHistorySummary {
    const inboundGrouped: Record<string, string[]> = {};
    const outboundGrouped: Record<string, string[]> = {};

    for (const record of records) {
        const targetMap = record.direction === 'sent' ? outboundGrouped : inboundGrouped;
        if (record.summableAmounts) {
            for (const [unit, amountStr] of Object.entries(record.summableAmounts)) {
                const raw = String(amountStr ?? '').trim();
                if (parseDecimalToScaledBigInt(raw, getDecimalFractionLength(raw, false)) !== null) {
                    if (!targetMap[unit]) targetMap[unit] = [];
                    targetMap[unit].push(raw);
                }
            }
        }
    }

    const inboundSums: Record<string, number> = {};
    for (const [unit, amounts] of Object.entries(inboundGrouped)) {
        inboundSums[unit] = sumExactDecimalStrings(amounts).number;
    }

    const outboundSums: Record<string, number> = {};
    for (const [unit, amounts] of Object.entries(outboundGrouped)) {
        outboundSums[unit] = sumExactDecimalStrings(amounts).number;
    }

    const summaryUnits = Array.from(
        new Set([...Object.keys(inboundSums), ...Object.keys(outboundSums)]),
    ).sort();

    return {
        totalBookings: records.length,
        inboundSums,
        outboundSums,
        summaryUnits,
    };
}

export function formatSignedNetBalance(amountStr?: string): string {
    if (!amountStr) return '0.00';
    const trimmed = amountStr.trim();
    const num = parseFloat(trimmed);
    if (Number.isNaN(num)) return amountStr;
    if (num === 0) return '0.00';
    const decimals = Math.min(18, Math.max(2, getDecimalFractionLength(trimmed, true)));
    const formatted = formatAmount(Math.abs(num), decimals);
    return num > 0 ? `+${formatted}` : `-${formatted}`;
}

export function formatRelativeExpiry(
    validUntil: string | undefined,
    translate: TranslateFn,
    nowMs: number = Date.now(),
): string {
    if (!validUntil) return '';
    const targetMs = new Date(validUntil).getTime();
    if (Number.isNaN(targetMs)) return '';
    const diffDays = Math.ceil((targetMs - nowMs) / (24 * 60 * 60 * 1000));
    if (diffDays < 0) return translate('dashboard.voucherExpiredShort');
    if (diffDays === 0) return translate('dashboard.expiresToday');
    if (diffDays < 60) return translate('dashboard.expiresInDays', { count: diffDays });
    if (diffDays < 730) {
        return translate('dashboard.expiresInMonths', {
            count: Math.max(1, Math.round(diffDays / 30)),
        });
    }
    return translate('dashboard.expiresInYears', {
        count: Math.max(1, Math.round(diffDays / 365)),
    });
}
