// src/components/Activities.tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { WalletEvent } from '../types';

interface ActivitiesProps {
    onBack: () => void;
    onNavigateToVoucherDetail: (voucherId: string) => void;
    onNavigateToHistory: () => void;
}

function formatTimestamp(isoString: string): string {
    return new Date(isoString).toLocaleString(undefined, {
        dateStyle: 'medium',
        timeStyle: 'short',
    });
}

function getEventDetails(event: WalletEvent): { label: string; icon: string; color: string } {
    const type = event.event_type;
    const bff = event.bff_data;

    if (typeof type === 'string') {
        switch (type) {
            case 'VoucherCreated':
                return { label: 'Voucher Created', icon: '✨', color: 'bg-blue-100 text-blue-700' };
            case 'TransferSent':
                return { 
                    label: `Sent to ${bff.counterparty_name || 'Anonymous'}`, 
                    icon: '↑', 
                    color: 'bg-red-100 text-red-700' 
                };
            case 'TransferReceived':
                return { 
                    label: `Received from ${bff.counterparty_name || 'Anonymous'}`, 
                    icon: '↓', 
                    color: 'bg-green-100 text-green-700' 
                };
            case 'VoucherQuarantined':
                return { label: 'Voucher Quarantined', icon: '⚠️', color: 'bg-amber-100 text-amber-700' };
            case 'VoucherActivated':
                return { label: 'Voucher Activated', icon: '✅', color: 'bg-emerald-100 text-emerald-700' };
            case 'VoucherVoided':
                return { label: 'Voucher Voided', icon: '🚫', color: 'bg-gray-100 text-gray-700' };
            case 'VoucherExpired':
                return { label: 'Voucher Expired', icon: '⏰', color: 'bg-orange-100 text-orange-700' };
        }
    } else if (type && typeof type === 'object' && 'Unknown' in type) {
        return { label: type.Unknown, icon: '❓', color: 'bg-gray-100 text-gray-700' };
    }

    return { label: 'Unknown Event', icon: '?', color: 'bg-gray-100 text-gray-700' };
}

export function Activities({ onBack, onNavigateToVoucherDetail, onNavigateToHistory }: ActivitiesProps) {
    const [events, setEvents] = useState<WalletEvent[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState('');

    useEffect(() => {
        async function fetchEvents() {
            try {
                // Fetch first 50 events
                const data = await invoke<WalletEvent[]>("get_event_history", { offset: 0, limit: 50 });
                setEvents(data);
            } catch (e) {
                logger.error(`Failed to fetch activities: ${e}`);
                setError('Failed to load activity history.');
            } finally {
                setIsLoading(false);
            }
        }
        fetchEvents();
    }, []);

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
                    <h1 className="text-2xl font-bold text-theme-primary">Activity Log</h1>
                </div>
                <p className="text-theme-light ml-14">A chronological record of all wallet events.</p>
            </header>

            <div className="flex-grow overflow-y-auto">
                {isLoading && <p className="text-center text-theme-light py-8">Loading activities...</p>}
                {error && <p className="text-center text-red-500 py-8">{error}</p>}

                {!isLoading && !error && (
                    <div className="space-y-3">
                        {events.length > 0 ? events.map(event => {
                            const { label, icon, color } = getEventDetails(event);
                            return (
                                <button 
                                    key={event.event_id} 
                                    onClick={() => {
                                        const type = event.event_type;
                                        if (type === 'TransferSent' || type === 'TransferReceived') {
                                            onNavigateToHistory();
                                        } else {
                                            onNavigateToVoucherDetail(event.local_instance_id);
                                        }
                                    }}
                                    className="w-full text-left bg-bg-card border border-theme-subtle rounded-lg p-4 flex items-center justify-between hover:border-theme-primary hover:shadow-sm transition-all group"
                                >
                                    <div className="flex items-center gap-4">
                                        <div className={`flex items-center justify-center h-10 w-10 rounded-full font-bold text-lg ${color}`}>
                                            {icon}
                                        </div>
                                        <div>
                                            <p className="font-semibold text-theme-primary">{label}</p>
                                            <p className="text-xs text-theme-light mt-0.5">
                                                ID: {event.local_instance_id.substring(0, 8)}...
                                                {event.bff_data.display_currency && ` · ${event.bff_data.amount} ${event.bff_data.display_currency}`}
                                            </p>
                                        </div>
                                    </div>
                                    <div className="text-right">
                                        <p className="text-sm text-theme-secondary">
                                            {formatTimestamp(event.timestamp)}
                                        </p>
                                    </div>
                                </button>
                            );
                        }) : (
                            <div className="text-center text-theme-light py-12 bg-bg-card border border-dashed border-theme-subtle rounded-lg">
                                <p>No activities recorded yet.</p>
                            </div>
                        )}
                    </div>
                )}
            </div>
        </div>
    );
}
