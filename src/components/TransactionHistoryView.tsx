// src/components/TransactionHistoryView.tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { Button } from './ui/Button';
import { TransactionRecord } from '../types'; // Stellt sicher, dass TransactionRecord 'involved_sources_details?' enthält

interface TransactionHistoryViewProps {
    onBack: () => void;
}

function formatTimestamp(isoString: string): string {
    return new Date(isoString).toLocaleString(undefined, {
        dateStyle: 'medium',
        timeStyle: 'short',
    });
}

// NEU: Helper-Funktion zum Kürzen von DIDs im Format [erste 10]...[letzte 5]
function truncateId(id: string): string {
    const prefix = "did:key:";
    let key = id;

    if (id.startsWith(prefix)) {
        key = id.substring(prefix.length);
    }

    // z.B. z6MknxUwKV...a6xEP
    if (key.length > 15) {
        return `${key.substring(0, 10)}...${key.substring(key.length - 5)}`;
    }

    // Fallback für kurze oder unerwartete IDs
    return key;
}

// NEU: Helper-Funktion zum Formatieren der vollständigen Zusammenfassung
function formatSummary(
    summable: Record<string, string> | undefined,
    countable: Record<string, number> | undefined
): string {
    const s = Object.entries(summable || {}).map(([unit, amount]) => `${amount} ${unit}`);
    const c = Object.entries(countable || {}).map(([unit, total]) => `${total} ${unit}${total > 1 ? 's' : ''}`); // Simple plural
    const all = [...s, ...c];
    return all.length > 0 ? all.join(', ') : '0.00';
}

export function TransactionHistoryView({ onBack }: TransactionHistoryViewProps) {
    const [history, setHistory] = useState<TransactionRecord[]>([]);
    const [error, setError] = useState('');
    const [isLoading, setIsLoading] = useState(true);
    const [feedback, setFeedback] = useState<{ [key: string]: string }>({});
    const [isSaving, setIsSaving] = useState<string | null>(null);
    const [expandedId, setExpandedId] = useState<string | null>(null);

    useEffect(() => {
        async function fetchHistory() {
            logger.info("TransactionHistoryView: Attempting to load history.");


            try {
                // Ruft die Historie aus dem Cache ab, kein Passwort erforderlich.
                const records = await invoke<TransactionRecord[]>("get_transaction_history");
                // Sort records by timestamp, newest first
                records.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
                setHistory(records);
                logger.info(`Successfully loaded ${records.length} transaction records.`);
            } catch (e) {
                const msg = `Failed to load transaction history: ${e}`;
                logger.error(msg);
                setError(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchHistory();
    }, []);

    async function handleSaveBundle(record: TransactionRecord) {
        if (!record.bundle_data || record.bundle_data.length === 0) {
            setFeedback(f => ({ ...f, [record.id]: 'Error: No bundle data available for this record.' }));
            return;
        }

        setIsSaving(record.id);
        setFeedback(f => ({ ...f, [record.id]: '' }));

        try {
            // NEUE LOGIK: Format [RECIPIENT_NAME]_[DATUM-ZEIT].transfer
            // 1. Extrahiere den Teil vor dem "@" (z.B. hans-tRB)
            const recipientNameMatch = record.recipient_id?.match(/(.+)@/);
            const recipientName = recipientNameMatch ? recipientNameMatch[1] : 'transfer';

            // 2. Erzeuge den Zeitstempel aus dem Record: YYYYMMDD_HHmm (ohne Doppelpunkte)
            const txDate = new Date(record.timestamp);
            const dateTimePart = txDate.toISOString().substring(0, 16).replace(/-/g, '').replace('T', '_').replace(/:/g, ''); // z.B. 20251106_1801
            
            const suggestedFilename = `${recipientName}_${dateTimePart}.transfer`;

            const filePath = await save({
                title: 'Save Transfer Bundle Again',
                defaultPath: suggestedFilename,
                filters: [{ name: 'Transfer Bundle', extensions: ['transfer'] }]
            });

            if (filePath) {
                const content = new Uint8Array(record.bundle_data);
                await writeFile(filePath, content);
                logger.info(`Successfully re-saved transfer bundle to ${filePath}`);
                setFeedback(f => ({ ...f, [record.id]: `Saved successfully!` }));
            } else {
                logger.info('File re-save dialog was cancelled.');
            }

        } catch (e) {
            const msg = `Failed to re-save file: ${e}`;
            logger.error(msg);
            setFeedback(f => ({ ...f, [record.id]: `Error: ${msg}` }));
        } finally {
            setIsSaving(null);
        }
    }

    const toggleDetails = (id: string) => {
        if (expandedId === id) {
            setExpandedId(null);
        } else {
            setExpandedId(id);
        }
    };

    return (
        <div className="flex flex-col h-full max-w-4xl mx-auto">
            <header className="flex-shrink-0 mb-6">
                <div className="flex items-center gap-4 mb-2">
                    <button
                        onClick={onBack}
                        className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                        title="Back to Dashboard"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                        </svg>
                    </button>
                    <h1 className="text-2xl font-bold text-theme-primary">Transaction History</h1>
                </div>
                <p className="text-theme-light ml-14">An overview of your past transactions.</p>
            </header>

            <div className="flex-grow overflow-y-auto">
                {isLoading && <p className="text-center text-theme-light">Loading history...</p>}
                {error && <p className="text-center text-red-500">{error}</p>}

                {!isLoading && !error && (
                    <div className="space-y-4">
                        {history.length > 0 ? history.map(record => (
                            <div key={record.id} className="bg-bg-card border border-theme-subtle rounded-lg p-4">
                                <div className="flex items-center justify-between">
                                    <div className="flex items-center gap-4">
                                        <div className={`flex items-center justify-center h-10 w-10 rounded-full ${record.direction === 'sent' ? 'bg-red-200 text-red-800' : 'bg-green-200 text-green-800'}`}>
                                            {record.direction === 'sent' ? '↑' : '↓'}
                                        </div>
                                        <div>
                                            <p className="font-semibold capitalize text-theme-primary">
                                                {record.direction === 'sent' ? 'Sent' : 'Received'}
                                            </p>
                                            
                                            {/* --- NEUES LAYOUT FÜR SENDER/EMPFÄNGER & NOTIZEN --- */}
                                            {record.direction === 'sent' ? (
                                                (() => {
                                                    const recipientNameMatch = record.recipient_id?.match(/(.+)@/);
                                                    const recipientName = recipientNameMatch ? recipientNameMatch[1] : null;
                                                    const recipientIdPart = recipientNameMatch ? record.recipient_id!.split('@')[1] : (record.recipient_id || 'Unknown');

                                                    return (
                                                        <div>
                                                            <p className="text-sm text-theme-light" title={record.recipient_id}>
                                                                To: {recipientName ? (
                                                                    <>
                                                                        <span className="font-semibold text-theme-primary">{recipientName}</span>
                                                                        <span className="font-mono text-xs ml-1.5">({truncateId(recipientIdPart)})</span>
                                                                    </>
                                                                ) : (
                                                                    <span className="font-mono">{truncateId(record.recipient_id || '')}</span>
                                                                )}
                                                            </p>
                                                            {/* NEUES LAYOUT FÜR NOTIZEN */}
                                                            {record.notes && (
                                                                <p className="text-xs text-theme-light max-w-xs truncate" title={record.notes}>
                                                                    <span className="font-medium text-theme-secondary">Notes:</span>
                                                                    <span className="text-sm text-theme-primary italic ml-1.5">{record.notes}</span>
                                                                </p>
                                                            )}
                                                        </div>
                                                    );
                                                })()
                                            ) : (
                                                <div>
                                                    <p className="text-sm text-theme-light" title={record.sender_id}>
                                                        From: {record.sender_profile_name ? (
                                                            <>
                                                                <span className="font-semibold text-theme-primary">{record.sender_profile_name}</span>
                                                                <span className="font-mono text-xs ml-1.5">({truncateId(record.sender_id || 'Unknown')})</span>
                                                            </>
                                                        ) : (
                                                            <span className="font-mono">{truncateId(record.sender_id || 'Unknown')}</span>
                                                        )}
                                                    </p>
                                                    {/* NEUES LAYOUT FÜR NOTIZEN */}
                                                    {record.notes && (
                                                        <p className="text-xs text-theme-light max-w-xs truncate" title={record.notes}>
                                                            <span className="font-medium text-theme-secondary">Notes:</span>
                                                            <span className="text-sm text-theme-primary italic ml-1.5">{record.notes}</span>
                                                        </p>
                                                    )}
                                                </div>
                                            )}
                                            {/* --- ENDE NEUES LAYOUT --- */}
                                        </div>
                                    </div>
                                    <div className="text-right">
                                        <p className="font-bold text-lg text-theme-primary">
                                            {formatSummary(record.summableAmounts, record.countableItems)}
                                        </p>
                                        <p className="text-sm text-theme-light">
                                            {formatTimestamp(record.timestamp)}
                                        </p>
                                    </div>
                                </div>

                                <div className="border-t border-theme-subtle mt-3 pt-3">
                                    <div className="flex justify-between items-center">
                                        <button
                                            onClick={() => toggleDetails(record.id)}
                                            className="text-sm text-theme-accent font-medium"
                                        >
                                            {expandedId === record.id ? 'Hide Details' : 'Show Details'}
                                        </button>

                                        {record.direction === 'sent' && record.bundle_data && record.bundle_data.length > 0 && (
                                            <div className="flex justify-end items-center gap-4">
                                                {feedback[record.id] && <p className="text-xs text-theme-light">{feedback[record.id]}</p>}
                                                <Button size="sm" variant="outline" onClick={() => handleSaveBundle(record)} disabled={isSaving === record.id}>
                                                    {isSaving === record.id ? 'Saving...' : 'Save Bundle'}
                                                </Button>
                                            </div>
                                        )}
                                    </div>

                                    {/* --- NEUER DETAILBEREICH START --- */}
                                    {expandedId === record.id && (
                                        <div className="mt-4 space-y-3 text-sm">
                                            <p className="text-theme-light font-mono text-xs" title={record.bundle_id}>
                                                <span className="font-medium text-theme-secondary">Bundle ID:</span> {record.bundle_id}
                                            </p>

                                            {/* Fall 1: Wir haben reiche Details (NUR für "Sent" Transaktionen) */}
                                            {record.involved_sources_details && record.involved_sources_details.length > 0 && (
                                                <div className="text-theme-light pt-2">
                                                    <span className="font-medium text-theme-secondary">
                                                        {record.direction === 'sent' ? 'Sent from Vouchers' : 'Received into Vouchers'} ({record.involved_sources_details.length}):
                                                    </span>
                                                    <div className="mt-2 space-y-2 font-mono text-xs">
                                                        {/* Header für die "Tabelle" */}
                                                        <div className="grid grid-cols-5 gap-2 font-bold text-theme-secondary">
                                                            <span>Standard</span>
                                                            <span className="text-right">Amount</span>
                                                            <span>Unit</span>
                                                            <span>Voucher-ID</span>
                                                            <span>Local-ID</span>
                                                        </div>
                                                        {/* Datenzeilen */}
                                                        {record.involved_sources_details!.map((detail, index) => (
                                                            <div key={index} className="grid grid-cols-5 gap-2 p-1.5 bg-black/10 rounded items-center">
                                                                <span className="truncate" title={detail.standard_name}>{detail.standard_name}</span>
                                                                <span className="text-right font-semibold" title={detail.amount}>{detail.amount}</span>
                                                                <span title={detail.unit}>{detail.unit}</span>
                                                                <span className="truncate" title={detail.voucher_id || ''}>{truncateId(detail.voucher_id || '')}</span>
                                                                <span className="truncate" title={detail.local_instance_id}>{truncateId(detail.local_instance_id)}</span>
                                                            </div>
                                                        ))}
                                                    </div>
                                                </div>
                                            )}

                                            {/* Fall 2: Wir haben NUR IDs (NUR für "Received" Transaktionen) */}
                                            {/* Zeige dies nur an, wenn Fall 1 nicht zutrifft */}
                                            {record.involved_vouchers && record.involved_vouchers.length > 0 && !(record.involved_sources_details && record.involved_sources_details.length > 0) && (
                                                <div className="text-theme-light pt-2">
                                                    <span className="font-medium text-theme-secondary">
                                                        Received into Vouchers ({record.involved_vouchers.length}):
                                                    </span>
                                                    <ul className="list-disc list-inside pl-2 font-mono text-xs mt-1">
                                                        {record.involved_vouchers.map(id => (
                                                            <li key={id} title={id}>{truncateId(id)}</li>
                                                        ))}
                                                    </ul>
                                                </div>
                                            )}

                                        </div>
                                    )}
                                    {/* --- NEUER DETAILBEREICH ENDE --- */}
                                </div>
                            </div>
                        )) : (
                            <div className="text-center text-theme-light py-8 bg-input-readonly rounded-lg border border-theme-subtle">
                                <p>No transaction history found.</p>
                            </div>
                        )}
                    </div>
                )}
            </div>
        </div>
    );
}