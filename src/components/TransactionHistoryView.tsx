// src/components/TransactionHistoryView.tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
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

export function TransactionHistoryView({ onBack }: TransactionHistoryViewProps) {
    const [history, setHistory] = useState<TransactionRecord[]>([]);
    const [error, setError] = useState('');
    const [isLoading, setIsLoading] = useState(true);

    useEffect(() => {
        async function fetchHistory() {
            logger.info("TransactionHistoryView: Attempting to load history.");
            const password = window.prompt("Please enter your wallet password to view your transaction history:");
            if (!password) {
                logger.warn("No password provided. Aborting history load.");
                onBack();
                return;
            }

            try {
                const records = await invoke<TransactionRecord[]>("load_transaction_history", { password });
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
    }, [onBack]);

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
                                                {record.direction}
                                            </p>
                                            <p className="text-sm text-theme-light font-mono">
                                                {record.direction === 'sent' ? `To: ${record.recipient_id.substring(0, 25)}...` : `From: ${record.sender_id.substring(0, 25)}...`}
                                            </p>
                                        </div>
                                    </div>
                                    <div className="text-right">
                                        <p className="font-bold text-lg text-theme-primary">
                                            {Object.entries(record.total_amount_by_unit).map(([unit, amount]) => `${amount} ${unit}`).join(', ')}
                                        </p>
                                        <p className="text-sm text-theme-light">
                                            {formatTimestamp(record.timestamp)}
                                        </p>
                                    </div>
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