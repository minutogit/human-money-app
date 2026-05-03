// src/components/Activities.tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { WalletEvent } from '../types';
import { PageLayout } from './ui/PageLayout';
import { formatDateTime } from '../utils/format';
import { 
    PlusCircle, 
    ArrowUpRight, 
    ArrowDownLeft, 
    ShieldAlert, 
    CheckCircle2, 
    XCircle, 
    Clock, 
    Info,
    ChevronRight,
    Calendar
} from 'lucide-react';

interface ActivitiesProps {
    onBack: () => void;
    onNavigateToVoucherDetail: (voucherId: string) => void;
    onNavigateToHistory: () => void;
}


function getEventDetails(event: WalletEvent): { label: string; icon: any; color: string; bgColor: string } {
    const type = event.eventType;
    const bff = event.bffData;

    if (typeof type === 'string') {
        switch (type) {
            case 'voucherCreated':
                return { label: 'Voucher Created', icon: PlusCircle, color: 'text-blue-600', bgColor: 'bg-blue-50' };
            case 'transferSent':
                return { 
                    label: `Sent to ${bff.counterpartyName || 'Anonymous'}`, 
                    icon: ArrowUpRight, 
                    color: 'text-rose-600', 
                    bgColor: 'bg-rose-50' 
                };
            case 'transferReceived':
                return { 
                    label: `Received from ${bff.counterpartyName || 'Anonymous'}`, 
                    icon: ArrowDownLeft, 
                    color: 'text-emerald-600', 
                    bgColor: 'bg-emerald-50' 
                };
            case 'voucherQuarantined':
                return { label: 'Security Quarantine', icon: ShieldAlert, color: 'text-amber-600', bgColor: 'bg-amber-50' };
            case 'voucherActivated':
                return { label: 'Voucher Activated', icon: CheckCircle2, color: 'text-emerald-600', bgColor: 'bg-emerald-50' };
            case 'voucherVoided':
                return { label: 'Voucher Voided', icon: XCircle, color: 'text-gray-600', bgColor: 'bg-gray-50' };
            case 'voucherExpired':
                return { label: 'Voucher Expired', icon: Clock, color: 'text-orange-600', bgColor: 'bg-orange-50' };
        }
    }

    return { label: 'Wallet Event', icon: Info, color: 'text-gray-600', bgColor: 'bg-gray-50' };
}

export function Activities({ onBack, onNavigateToVoucherDetail, onNavigateToHistory }: ActivitiesProps) {
    const [events, setEvents] = useState<WalletEvent[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState('');

    useEffect(() => {
        async function fetchEvents() {
            try {
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
        <PageLayout 
            title="Activity Log" 
            description="A chronological record of your wallet events." 
            onBack={onBack}
        >
            <div className="max-w-4xl mx-auto space-y-6">
                {isLoading && (
                    <div className="py-20 text-center animate-pulse text-theme-light font-black uppercase tracking-[0.2em]">
                        Loading Activities...
                    </div>
                )}
                
                {error && (
                    <div className="p-8 text-center bg-rose-50 border border-rose-100 rounded-3xl text-rose-500 font-bold">
                        {error}
                    </div>
                )}

                {!isLoading && !error && (
                    <div className="space-y-3">
                        {events.length > 0 ? events.map(event => {
                            const { label, icon: Icon, color, bgColor } = getEventDetails(event);
                            return (
                                <button 
                                    key={event.eventId} 
                                    onClick={() => {
                                        const type = event.eventType;
                                        if (type === 'transferSent' || type === 'transferReceived') {
                                            onNavigateToHistory();
                                        } else {
                                            onNavigateToVoucherDetail(event.localInstanceId);
                                        }
                                    }}
                                    className="w-full text-left group"
                                >
                                    <div className="p-4 bg-white border border-theme-subtle rounded-2xl flex items-center justify-between group-hover:border-theme-primary/30 group-hover:shadow-premium transition-all">
                                        <div className="flex items-center gap-4">
                                            <div className={`flex items-center justify-center h-12 w-12 rounded-xl shrink-0 transition-transform group-hover:scale-105 ${bgColor} ${color}`}>
                                                <Icon size={24} />
                                            </div>
                                            <div className="min-w-0">
                                                <p className="font-bold text-theme-primary truncate tracking-tight">{label}</p>
                                                <div className="flex items-center gap-2 mt-0.5">
                                                    <span className="text-[10px] font-mono font-bold text-theme-light/60">
                                                        #{event.localInstanceId.substring(0, 8)}
                                                    </span>
                                                    {event.bffData.displayCurrency && (
                                                        <span className="text-[10px] font-black text-theme-accent uppercase tracking-widest">
                                                            {event.bffData.amount} {event.bffData.displayCurrency}
                                                        </span>
                                                    )}
                                                </div>
                                            </div>
                                        </div>
                                        <div className="text-right shrink-0 flex items-center gap-4">
                                            <div className="hidden sm:block">
                                                <p className="text-[10px] font-black text-theme-light uppercase tracking-widest flex items-center justify-end gap-1 mb-1">
                                                    <Calendar size={10} />
                                                    Date
                                                </p>
                                                <p className="text-xs font-bold text-theme-secondary">
                                                    {formatDateTime(event.timestamp)}
                                                </p>
                                            </div>
                                            <ChevronRight size={18} className="text-theme-light/30 group-hover:text-theme-primary group-hover:translate-x-1 transition-all" />
                                        </div>
                                    </div>
                                </button>
                            );
                        }) : (
                            <div className="py-24 flex flex-col items-center justify-center text-center bg-white/30 backdrop-blur-sm rounded-[40px] border-2 border-dashed border-theme-subtle/50">
                                <div className="p-6 bg-theme-subtle/10 rounded-full text-theme-light mb-6">
                                    <Clock size={48} />
                                </div>
                                <h3 className="text-2xl font-black text-theme-primary tracking-tight">No Events Recorded</h3>
                                <p className="text-theme-light mt-2 max-w-sm font-medium">Your activity trail will appear here as you create and spend vouchers.</p>
                            </div>
                        )}
                    </div>
                )}
            </div>
        </PageLayout>
    );
}
