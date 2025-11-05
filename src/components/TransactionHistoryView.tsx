// src/components/TransactionHistoryView.tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { Button } from './ui/Button';
import { TransactionRecord } from '../types';

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
    const [expandedId, setExpandedId] = useState<string | null>(null); // NEU: Für aufklappbare Details

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
    }, []); // onBack aus den Abhängigkeiten entfernt, da es nicht mehr für den Abbruch benötigt wird.

    async function handleSaveBundle(record: TransactionRecord) {
        if (!record.bundle_data || record.bundle_data.length === 0) {
            setFeedback(f => ({ ...f, [record.id]: 'Error: No bundle data available for this record.' }));
            return;
        }

        setIsSaving(record.id);
        setFeedback(f => ({ ...f, [record.id]: '' }));

        try {
            const suggestedFilename = `transfer-for-${record.recipient_id.substring(8, 20)}-${new Date(record.timestamp).toISOString().split('T')[0]}.minuto-bundle`;

            const filePath = await save({
                title: 'Save Transfer Bundle Again',
                defaultPath: suggestedFilename,
                filters: [{ name: 'Minuto Bundle', extensions: ['minuto-bundle'] }]
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
                <div className="flex items-center justify-between">
                    <h1 className="text-2xl font-bold text-theme-primary">Transaction History</h1>
                    <Button variant="secondary" onClick={onBack}>Back to Dashboard</Button>
                </div>
                <p className="text-theme-light mt-1">An overview of your past transactions.</p>
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
                                                {/* Leichte Bereinigung: "sent" -> "Sent" */}
                                                {record.direction === 'sent' ? 'Sent' : 'Received'}
                                            </p>
                                            {/* --- ANGEPASSTE LOGIK FÜR ID/NAME --- */}
                                            {record.direction === 'sent' ? (
                                                <p className="text-sm text-theme-light font-mono" title={record.recipient_id}>
                                                    To: {truncateId(record.recipient_id)}
                                                </p>
                                            ) : (
                                                <>
                                                    {record.sender_profile_name ? (
                                                        <p className="text-sm text-theme-light">
                                                            From: {record.sender_profile_name}
                                                        </p>
                                                    ) : null}
                                                    <p className={`font-mono ${record.sender_profile_name ? 'text-xs text-theme-light' : 'text-sm text-theme-light'}`} title={record.sender_id}>
                                                        {record.sender_profile_name ? `(${truncateId(record.sender_id)})` : `From: ${truncateId(record.sender_id)}`}
                                                    </p>
                                                </>
                                            )}
                                            {/* --- ENDE ANPASSUNG --- */}
                                        </div>
                                    </div>
                                    <div className="text-right">
                                        <p className="font-bold text-lg text-theme-primary">
                                            {/* NEU: formatSummary verwenden */}
                                            {formatSummary(record.summableAmounts, record.countableItems)}
                                        </p>
                                        <p className="text-sm text-theme-light">
                                            {formatTimestamp(record.timestamp)}
                                        </p>
                                    </div>
                                </div>

                                {/* NEU: Aufklappbare Details */}
                                <div className="border-t border-theme-subtle mt-3 pt-3">
                                    <div className="flex justify-between items-center">
                                        <button
                                            onClick={() => toggleDetails(record.id)}
                                            className="text-sm text-theme-accent font-medium"
                                        >
                                            {expandedId === record.id ? 'Hide Details' : 'Show Details'}
                                        </button>

                                        {/* Button zum erneuten Speichern (nur für "sent") */}
                                        {record.direction === 'sent' && record.bundle_data && record.bundle_data.length > 0 && (
                                            <div className="flex justify-end items-center gap-4">
                                                {feedback[record.id] && <p className="text-xs text-theme-light">{feedback[record.id]}</p>}
                                                <Button size="sm" variant="outline" onClick={() => handleSaveBundle(record)} disabled={isSaving === record.id}>
                                                    {isSaving === record.id ? 'Saving...' : 'Save Bundle'}
                                                </Button>
                                            </div>
                                        )}
                                    </div>

                                    {/* Aufklappbarer Bereich */}
                                    {expandedId === record.id && (
                                        <div className="mt-3 space-y-1 text-sm">
                                            {record.notes && (
                                                <p className="text-theme-light"><span className="font-medium text-theme-secondary">Notes:</span> {record.notes}</p>
                                            )}
                                            <p className="text-theme-light font-mono text-xs" title={record.bundle_id}>
                                                <span className="font-medium text-theme-secondary">Bundle ID:</span> {record.bundle_id}
                                            </p>

                                            {/* NEU: Involvierte Gutscheine anzeigen */}
                                            {record.involved_vouchers && record.involved_vouchers.length > 0 && (
                                                <div className="text-theme-light pt-1">
                                                    <span className="font-medium text-theme-secondary">
                                                        {record.direction === 'sent' ? 'Sent from Vouchers' : 'Received into Vouchers'} ({record.involved_vouchers.length}):
                                                    </span>
                                                    {/* Wir kürzen die lokalen IDs mit derselben Funktion wie die User-IDs */}
                                                    <ul className="list-disc list-inside pl-2 font-mono text-xs">
                                                        {record.involved_vouchers.map(id => (
                                                            <li key={id} title={id}>{truncateId(id)}</li>
                                                        ))}
                                                    </ul>
                                                </div>
                                            )}

                                        </div>
                                    )}
                                </div>
                                {/* KORREKTUR: Der redundante "Save Bundle"-Block wurde von hier entfernt */}
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