// src/utils/__tests__/csvExport.test.ts
import { describe, it, expect } from 'vitest';
import { buildStatementCsv, escapeCsvField, formatIsoForCsv } from '../csvExport';
import { TransactionRecord } from '../../types';

describe('csvExport', () => {
    describe('escapeCsvField', () => {
        it('returns plain string without special chars as-is', () => {
            expect(escapeCsvField('Hello')).toBe('Hello');
            expect(escapeCsvField(123)).toBe('123');
        });

        it('handles null and undefined', () => {
            expect(escapeCsvField(null)).toBe('');
            expect(escapeCsvField(undefined)).toBe('');
        });

        it('escapes fields containing delimiter', () => {
            expect(escapeCsvField('A;B', ';')).toBe('"A;B"');
        });

        it('escapes and doubles quotes', () => {
            expect(escapeCsvField('He said "Hi"')).toBe('"He said ""Hi"""');
        });

        it('escapes newlines within fields', () => {
            expect(escapeCsvField("Line 1\nLine 2")).toBe('"Line 1\nLine 2"');
            expect(escapeCsvField("Line 1\r\nLine 2")).toBe('"Line 1\r\nLine 2"');
        });
    });

    describe('formatIsoForCsv', () => {
        it('formats valid ISO dates into YYYY-MM-DD HH:mm:ss', () => {
            const result = formatIsoForCsv('2026-05-15T14:30:45.000Z');
            expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
        });

        it('returns raw string for invalid dates', () => {
            expect(formatIsoForCsv('not-a-date')).toBe('not-a-date');
        });
    });

    describe('buildStatementCsv', () => {
        const sampleRecords: TransactionRecord[] = [
            {
                id: 'tx-1',
                direction: 'received',
                senderId: 'did:key:z6MkhaXG',
                senderProfileName: 'Alice Müller',
                timestamp: '2026-06-01T10:00:00.000Z',
                summableAmounts: { 'Minuto': '50.00' },
                countableItems: {},
                involvedVouchers: ['v-1'],
                notes: 'Zahlung für Honig & Äpfel mit "Bio"-Zertifikat',
                bundleId: 'bundle-abc-123'
            },
            {
                id: 'tx-2',
                direction: 'sent',
                recipientId: 'did:key:z6MkpTHR',
                timestamp: '2026-06-02T15:30:00.000Z',
                summableAmounts: { 'Minuto': '15.50' },
                countableItems: {},
                involvedVouchers: ['v-2'],
                notes: 'Miete Juni;\nZweite Zeile',
                bundleId: 'bundle-def-456'
            }
        ];

        it('starts with UTF-8 BOM (\\uFEFF)', () => {
            const csv = buildStatementCsv(sampleRecords);
            expect(csv.startsWith('\uFEFF')).toBe(true);
        });

        it('includes standard header line', () => {
            const csv = buildStatementCsv(sampleRecords);
            expect(csv).toContain('Datum;Typ;Gegenpartei;DID;Betrag;Waehrung;Verwendungszweck;Transaktionsnummer');
        });

        it('formats received transactions with positive amounts', () => {
            const csv = buildStatementCsv(sampleRecords);
            expect(csv).toContain('Einnahme');
            expect(csv).toContain('50.00;Minuto');
        });

        it('formats sent transactions with negative amounts', () => {
            const csv = buildStatementCsv(sampleRecords);
            expect(csv).toContain('Ausgabe');
            expect(csv).toContain('-15.50;Minuto');
        });

        it('escapes quotes and newlines in notes correctly', () => {
            const csv = buildStatementCsv(sampleRecords);
            // Notes with quotes: "Bio"-Zertifikat -> ""Bio""-Zertifikat
            expect(csv).toContain('"Zahlung für Honig & Äpfel mit ""Bio""-Zertifikat"');
            // Notes with semicolons and newlines: "Miete Juni;\nZweite Zeile"
            expect(csv).toContain('"Miete Juni;\nZweite Zeile"');
        });

        it('resolves counterparty names via custom resolver option', () => {
            const resolver = (did?: string | null) => did === 'did:key:z6MkpTHR' ? 'Bob Der Baumeister' : 'Unbekannt';
            const csv = buildStatementCsv(sampleRecords, { resolveContactName: resolver });
            expect(csv).toContain('Bob Der Baumeister;did:key:z6MkpTHR');
        });

        it('handles countable items when summable amounts are empty', () => {
            const countableRecord: TransactionRecord[] = [
                {
                    id: 'tx-3',
                    direction: 'received',
                    senderId: 'did:key:z6MkhaXG',
                    timestamp: '2026-06-03T12:00:00.000Z',
                    summableAmounts: {},
                    countableItems: { 'Gutschein': 3 },
                    involvedVouchers: ['v-3'],
                    notes: '3 Einheiten'
                }
            ];
            const csv = buildStatementCsv(countableRecord);
            expect(csv).toContain('3;Gutschein');
        });
    });
});
