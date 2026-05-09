// src/components/TransactionHistoryView.tsx
import { useState, useEffect } from 'react';
import { transferService } from '../services/transferService';
import { logger } from '../utils/log';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { Button } from './ui/Button';
import { TransactionRecord } from '../types';
import { PageLayout } from './ui/PageLayout';
import { formatDateTime, formatSummary } from '../utils/format';
import { 
    ArrowUpRight, 
    ArrowDownLeft, 
    Calendar, 
    User, 
    Download, 
    ChevronDown, 
    ChevronUp,
    Hash,
    Layers,
    Info,
    Search,
    CheckCircle2
} from 'lucide-react';
import { extractDisplayName, truncateUserId, suggestFilename } from '../utils/userIdHelper';

import { useNavigation } from '../context/NavigationContext';

export function TransactionHistoryView() {
    const { goBack } = useNavigation();
    const [history, setHistory] = useState<TransactionRecord[]>([]);
    const [error, setError] = useState('');
    const [isLoading, setIsLoading] = useState(true);
    const [feedback, setFeedback] = useState<{ [key: string]: string }>({});
    const [isSaving, setIsSaving] = useState<string | null>(null);
    const [expandedId, setExpandedId] = useState<string | null>(null);
    const [searchQuery, setSearchQuery] = useState('');

    useEffect(() => {
        async function fetchHistory() {
            logger.info("TransactionHistoryView: Loading history.");
            try {
                const records = await transferService.getHistory();
                records.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
                setHistory(records);
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
        if (!record.bundleData || record.bundleData.length === 0) {
            setFeedback(f => ({ ...f, [record.id]: 'Error: No bundle data available.' }));
            return;
        }

        setIsSaving(record.id);
        try {
            const recipientName = suggestFilename(record.recipientId ?? '');
            const txDate = new Date(record.timestamp);
            const dateTimePart = txDate.toISOString().substring(0, 16).replace(/-/g, '').replace('T', '_').replace(/:/g, '');
            const suggestedFilename = `${recipientName}_${dateTimePart}.transfer`;

            const filePath = await save({
                title: 'Save Transfer File',
                defaultPath: suggestedFilename,
                filters: [{ name: 'Transfer File', extensions: ['transfer'] }]
            });

            if (filePath) {
                const content = new Uint8Array(record.bundleData);
                await writeFile(filePath, content);
                setFeedback(f => ({ ...f, [record.id]: `Saved!` }));
            }
        } catch (e) {
            setFeedback(f => ({ ...f, [record.id]: `Error: ${e}` }));
        } finally {
            setIsSaving(null);
        }
    }

    const filteredHistory = history.filter(record => {
        const query = searchQuery.toLowerCase();
        return (
            record.notes?.toLowerCase().includes(query) ||
            record.recipientId?.toLowerCase().includes(query) ||
            record.senderId?.toLowerCase().includes(query) ||
            record.senderProfileName?.toLowerCase().includes(query)
        );
    });

    return (
        <PageLayout 
            title="Activity Audit" 
            description="Complete record of all secure transfers." 
            onBack={goBack}
        >
            <div className="max-w-4xl mx-auto space-y-6">
                {/* Search & Filter Bar */}
                <div className="relative group">
                    <div className="absolute inset-y-0 left-4 flex items-center pointer-events-none text-theme-light group-focus-within:text-theme-primary transition-colors">
                        <Search size={18} />
                    </div>
                    <input 
                        type="text" 
                        placeholder="Filter by recipient, sender or notes..." 
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        className="w-full pl-12 pr-4 py-4 bg-white/50 backdrop-blur-sm border border-theme-subtle rounded-2xl focus:outline-none focus:ring-2 focus:ring-theme-primary/10 transition-all shadow-inner-soft placeholder:text-theme-light/60 font-medium"
                    />
                </div>

                {isLoading && (
                    <div className="text-center py-20 text-theme-light animate-pulse font-black uppercase tracking-[0.2em]">
                        Loading history...
                    </div>
                )}
                
                {error && (
                    <div className="p-8 text-center bg-rose-50 border border-rose-100 rounded-3xl text-rose-500 font-bold">
                        {error}
                    </div>
                )}

                {!isLoading && !error && (
                    <div className="space-y-3">
                        {filteredHistory.length > 0 ? filteredHistory.map(record => {
                            const isSent = record.direction === 'sent';
                            const isExpanded = expandedId === record.id;
                            
                            return (
                                <div key={record.id} className="group">
                                    <div 
                                        className={`p-4 bg-white border ${isExpanded ? 'border-theme-primary/30 ring-1 ring-theme-primary/5' : 'border-theme-subtle hover:border-theme-primary/20'} rounded-2xl transition-all cursor-pointer shadow-sm group-hover:shadow-md`}
                                        onClick={() => setExpandedId(isExpanded ? null : record.id)}
                                    >
                                        <div className="flex items-center justify-between gap-4">
                                            <div className="flex items-center gap-4 flex-1">
                                                <div className={`flex items-center justify-center h-12 w-12 rounded-2xl shrink-0 transition-transform group-hover:scale-105 ${isSent ? 'bg-rose-50 text-rose-500' : 'bg-emerald-50 text-emerald-500'}`}>
                                                    {isSent ? <ArrowUpRight size={24} /> : <ArrowDownLeft size={24} />}
                                                </div>
                                                <div className="min-w-0">
                                                    <div className="flex items-center gap-2 mb-0.5">
                                                        <span className={`text-[10px] font-black uppercase tracking-widest ${isSent ? 'text-rose-500' : 'text-emerald-500'}`}>
                                                            {isSent ? 'Outbound' : 'Inbound'}
                                                        </span>
                                                        <span className="text-[10px] text-theme-light/60 font-medium flex items-center gap-1">
                                                            <Calendar size={10} />
                                                            {formatDateTime(record.timestamp)}
                                                        </span>
                                                    </div>
                                                    <div className="flex flex-col">
                                                        <p className="text-sm font-bold text-theme-secondary truncate">
                                                            {isSent ? (
                                                                <>To: <span className="text-theme-primary">{extractDisplayName(record.recipientId || '')}</span></>
                                                            ) : (
                                                                <>From: <span className="text-theme-primary">{record.senderProfileName || extractDisplayName(record.senderId || '')}</span></>
                                                            )}
                                                        </p>
                                                        {record.notes && (
                                                            <p className="text-xs text-theme-light italic truncate max-w-md">
                                                                "{record.notes}"
                                                            </p>
                                                        )}
                                                    </div>
                                                </div>
                                            </div>
                                            <div className="text-right shrink-0">
                                                <p className={`text-lg font-black tracking-tight ${isSent ? 'text-theme-primary' : 'text-emerald-600'}`}>
                                                    {isSent ? '-' : '+'}{formatSummary(record.summableAmounts, record.countableItems, record.involvedSourcesDetails)}
                                                </p>
                                                <div className="flex items-center justify-end gap-1 mt-1 text-theme-light/50">
                                                    {isExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                                                </div>
                                            </div>
                                        </div>

                                        {/* Expanded Content */}
                                        {isExpanded && (
                                            <div className="mt-6 pt-6 border-t border-theme-subtle animate-in slide-in-from-top-2 duration-200">
                                                <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-6">
                                                    <div className="space-y-4">
                                                        <div>
                                                            <p className="text-[9px] font-black text-theme-light uppercase tracking-[0.2em] mb-2 flex items-center gap-1.5">
                                                                <Hash size={10} /> Transaction ID
                                                            </p>
                                                            <p className="text-xs font-mono text-theme-secondary break-all bg-theme-subtle/10 p-3 rounded-xl border border-theme-subtle/20">
                                                                {record.bundleId}
                                                            </p>
                                                        </div>
                                                        <div>
                                                            <p className="text-[9px] font-black text-theme-light uppercase tracking-[0.2em] mb-2 flex items-center gap-1.5">
                                                                <User size={10} /> Full {isSent ? 'Recipient' : 'Sender'} DID
                                                            </p>
                                                            <p className="text-xs font-mono text-theme-secondary break-all bg-theme-subtle/10 p-3 rounded-xl border border-theme-subtle/20">
                                                                {isSent ? record.recipientId : record.senderId}
                                                            </p>
                                                        </div>
                                                    </div>
                                                    
                                                    <div className="flex flex-col justify-between">
                                                        <div className="p-4 bg-theme-primary/5 rounded-2xl border border-theme-primary/10">
                                                            <p className="text-[9px] font-black text-theme-primary uppercase tracking-[0.2em] mb-2">Transaction Status</p>
                                                            <div className="flex items-center gap-2 text-theme-secondary">
                                                                <CheckCircle2 size={16} className="text-emerald-500" />
                                                                <span className="text-sm font-bold">Securely Verified</span>
                                                            </div>
                                                        </div>
                                                        
                                                        {isSent && record.bundleData && record.bundleData.length > 0 && (
                                                            <div className="mt-4 flex items-center gap-3">
                                                                <Button 
                                                                    size="sm" 
                                                                    variant="secondary" 
                                                                    className="w-full gap-2 rounded-xl"
                                                                    onClick={(e) => {
                                                                        e.stopPropagation();
                                                                        handleSaveBundle(record);
                                                                    }} 
                                                                    disabled={isSaving === record.id}
                                                                >
                                                                    <Download size={14} />
                                                                    {isSaving === record.id ? 'Saving...' : 'Export Transfer File'}
                                                                </Button>
                                                                {feedback[record.id] && (
                                                                    <span className="text-[10px] font-bold text-emerald-500 animate-pulse">{feedback[record.id]}</span>
                                                                )}
                                                            </div>
                                                        )}
                                                    </div>
                                                </div>

                                                {/* Voucher Breakdown */}
                                                {record.involvedSourcesDetails && record.involvedSourcesDetails.length > 0 && (
                                                    <div className="space-y-3">
                                                        <p className="text-[9px] font-black text-theme-light uppercase tracking-[0.2em] flex items-center gap-1.5">
                                                            <Layers size={10} /> Involved Assets ({record.involvedSourcesDetails.length})
                                                        </p>
                                                        <div className="overflow-hidden border border-theme-subtle rounded-2xl">
                                                            <table className="w-full text-[11px] text-left">
                                                                <thead className="bg-theme-subtle/10 border-b border-theme-subtle">
                                                                    <tr>
                                                                        <th className="px-4 py-2 font-black text-theme-light uppercase tracking-widest">Asset Standard</th>
                                                                        <th className="px-4 py-2 font-black text-theme-light uppercase tracking-widest text-right">Value</th>
                                                                        <th className="px-4 py-2 font-black text-theme-light uppercase tracking-widest">Local ID</th>
                                                                    </tr>
                                                                </thead>
                                                                <tbody className="divide-y divide-theme-subtle/50">
                                                                    {record.involvedSourcesDetails.map((detail, idx) => (
                                                                        <tr key={idx} className="hover:bg-theme-subtle/5 transition-colors">
                                                                            <td className="px-4 py-2.5 font-bold text-theme-secondary">{detail.standardName}</td>
                                                                            <td className="px-4 py-2.5 font-black text-theme-primary text-right">{detail.amount} {detail.displayCurrency}</td>
                                                                            <td className="px-4 py-2.5 font-mono text-theme-light opacity-60">{truncateUserId(detail.localInstanceId)}</td>
                                                                        </tr>
                                                                    ))}
                                                                </tbody>
                                                            </table>
                                                        </div>
                                                    </div>
                                                )}
                                            </div>
                                        )}
                                    </div>
                                </div>
                            );
                        }) : (
                            <div className="text-center py-20 bg-white/30 backdrop-blur-sm border border-dashed border-theme-subtle rounded-3xl">
                                <div className="inline-flex items-center justify-center h-16 w-16 rounded-full bg-theme-subtle/20 text-theme-light mb-4">
                                    <Info size={32} />
                                </div>
                                <p className="text-theme-light font-bold">No matching transactions found.</p>
                                <p className="text-xs text-theme-light/60 mt-1">Try adjusting your filter or search query.</p>
                            </div>
                        )}
                    </div>
                )}
            </div>
        </PageLayout>
    );
}

