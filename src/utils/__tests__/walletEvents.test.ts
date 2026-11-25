// src/utils/__tests__/walletEvents.test.ts
import { describe, it, expect, vi } from 'vitest';
import {
    normalizeWalletEventKind,
    getWalletEventDirection,
    isTransferWalletEvent,
    resolveEventCounterpartyLabel,
    formatWalletEventAmount,
    getWalletEventPresentation,
    groupWalletEventsByDate,
    summarizeWalletEventAmounts,
    matchEventsForTransactionRecord,
    summarizeTransactionHistory,
    formatSignedNetBalance,
    formatRelativeExpiry,
} from '../walletEvents';
import type { TransactionRecord, WalletEvent } from '../../types';

const translateMock = (key: string, params?: Record<string, unknown>) =>
    params ? `${key}:${JSON.stringify(params)}` : key;

const makeEvent = (overrides: Partial<WalletEvent>): WalletEvent => ({
    eventId: 'ev-1',
    localInstanceId: 'inst-1',
    voucherId: 'voucher-1',
    timestamp: '2026-06-01T12:00:00.000Z',
    eventType: 'transferReceived',
    bffData: {
        displayCurrency: 'Minuto',
        amount: '50.00',
        isTestVoucher: false,
        counterpartyId: 'did:key:z6MkhaXGr5zExample1234567890',
        counterpartyName: 'Alice',
    },
    ...overrides,
});

describe('walletEvents utilities', () => {
    it('normalizes known and unknown event types and directions', () => {
        expect(normalizeWalletEventKind('voucherCreated')).toBe('created');
        expect(normalizeWalletEventKind('transferSent')).toBe('sent');
        expect(normalizeWalletEventKind('transferReceived')).toBe('received');
        expect(normalizeWalletEventKind({ unknown: 'custom' })).toBe('unknown');

        const sent = makeEvent({ eventType: 'transferSent' });
        const received = makeEvent({ eventType: 'transferReceived' });
        const quarantined = makeEvent({ eventType: 'voucherQuarantined' });

        expect(getWalletEventDirection(sent)).toBe('out');
        expect(getWalletEventDirection(received)).toBe('in');
        expect(getWalletEventDirection(quarantined)).toBe('neutral');
        expect(isTransferWalletEvent(sent)).toBe(true);
        expect(isTransferWalletEvent(quarantined)).toBe(false);
    });

    it('resolves counterparty label with resolver fallback and anonymous handling', () => {
        const resolver = vi.fn(() => 'Resolved Bob');
        const ev = makeEvent({ eventType: 'transferSent' });
        expect(resolveEventCounterpartyLabel(ev, resolver)).toBe('Resolved Bob');
        expect(resolver).toHaveBeenCalledWith(
            'did:key:z6MkhaXGr5zExample1234567890',
            'Alice',
        );

        const anonEv = makeEvent({
            bffData: {
                displayCurrency: 'Taler',
                amount: '10',
                isTestVoucher: false,
                counterpartyId: 'did:key:z6MkpTHR8VNsBxYExample999999999',
                counterpartyName: 'Anonymous',
            },
        });
        expect(resolveEventCounterpartyLabel(anonEv)).toContain('did:key:z6MkpTHR');
    });

    it('formats presentation differently for dashboard vs history contexts', () => {
        const ev = makeEvent({ eventType: 'transferSent' });
        const dash = getWalletEventPresentation(ev, 'dashboard', translateMock);
        const hist = getWalletEventPresentation(ev, 'history', translateMock);

        expect(dash.kind).toBe('sent');
        expect(dash.label).toContain('dashboard.eventSent');
        expect(dash.label).toContain('Alice');
        expect(dash.amountDisplay).toBe('50.00 Minuto');
        expect(hist.label).toContain('history.eventSent');
        expect(hist.label).toContain('Alice');
        expect(formatWalletEventAmount(ev)).toBe('50.00 Minuto');
    });

    it('groups events by UTC date descending and summarizes currency flows', () => {
        const events: WalletEvent[] = [
            makeEvent({
                eventId: 'e1',
                timestamp: '2026-06-01T10:00:00.000Z',
                eventType: 'transferReceived',
                bffData: { displayCurrency: 'Minuto', amount: '100', isTestVoucher: false },
            }),
            makeEvent({
                eventId: 'e2',
                timestamp: '2026-06-02T09:00:00.000Z',
                eventType: 'transferSent',
                bffData: { displayCurrency: 'Minuto', amount: '35.5', isTestVoucher: false },
            }),
            makeEvent({
                eventId: 'e3',
                timestamp: '2026-06-02T15:00:00.000Z',
                eventType: 'voucherCreated',
                bffData: { displayCurrency: 'Taler', amount: '20', isTestVoucher: false },
            }),
        ];

        const grouped = groupWalletEventsByDate(events);
        expect(grouped.map((g) => g.dateKey)).toEqual(['2026-06-02', '2026-06-01']);
        expect(grouped[0].events.map((e) => e.eventId)).toEqual(['e3', 'e2']);

        const summary = summarizeWalletEventAmounts(events);
        expect(summary.Minuto).toEqual({ inbound: 100, outbound: 35.5, net: 64.5 });
        expect(summary.Taler).toBeUndefined();
    });

    it('matches transaction records to events by voucher ID or 60s window and summarizes records', () => {
        const records: TransactionRecord[] = [
            {
                id: 'tx-1',
                direction: 'received',
                senderId: 'did:key:sender',
                timestamp: '2026-06-01T12:00:00.000Z',
                summableAmounts: { Minuto: '40' },
                countableItems: {},
                involvedVouchers: ['inst-1'],
            },
            {
                id: 'tx-2',
                direction: 'sent',
                recipientId: 'did:key:recip',
                timestamp: '2026-06-02T12:00:00.000Z',
                summableAmounts: { FreeTaler: '15' },
                countableItems: {},
                involvedVouchers: [],
            },
        ];

        const events: WalletEvent[] = [
            makeEvent({ eventId: 'match-id', localInstanceId: 'inst-1', timestamp: '2026-05-01T00:00:00.000Z' }),
            makeEvent({
                eventId: 'match-time',
                localInstanceId: 'other',
                voucherId: 'other',
                eventType: 'transferSent',
                timestamp: '2026-06-02T12:00:30.000Z',
            }),
            makeEvent({
                eventId: 'no-match',
                localInstanceId: 'unrelated',
                voucherId: 'unrelated',
                eventType: 'transferSent',
                timestamp: '2026-06-05T12:00:00.000Z',
            }),
        ];

        expect(matchEventsForTransactionRecord(records[0], events).map((e) => e.eventId)).toEqual([
            'match-id',
        ]);
        expect(matchEventsForTransactionRecord(records[1], events).map((e) => e.eventId)).toEqual([
            'match-time',
        ]);

        const txSummary = summarizeTransactionHistory(records);
        expect(txSummary.totalBookings).toBe(2);
        expect(txSummary.inboundSums).toEqual({ Minuto: 40 });
        expect(txSummary.outboundSums).toEqual({ FreeTaler: 15 });
        expect(txSummary.summaryUnits).toEqual(['FreeTaler', 'Minuto']);
    });

    it('formats signed net balances and relative expiry labels', () => {
        expect(formatSignedNetBalance('700')).toMatch(/^\+700[.,]00$/);
        expect(formatSignedNetBalance('-300')).toMatch(/^-300[.,]00$/);
        expect(formatSignedNetBalance('0')).toBe('0.00');
        expect(formatSignedNetBalance(undefined)).toBe('0.00');

        const now = Date.UTC(2026, 5, 1, 12, 0, 0);
        const in10Days = new Date(now + 10 * 24 * 60 * 60 * 1000).toISOString();
        const expired = new Date(now - 2 * 24 * 60 * 60 * 1000).toISOString();
        expect(formatRelativeExpiry(in10Days, translateMock, now)).toBe(
            'dashboard.expiresInDays:{"count":10}',
        );
        expect(formatRelativeExpiry(expired, translateMock, now)).toBe(
            'dashboard.voucherExpiredShort',
        );
    });

    it('avoids floating-point drift on 0.1 + 0.2 and preserves net balance precision > 2 decimals', () => {
        const events: WalletEvent[] = [
            makeEvent({
                eventId: 'd1',
                eventType: 'transferReceived',
                bffData: { displayCurrency: 'Minuto', amount: '0.1', isTestVoucher: false },
            }),
            makeEvent({
                eventId: 'd2',
                eventType: 'transferReceived',
                bffData: { displayCurrency: 'Minuto', amount: '0.2', isTestVoucher: false },
            }),
            makeEvent({
                eventId: 'd3',
                eventType: 'transferSent',
                bffData: { displayCurrency: 'Minuto', amount: '0.15', isTestVoucher: false },
            }),
        ];
        const summary = summarizeWalletEventAmounts(events);
        expect(summary.Minuto.inbound).toBe(0.3);
        expect(summary.Minuto.outbound).toBe(0.15);
        expect(summary.Minuto.net).toBe(0.15);

        const txSummary = summarizeTransactionHistory([
            {
                id: 'r1',
                direction: 'received',
                senderId: 'did:key:a',
                timestamp: '2026-06-01T00:00:00Z',
                summableAmounts: { Minuto: '0.1' },
                countableItems: {},
                involvedVouchers: [],
            },
            {
                id: 'r2',
                direction: 'received',
                senderId: 'did:key:b',
                timestamp: '2026-06-01T00:01:00Z',
                summableAmounts: { Minuto: '0.2' },
                countableItems: {},
                involvedVouchers: [],
            },
        ]);
        expect(txSummary.inboundSums.Minuto).toBe(0.3);

        expect(formatSignedNetBalance('1.2345')).toMatch(/^\+1[.,]2345$/);
        expect(formatSignedNetBalance('-0.005')).toMatch(/^-0[.,]005$/);
    });

    it('maps activated, voided, expired, quarantined, and unknown kinds across direction and contexts', () => {
        const cases: Array<{
            eventType: WalletEvent['eventType'];
            kind: ReturnType<typeof normalizeWalletEventKind>;
            direction: 'in' | 'out' | 'neutral';
            dashLabel: string;
            histLabel: string;
        }> = [
            { eventType: 'voucherActivated', kind: 'activated', direction: 'in', dashLabel: 'dashboard.eventVoucherActivated', histLabel: 'history.eventActivated' },
            { eventType: 'voucherVoided', kind: 'voided', direction: 'out', dashLabel: 'dashboard.eventVoucherVoided', histLabel: 'history.eventVoided' },
            { eventType: 'voucherExpired', kind: 'expired', direction: 'out', dashLabel: 'dashboard.eventVoucherExpired', histLabel: 'history.eventExpired' },
            { eventType: 'voucherQuarantined', kind: 'quarantined', direction: 'neutral', dashLabel: 'dashboard.eventVoucherQuarantined', histLabel: 'history.eventQuarantine' },
            { eventType: { unknown: 'mystery' }, kind: 'unknown', direction: 'neutral', dashLabel: 'dashboard.eventUnknown', histLabel: 'history.eventDefault' },
        ];

        for (const c of cases) {
            const ev = makeEvent({ eventType: c.eventType });
            expect(normalizeWalletEventKind(c.eventType)).toBe(c.kind);
            expect(getWalletEventDirection(ev)).toBe(c.direction);
            expect(isTransferWalletEvent(ev)).toBe(false);
            expect(getWalletEventPresentation(ev, 'dashboard', translateMock).label).toBe(c.dashLabel);
            expect(getWalletEventPresentation(ev, 'history', translateMock).label).toBe(c.histLabel);
        }
    });

    it('handles invalid timestamps, missing currency/counterparty fallbacks, and skips malformed amounts in summaries', () => {
        const ev = makeEvent({
            eventId: 'bad-ts',
            timestamp: 'not-a-valid-date',
            bffData: {
                displayCurrency: '',
                amount: 'not-a-number',
                isTestVoucher: false,
                counterpartyId: '',
                counterpartyName: '',
            },
        });

        expect(resolveEventCounterpartyLabel(ev)).toBe('Unknown');
        expect(formatWalletEventAmount(ev)).toBe('');
        expect(groupWalletEventsByDate([ev])).toEqual([{ dateKey: 'unknown', events: [ev] }]);
        expect(summarizeWalletEventAmounts([ev])).toEqual({});

        const txSummary = summarizeTransactionHistory([
            {
                id: 'tx-bad',
                direction: 'received',
                senderId: 'did:key:s',
                timestamp: '2026-06-01T00:00:00Z',
                summableAmounts: { Minuto: 'invalid' },
                countableItems: {},
                involvedVouchers: [],
            },
        ]);
        expect(txSummary).toEqual({
            totalBookings: 1,
            inboundSums: {},
            outboundSums: {},
            summaryUnits: [],
        });
    });

    it('matches transaction records via involvedSourcesDetails and enforces 60s inclusive boundary and direction', () => {
        const record: TransactionRecord = {
            id: 'tx-bound',
            direction: 'sent',
            recipientId: 'did:key:r',
            timestamp: '2026-06-02T12:00:00.000Z',
            summableAmounts: { Minuto: '10' },
            countableItems: {},
            involvedVouchers: [],
            involvedSourcesDetails: [
                {
                    localInstanceId: 'src-inst-1',
                    voucherId: 'v-src',
                    standardName: 'Minuto',
                    amount: '10',
                    unit: 'Minuto',
                    isTestVoucher: false,
                    displayCurrency: 'Minuto',
                    displayStandardName: 'Minuto',
                },
            ],
        };

        const srcMatch = makeEvent({
            eventId: 'src-match',
            localInstanceId: 'src-inst-1',
            voucherId: 'v-other',
            timestamp: '2026-01-01T00:00:00.000Z',
        });
        const exact60sMatch = makeEvent({
            eventId: 'exact-60s',
            localInstanceId: 'none',
            voucherId: 'none',
            eventType: 'transferSent',
            timestamp: '2026-06-02T12:01:00.000Z',
        });
        const over60sNoMatch = makeEvent({
            eventId: 'over-60s',
            localInstanceId: 'none',
            voucherId: 'none',
            eventType: 'transferSent',
            timestamp: '2026-06-02T12:01:00.001Z',
        });
        const wrongDirNoMatch = makeEvent({
            eventId: 'wrong-dir',
            localInstanceId: 'none',
            voucherId: 'none',
            eventType: 'transferReceived',
            timestamp: '2026-06-02T12:00:01.000Z',
        });

        expect(
            matchEventsForTransactionRecord(record, [srcMatch, exact60sMatch, over60sNoMatch, wrongDirNoMatch]).map(
                (e) => e.eventId,
            ),
        ).toEqual(['src-match', 'exact-60s']);
    });

    it('covers formatRelativeExpiry edge cases and formatSignedNetBalance NaN pass-through', () => {
        const now = Date.UTC(2026, 5, 1, 12, 0, 0);
        expect(formatRelativeExpiry(undefined, translateMock, now)).toBe('');
        expect(formatRelativeExpiry('invalid-date', translateMock, now)).toBe('');
        expect(formatRelativeExpiry(new Date(now).toISOString(), translateMock, now)).toBe(
            'dashboard.expiresToday',
        );
        expect(formatRelativeExpiry(new Date(now + 90 * 24 * 60 * 60 * 1000).toISOString(), translateMock, now)).toBe(
            'dashboard.expiresInMonths:{"count":3}',
        );
        expect(formatRelativeExpiry(new Date(now + 800 * 24 * 60 * 60 * 1000).toISOString(), translateMock, now)).toBe(
            'dashboard.expiresInYears:{"count":2}',
        );
        expect(formatSignedNetBalance('not-a-number')).toBe('not-a-number');
    });
});
