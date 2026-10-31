// src/utils/csvExport.ts
import { TransactionRecord } from '../types';

export interface StatementCsvOptions {
    delimiter?: string;
    resolveContactName?: (did?: string | null, name?: string | null) => string;
    typeLabels?: {
        sent: string;
        received: string;
    };
    headers?: {
        date: string;
        type: string;
        counterparty: string;
        did: string;
        amount: string;
        currency: string;
        purpose: string;
        txId: string;
    };
}

/**
 * Escapes a single CSV value according to RFC 4180.
 * Wraps values containing delimiters, quotes, or newlines in quotes,
 * and doubles internal double quotes.
 */
export function escapeCsvField(value: unknown, delimiter = ';'): string {
    if (value === undefined || value === null) {
        return '';
    }
    const str = String(value);
    if (str.includes(delimiter) || str.includes('"') || str.includes('\n') || str.includes('\r')) {
        return `"${str.replace(/"/g, '""')}"`;
    }
    return str;
}

/**
 * Formats a Date/ISO string to YYYY-MM-DD HH:mm:ss for reliable spreadsheet parsing.
 */
export function formatIsoForCsv(isoString: string): string {
    try {
        const d = new Date(isoString);
        if (isNaN(d.getTime())) return isoString;
        const pad = (n: number) => String(n).padStart(2, '0');
        const year = d.getFullYear();
        const month = pad(d.getMonth() + 1);
        const day = pad(d.getDate());
        const hours = pad(d.getHours());
        const minutes = pad(d.getMinutes());
        const seconds = pad(d.getSeconds());
        return `${year}-${month}-${day} ${hours}:${minutes}:${seconds}`;
    } catch {
        return isoString;
    }
}

/**
 * Builds a bank-grade CSV export string with UTF-8 BOM (\uFEFF) for Excel/LibreOffice.
 *
 * Columns:
 * Datum;Typ;Gegenpartei;DID;Betrag;Waehrung;Verwendungszweck;Transaktionsnummer
 */
export function buildStatementCsv(
    records: TransactionRecord[],
    options?: StatementCsvOptions
): string {
    const delimiter = options?.delimiter || ';';
    const typeLabels = options?.typeLabels || {
        sent: 'Ausgabe',
        received: 'Einnahme'
    };
    const headers = options?.headers || {
        date: 'Datum',
        type: 'Typ',
        counterparty: 'Gegenpartei',
        did: 'DID',
        amount: 'Betrag',
        currency: 'Waehrung',
        purpose: 'Verwendungszweck',
        txId: 'Transaktionsnummer'
    };

    const headerLine = [
        headers.date,
        headers.type,
        headers.counterparty,
        headers.did,
        headers.amount,
        headers.currency,
        headers.purpose,
        headers.txId
    ].map(h => escapeCsvField(h, delimiter)).join(delimiter);

    const rows: string[] = [];

    for (const record of records) {
        const isSent = record.direction === 'sent';
        const typeLabel = isSent ? typeLabels.sent : typeLabels.received;
        const counterpartyDid = (isSent ? record.recipientId : record.senderId) || '';
        const rawName = isSent ? null : record.senderProfileName;
        
        let counterpartyName = '';
        if (options?.resolveContactName) {
            counterpartyName = options.resolveContactName(counterpartyDid, rawName);
        } else {
            counterpartyName = rawName || counterpartyDid;
        }

        const dateStr = formatIsoForCsv(record.timestamp);
        const purpose = record.notes || '';
        const txId = record.bundleId || record.id || '';

        const summableEntries = Object.entries(record.summableAmounts || {});
        const countableEntries = Object.entries(record.countableItems || {});

        if (summableEntries.length > 0) {
            for (const [unit, amountStr] of summableEntries) {
                const num = parseFloat(amountStr);
                const signedNum = isNaN(num) 
                    ? amountStr 
                    : (isSent ? -Math.abs(num) : Math.abs(num));
                
                const formattedAmount = typeof signedNum === 'number' 
                    ? signedNum.toFixed(2) 
                    : (isSent ? `-${signedNum}` : signedNum);

                const line = [
                    dateStr,
                    typeLabel,
                    counterpartyName,
                    counterpartyDid,
                    formattedAmount,
                    unit,
                    purpose,
                    txId
                ].map(v => escapeCsvField(v, delimiter)).join(delimiter);

                rows.push(line);
            }
        } else if (countableEntries.length > 0) {
            for (const [unit, count] of countableEntries) {
                const signedCount = isSent ? -Math.abs(count) : Math.abs(count);
                const line = [
                    dateStr,
                    typeLabel,
                    counterpartyName,
                    counterpartyDid,
                    String(signedCount),
                    unit,
                    purpose,
                    txId
                ].map(v => escapeCsvField(v, delimiter)).join(delimiter);

                rows.push(line);
            }
        } else {
            // Transaction with no explicit units/amounts
            const line = [
                dateStr,
                typeLabel,
                counterpartyName,
                counterpartyDid,
                '0.00',
                '',
                purpose,
                txId
            ].map(v => escapeCsvField(v, delimiter)).join(delimiter);

            rows.push(line);
        }
    }

    // Prepend UTF-8 BOM for automatic Excel delimiter and encoding detection
    return '\uFEFF' + [headerLine, ...rows].join('\r\n') + '\r\n';
}
