// src/components/TransactionHistoryView.tsx
import { useState, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { transferService } from '../services/transferService';
import { contactService } from '../services/contactService';
import { logger } from '../utils/log';
import { translateError, stringifyError } from '../utils/errorHelper';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { Button } from './ui/Button';
import { TransactionRecord, Contact, WalletEvent } from '../types';
import { PageLayout } from './ui/PageLayout';
import { AlertBanner } from './ui/AlertBanner';
import { Advanced } from './ui/Advanced';
import { AdvancedDisclosure } from './ui/AdvancedDisclosure';
import { Disclosure } from './ui/Disclosure';
import { TechId } from './ui/TechId';
import { formatDateTime, formatSummary, formatAmount } from '../utils/format';
import { buildStatementCsv } from '../utils/csvExport';
import ContactDialog from './ContactDialog';
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
    CheckCircle2, 
    FileSpreadsheet, 
    UserPlus, 
    Filter,
    RefreshCw,
    UploadCloud,
    PlusCircle,
    ShieldAlert,
    XCircle,
    Clock,
    History
} from 'lucide-react';
import { suggestFilename } from '../utils/userIdHelper';
import { useNavigation } from '../context/NavigationContext';
import { useOptionalSession } from '../context/SessionContext';
import { useContactResolver } from '../hooks/useContactResolver';
import { postService, POST_INBOX_UPDATED_EVENT } from '../services/postService';

export function TransactionHistoryView() {
    const { t } = useTranslation();
    const { navigate, goBack } = useNavigation();
    const session = useOptionalSession();
    const protectAction = session?.protectAction ?? (<T,>(action: (password: string | null) => Promise<T>): Promise<T | void> => action(null));
    const { resolveIdentity, isKnownContact, refresh: refreshContacts } = useContactResolver();

    const [history, setHistory] = useState<TransactionRecord[]>([]);
    const [events, setEvents] = useState<WalletEvent[]>([]);
    const [error, setError] = useState('');
    const [isLoading, setIsLoading] = useState(true);
    const [bundleFeedback, setBundleFeedback] = useState<{ [key: string]: string }>({});
    const [isSavingBundle, setIsSavingBundle] = useState<string | null>(null);
    const [expandedId, setExpandedId] = useState<string | null>(null);

    // Dual-Delivery post sync status
    const [unacknowledgedPayments, setUnacknowledgedPayments] = useState<string[]>([]);
    const [isRedepositingId, setIsRedepositingId] = useState<string | null>(null);
    const [redepositFeedback, setRedepositFeedback] = useState<{ [key: string]: string }>({});

    // Filters
    const [searchQuery, setSearchQuery] = useState('');
    const [directionFilter, setDirectionFilter] = useState<'all' | 'received' | 'sent'>('all');
    const [currencyFilter, setCurrencyFilter] = useState<string>('all');
    const [periodFilter, setPeriodFilter] = useState<'all' | 'last30Days' | 'thisYear'>('all');

    // CSV Feedback
    const [exportSuccessMsg, setExportSuccessMsg] = useState<string | null>(null);
    const [exportErrorMsg, setExportErrorMsg] = useState<string | null>(null);

    // Add Contact Modal
    const [contactModalData, setContactModalData] = useState<{ did: string; name?: string } | null>(null);

    useEffect(() => {
        let active = true;
        async function fetchHistory() {
            logger.info("TransactionHistoryView: Loading history.");
            try {
                const [records, eventList] = await Promise.all([
                    transferService.getHistory(),
                    typeof transferService.getEventHistory === 'function'
                        ? Promise.resolve(transferService.getEventHistory(0, 100)).catch(() => [])
                        : Promise.resolve([]),
                ]);
                records.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
                if (!active) return;
                setHistory(records);
                setEvents(Array.isArray(eventList) ? eventList : []);
            } catch (e) {
                if (!active) return;
                const msg = t('history.loadFailed', { error: translateError(e, t) });
                logger.error(`Failed to load transaction history: ${stringifyError(e)}`);
                setError(msg);
            } finally {
                if (active) setIsLoading(false);
            }
        }
        fetchHistory();

        const loadPostSummary = () => {
            postService.getPostInboxSummary()
                .then(summary => {
                    if (!active) return;
                    setUnacknowledgedPayments(summary.unacknowledgedPayments || []);
                })
                .catch(() => {});
        };
        loadPostSummary();
        window.addEventListener(POST_INBOX_UPDATED_EVENT, loadPostSummary);
        return () => {
            active = false;
            window.removeEventListener(POST_INBOX_UPDATED_EVENT, loadPostSummary);
        };
    }, [t]);

    // Discover all unique currencies present in history
    const availableCurrencies = useMemo(() => {
        const set = new Set<string>();
        for (const record of history) {
            if (record.summableAmounts) {
                for (const unit of Object.keys(record.summableAmounts)) {
                    set.add(unit);
                }
            }
            if (record.involvedSourcesDetails) {
                for (const d of record.involvedSourcesDetails) {
                    if (d.displayCurrency) set.add(d.displayCurrency);
                    else if (d.unit) set.add(d.unit);
                }
            }
        }
        return Array.from(set).sort();
    }, [history]);

    // Filtered transaction list
    const filteredHistory = useMemo(() => {
        const now = new Date();
        const thirtyDaysAgo = new Date(now.getTime() - 30 * 24 * 60 * 60 * 1000);
        const currentYear = now.getFullYear();
        const query = searchQuery.trim().toLowerCase();

        return history.filter(record => {
            // 1. Direction Filter
            if (directionFilter !== 'all' && record.direction !== directionFilter) {
                return false;
            }

            // 2. Period Filter
            if (periodFilter === 'last30Days') {
                const recordDate = new Date(record.timestamp);
                if (recordDate < thirtyDaysAgo) return false;
            } else if (periodFilter === 'thisYear') {
                const recordDate = new Date(record.timestamp);
                if (recordDate.getFullYear() !== currentYear) return false;
            }

            // 3. Currency Filter
            if (currencyFilter !== 'all') {
                const hasCurrencyInSummable = record.summableAmounts && Object.keys(record.summableAmounts).includes(currencyFilter);
                const hasCurrencyInDetails = record.involvedSourcesDetails?.some(
                    d => d.displayCurrency === currencyFilter || d.unit === currencyFilter
                );
                if (!hasCurrencyInSummable && !hasCurrencyInDetails) {
                    return false;
                }
            }

            // 4. Search Query Filter
            if (query) {
                const isSent = record.direction === 'sent';
                const counterpartyDid = isSent ? record.recipientId : record.senderId;
                const rawName = isSent ? null : record.senderProfileName;
                const resolvedName = resolveIdentity(counterpartyDid, rawName).toLowerCase();
                const notes = (record.notes || '').toLowerCase();
                const bundleId = (record.bundleId || '').toLowerCase();
                const recordId = record.id.toLowerCase();
                const recipientId = (record.recipientId || '').toLowerCase();
                const senderId = (record.senderId || '').toLowerCase();
                const senderProfile = (record.senderProfileName || '').toLowerCase();
                const summaryText = formatSummary(record.summableAmounts, record.countableItems).toLowerCase();

                const matchesQuery = 
                    resolvedName.includes(query) ||
                    notes.includes(query) ||
                    bundleId.includes(query) ||
                    recordId.includes(query) ||
                    recipientId.includes(query) ||
                    senderId.includes(query) ||
                    senderProfile.includes(query) ||
                    summaryText.includes(query);

                if (!matchesQuery) return false;
            }

            return true;
        });
    }, [history, directionFilter, periodFilter, currencyFilter, searchQuery, resolveIdentity]);

    // Financial breakdown / period totals
    const summaryData = useMemo(() => {
        const totalBookings = filteredHistory.length;
        const inboundSums: Record<string, number> = {};
        const outboundSums: Record<string, number> = {};

        for (const record of filteredHistory) {
            const isSent = record.direction === 'sent';
            const targetMap = isSent ? outboundSums : inboundSums;

            if (record.summableAmounts) {
                for (const [unit, amountStr] of Object.entries(record.summableAmounts)) {
                    const val = parseFloat(amountStr);
                    if (!isNaN(val)) {
                        targetMap[unit] = (targetMap[unit] || 0) + val;
                    }
                }
            }
        }

        return { totalBookings, inboundSums, outboundSums };
    }, [filteredHistory]);

    async function handleSaveBundle(record: TransactionRecord) {
        if (!record.bundleData || record.bundleData.length === 0) {
            setBundleFeedback(f => ({ ...f, [record.id]: t('history.noBundleData') }));
            return;
        }

        setIsSavingBundle(record.id);
        try {
            const recipientName = suggestFilename(record.recipientId ?? '');
            const txDate = new Date(record.timestamp);
            const dateTimePart = txDate.toISOString().substring(0, 16).replace(/-/g, '').replace('T', '_').replace(/:/g, '');
            const suggestedFilename = `${recipientName}_${dateTimePart}.transfer`;

            const filePath = await save({
                title: t('history.saveTitle'),
                defaultPath: suggestedFilename,
                filters: [{ name: t('history.saveFilterName'), extensions: ['transfer'] }]
            });

            if (filePath) {
                const content = new Uint8Array(record.bundleData);
                await writeFile(filePath, content);
                setBundleFeedback(f => ({ ...f, [record.id]: t('history.saved') }));
            }
        } catch (e) {
            setBundleFeedback(f => ({ ...f, [record.id]: t('history.saveError', { error: translateError(e, t) }) }));
        } finally {
            setIsSavingBundle(null);
        }
    }

    async function handleExportCsv() {
        try {
            const now = new Date();
            const datePart = now.toISOString().split('T')[0];
            const defaultFilename = `kontoauszug-${datePart}.csv`;

            const filePath = await save({
                title: t('history.exportCsv'),
                defaultPath: defaultFilename,
                filters: [{ name: 'CSV (*.csv)', extensions: ['csv'] }]
            });

            if (filePath) {
                const csvString = buildStatementCsv(filteredHistory, {
                    resolveContactName: (did, name) => resolveIdentity(did, name),
                    typeLabels: {
                        sent: t('history.filterOutbound'),
                        received: t('history.filterInbound')
                    },
                    headers: {
                        date: t('history.date'),
                        type: t('history.txStatus'),
                        counterparty: t('history.counterparty'),
                        did: 'ID',
                        amount: t('history.value'),
                        currency: t('history.currency'),
                        purpose: t('history.notes'),
                        txId: t('history.receiptNumber')
                    }
                });

                const encoder = new TextEncoder();
                await writeFile(filePath, encoder.encode(csvString));
                setExportSuccessMsg(t('history.exportCsvSuccess'));
                setTimeout(() => setExportSuccessMsg(null), 3500);
            }
        } catch (e) {
            logger.error(`CSV export error: ${stringifyError(e)}`);
            setExportErrorMsg(t('history.exportCsvError', { error: translateError(e, t) }));
            setTimeout(() => setExportErrorMsg(null), 4000);
        }
    }

    function getEventDetails(event: WalletEvent): { label: string; icon: React.ElementType; color: string; bgColor: string } {
        const type = event.eventType;
        const bff = event.bffData;

        if (typeof type === 'string') {
            switch (type) {
                case 'voucherCreated':
                    return { label: t('history.eventCreated'), icon: PlusCircle, color: 'text-blue-600', bgColor: 'bg-blue-50' };
                case 'transferSent': {
                    const recipientName = resolveIdentity(bff.counterpartyId, bff.counterpartyName);
                    return {
                        label: t('history.eventSent', { recipient: recipientName }),
                        icon: ArrowUpRight,
                        color: 'text-rose-600',
                        bgColor: 'bg-rose-50'
                    };
                }
                case 'transferReceived': {
                    const senderName = resolveIdentity(bff.counterpartyId, bff.counterpartyName);
                    return {
                        label: t('history.eventReceived', { sender: senderName }),
                        icon: ArrowDownLeft,
                        color: 'text-emerald-600',
                        bgColor: 'bg-emerald-50'
                    };
                }
                case 'voucherQuarantined':
                    return { label: t('history.eventQuarantine'), icon: ShieldAlert, color: 'text-amber-600', bgColor: 'bg-amber-50' };
                case 'voucherActivated':
                    return { label: t('history.eventActivated'), icon: CheckCircle2, color: 'text-emerald-600', bgColor: 'bg-emerald-50' };
                case 'voucherVoided':
                    return { label: t('history.eventVoided'), icon: XCircle, color: 'text-gray-600', bgColor: 'bg-gray-50' };
                case 'voucherExpired':
                    return { label: t('history.eventExpired'), icon: Clock, color: 'text-orange-600', bgColor: 'bg-orange-50' };
            }
        }

        return { label: t('history.eventDefault'), icon: Info, color: 'text-gray-600', bgColor: 'bg-gray-50' };
    }

    function getMatchingEventsForRecord(record: TransactionRecord): WalletEvent[] {
        const recordMs = new Date(record.timestamp).getTime();
        return events.filter((ev) => {
            if (record.involvedVouchers?.includes(ev.localInstanceId)) return true;
            if (record.involvedVouchers?.includes(ev.voucherId)) return true;
            if (record.involvedSourcesDetails?.some((d) => d.localInstanceId === ev.localInstanceId)) return true;
            const matchesDirection =
                (record.direction === 'sent' && ev.eventType === 'transferSent') ||
                (record.direction === 'received' && ev.eventType === 'transferReceived');
            if (matchesDirection && !isNaN(recordMs)) {
                const evMs = new Date(ev.timestamp).getTime();
                if (!isNaN(evMs) && Math.abs(evMs - recordMs) <= 60_000) return true;
            }
            return false;
        });
    }

    return (
        <PageLayout 
            title={t('history.transactionHistory')} 
            description={t('history.auditDescription')} 
            onBack={goBack}
            advancedActions={
                <Button
                    variant="secondary"
                    size="sm"
                    onClick={handleExportCsv}
                    className="gap-1.5 text-xs justify-start"
                    title={t('history.exportCsv')}
                    disabled={filteredHistory.length === 0}
                >
                    <FileSpreadsheet size={14} />
                    <span>{t('history.exportCsv')}</span>
                </Button>
            }
        >
            <div className="max-w-4xl mx-auto space-y-5">
                {/* Export Feedback Banners */}
                {exportSuccessMsg && (
                    <AlertBanner variant="success">{exportSuccessMsg}</AlertBanner>
                )}
                {exportErrorMsg && (
                    <AlertBanner variant="error">{exportErrorMsg}</AlertBanner>
                )}

                {/* Filter and Search Section */}
                <div className="bg-white/70 backdrop-blur-md border border-theme-subtle rounded-2xl p-4 shadow-sm space-y-3">
                    {/* Search Bar */}
                    <div className="relative group">
                        <div className="absolute inset-y-0 left-3.5 flex items-center pointer-events-none text-theme-light group-focus-within:text-theme-primary transition-colors">
                            <Search size={18} />
                        </div>
                        <input 
                            type="text" 
                            placeholder={t('history.searchPlaceholder')} 
                            value={searchQuery}
                            onChange={(e) => setSearchQuery(e.target.value)}
                            className="w-full pl-10 pr-4 py-2.5 bg-bg-app/50 border border-theme-subtle rounded-xl text-sm focus:outline-none focus:ring-2 focus:ring-theme-primary/20 focus:bg-white transition-all placeholder:text-theme-light/60 font-medium"
                        />
                    </div>

                    {/* Filter Controls Row */}
                    <div className="flex flex-wrap items-center justify-between gap-3 pt-1 border-t border-theme-subtle/50 text-xs">
                        {/* Direction Filter Pills */}
                        <div className="flex items-center gap-1 bg-bg-app p-1 rounded-xl border border-theme-subtle/60">
                            <button
                                type="button"
                                onClick={() => setDirectionFilter('all')}
                                className={`px-2.5 py-1 rounded-lg font-bold transition-all ${
                                    directionFilter === 'all' 
                                        ? 'bg-white text-theme-secondary shadow-xs' 
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                {t('history.filterAllDirections')}
                            </button>
                            <button
                                type="button"
                                onClick={() => setDirectionFilter('received')}
                                className={`px-2.5 py-1 rounded-lg font-bold transition-all ${
                                    directionFilter === 'received' 
                                        ? 'bg-emerald-500 text-white shadow-xs' 
                                        : 'text-emerald-600 hover:text-emerald-700'
                                }`}
                            >
                                {t('history.filterInbound')}
                            </button>
                            <button
                                type="button"
                                onClick={() => setDirectionFilter('sent')}
                                className={`px-2.5 py-1 rounded-lg font-bold transition-all ${
                                    directionFilter === 'sent' 
                                        ? 'bg-slate-700 text-white shadow-xs' 
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                {t('history.filterOutbound')}
                            </button>
                        </div>
                    </div>

                    {/* Currency & Period Filters (Expert open by default, Simple collapsed behind disclosure) */}
                    <AdvancedDisclosure
                        title={t('common.moreOptions')}
                        isCustom={currencyFilter !== 'all' || periodFilter !== 'all'}
                        toggleTestId="history-filters-toggle"
                        contentTestId="history-filters-content"
                        className="border-t border-theme-subtle/50 pt-2"
                        buttonClassName="w-full flex items-center justify-between gap-2 py-1 px-1.5 rounded-lg text-xs font-bold text-theme-secondary hover:bg-bg-app/60 transition-colors text-left cursor-pointer"
                        contentClassName="pt-2 flex flex-wrap items-center gap-2 text-xs"
                    >
                        {availableCurrencies.length > 1 && (
                            <div className="flex items-center gap-1.5 bg-bg-app px-2 py-1 rounded-xl border border-theme-subtle/60">
                                <Filter size={12} className="text-theme-light" />
                                <select
                                    value={currencyFilter}
                                    onChange={(e) => setCurrencyFilter(e.target.value)}
                                    className="bg-transparent font-bold text-theme-secondary focus:outline-none cursor-pointer pr-1"
                                >
                                    <option value="all">{t('history.filterAllCurrencies')}</option>
                                    {availableCurrencies.map(c => (
                                        <option key={c} value={c}>{c}</option>
                                    ))}
                                </select>
                            </div>
                        )}

                        <div className="flex items-center gap-1.5 bg-bg-app px-2 py-1 rounded-xl border border-theme-subtle/60">
                            <Calendar size={12} className="text-theme-light" />
                            <select
                                value={periodFilter}
                                onChange={(e) => setPeriodFilter(e.target.value as 'all' | 'last30Days' | 'thisYear')}
                                className="bg-transparent font-bold text-theme-secondary focus:outline-none cursor-pointer pr-1"
                            >
                                <option value="all">{t('history.periodAll')}</option>
                                <option value="last30Days">{t('history.periodLast30Days')}</option>
                                <option value="thisYear">{t('history.periodThisYear')}</option>
                            </select>
                        </div>
                    </AdvancedDisclosure>
                </div>

                {/* Subtle Statement Summary Bar */}
                <div className="flex flex-wrap items-center justify-between gap-3 px-4 py-2.5 bg-white/50 backdrop-blur-sm border border-theme-subtle/60 rounded-xl text-xs text-theme-secondary shadow-xs">
                    <div className="flex items-center gap-2 font-medium">
                        <span className="font-bold text-theme-primary">
                            {t('history.summaryBookings', { count: summaryData.totalBookings })}
                        </span>
                    </div>

                    <div className="flex flex-wrap items-center gap-3 font-mono">
                        {currencyFilter !== 'all' ? (
                            <>
                                <span className="text-emerald-600 font-bold">
                                    +{formatAmount(summaryData.inboundSums[currencyFilter] || 0)} {currencyFilter}
                                </span>
                                <span className="text-theme-light opacity-40">|</span>
                                <span className="text-slate-700 font-bold">
                                    -{formatAmount(summaryData.outboundSums[currencyFilter] || 0)} {currencyFilter}
                                </span>
                            </>
                        ) : (() => {
                            const summaryUnits = Array.from(
                                new Set([
                                    ...Object.keys(summaryData.inboundSums),
                                    ...Object.keys(summaryData.outboundSums),
                                ])
                            ).sort();
                            return summaryUnits.length > 0 ? (
                                <div className="flex flex-wrap items-center gap-3">
                                    {summaryUnits.map((unit) => {
                                        const inVal = summaryData.inboundSums[unit] || 0;
                                        const outVal = summaryData.outboundSums[unit] || 0;
                                        return (
                                            <span key={unit} className="text-[11px] font-semibold text-theme-light">
                                                <span className="text-emerald-600 font-bold">+{formatAmount(inVal)}</span>
                                                {' / '}
                                                <span className="text-slate-700 font-bold">-{formatAmount(outVal)}</span>
                                                {' '}{unit}
                                            </span>
                                        );
                                    })}
                                </div>
                            ) : null;
                        })()}
                    </div>
                </div>

                {isLoading && (
                    <div className="text-center py-20 text-theme-light animate-pulse font-black uppercase tracking-[0.2em]">
                        {t('history.loading')}
                    </div>
                )}
                
                {error && (
                    <div className="p-8 text-center bg-rose-50 border border-rose-100 rounded-3xl text-rose-500 font-bold">
                        {error}
                    </div>
                )}

                {!isLoading && !error && (
                    <div className="space-y-2.5">
                        {filteredHistory.length > 0 ? filteredHistory.map(record => {
                            const isSent = record.direction === 'sent';
                            const isExpanded = expandedId === record.id;
                            const counterpartyDid = (isSent ? record.recipientId : record.senderId) || '';
                            const counterpartyName = isSent 
                                ? resolveIdentity(record.recipientId, null) 
                                : resolveIdentity(record.senderId, record.senderProfileName);
                            
                            const shortReceiptNo = (record.bundleId || record.id).slice(0, 8);
                            const fullTxId = record.bundleId || record.id;
                            const canSaveAsContact = !!counterpartyDid && 
                                counterpartyDid.toLowerCase() !== 'anonymous' && 
                                counterpartyDid.startsWith('did:') && 
                                !isKnownContact(counterpartyDid);

                            return (
                                <div 
                                    key={record.id} 
                                    className={`bg-white border transition-all duration-200 rounded-2xl shadow-xs overflow-hidden ${
                                        isExpanded 
                                            ? 'border-theme-primary/40 ring-2 ring-theme-primary/10 shadow-md' 
                                            : 'border-theme-subtle hover:border-theme-primary/30 hover:shadow-sm'
                                    }`}
                                >
                                    {/* Main Row: Bank Statement Layout */}
                                    <div 
                                        className="p-4 cursor-pointer select-none flex items-center justify-between gap-4"
                                        onClick={() => setExpandedId(prev => prev === record.id ? null : record.id)}
                                    >
                                        {/* Left: Direction Icon & Details */}
                                        <div className="flex items-center gap-3.5 flex-1 min-w-0">
                                            <div 
                                                className={`flex items-center justify-center h-10 w-10 rounded-xl shrink-0 ${
                                                    isSent 
                                                        ? 'bg-slate-100 text-slate-700' 
                                                        : 'bg-emerald-50 text-emerald-600'
                                                }`}
                                            >
                                                {isSent ? <ArrowUpRight size={20} /> : <ArrowDownLeft size={20} />}
                                            </div>

                                            <div className="min-w-0 flex-1">
                                                {/* Top line: Date + Receipt # Badge */}
                                                <div className="flex items-center gap-2 mb-0.5">
                                                    <span className="text-[10px] text-theme-light font-medium flex items-center gap-1">
                                                        <Calendar size={11} className="opacity-70" />
                                                        {formatDateTime(record.timestamp)}
                                                    </span>
                                                    <Advanced>
                                                        <span className="font-mono text-[10px] bg-theme-subtle/30 text-theme-light px-1.5 py-0.2 rounded font-bold">
                                                            #{shortReceiptNo}
                                                        </span>
                                                    </Advanced>
                                                    {isSent && record.bundleId && unacknowledgedPayments.includes(record.bundleId) && (
                                                        <span data-testid="unacknowledged-payment-badge" className="font-semibold text-[9px] bg-amber-50 text-amber-800 border border-amber-200 px-1.5 py-0.2 rounded flex items-center gap-1">
                                                            <span>📦</span>
                                                            <span>{t('history.postWaitingForPickup', 'In Ablage')}</span>
                                                        </span>
                                                    )}
                                                </div>

                                                {/* Counterparty Name */}
                                                <div className="flex items-baseline gap-1 text-sm font-bold text-theme-secondary truncate">
                                                    <span className="text-theme-light font-normal text-xs shrink-0">
                                                        {isSent ? t('history.to') : t('history.from')}:
                                                    </span>
                                                    <span className="truncate text-theme-primary">
                                                        {counterpartyName}
                                                    </span>
                                                </div>

                                                {/* Purpose Note (truncated) */}
                                                {record.notes && (
                                                    <p className="text-xs text-theme-light/80 italic truncate max-w-lg mt-0.5">
                                                        "{record.notes}"
                                                    </p>
                                                )}
                                            </div>
                                        </div>

                                        {/* Right: Amount & Toggle Chevron */}
                                        <div className="text-right shrink-0 flex items-center gap-3">
                                            <div>
                                                <p className={`text-base sm:text-lg font-bold font-mono tracking-tight ${
                                                    isSent ? 'text-slate-700' : 'text-emerald-600'
                                                }`}>
                                                    {isSent ? '-' : '+'}{formatSummary(record.summableAmounts, record.countableItems)}
                                                </p>
                                                <span className={`text-[10px] font-bold uppercase tracking-wider block ${
                                                    isSent ? 'text-theme-light' : 'text-emerald-600/80'
                                                }`}>
                                                    {isSent ? t('history.outbound') : t('history.inbound')}
                                                </span>
                                            </div>

                                            <div 
                                                className="text-theme-light/40 hover:text-theme-primary transition-colors"
                                                title={t('common.details')}
                                                aria-label={t('common.details')}
                                            >
                                                {isExpanded ? <ChevronUp size={18} /> : <ChevronDown size={18} />}
                                            </div>
                                        </div>
                                    </div>

                                    {/* Expanded Details Pane */}
                                    {isExpanded && (
                                        <div className="px-5 pb-5 pt-3 border-t border-theme-subtle/60 bg-bg-app/30 animate-in slide-in-from-top-1 duration-150 space-y-4">
                                            {/* Purpose Box if present */}
                                            {record.notes && (
                                                <div className="p-3 bg-white border border-theme-subtle rounded-xl text-xs">
                                                    <p className="text-[10px] font-bold text-theme-light uppercase tracking-wider mb-1">
                                                        {t('history.notes')}
                                                    </p>
                                                    <p className="text-theme-secondary whitespace-pre-wrap break-words leading-relaxed">
                                                        {record.notes}
                                                    </p>
                                                </div>
                                            )}

                                            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                                                {/* Left Column: Identifiers & Copy Buttons */}
                                                <div className="space-y-3" onClick={(e) => e.stopPropagation()}>
                                                    {/* Transaction ID */}
                                                    <div className="p-3 bg-white border border-theme-subtle rounded-xl space-y-1.5">
                                                        <p className="text-[10px] font-bold text-theme-light uppercase tracking-wider flex items-center gap-1">
                                                            <Hash size={12} /> {t('history.txId')}
                                                        </p>
                                                        <TechId value={fullTxId} kind="bundle" />
                                                    </div>

                                                    {/* Counterparty DID */}
                                                    {counterpartyDid && (
                                                        <div className="p-3 bg-white border border-theme-subtle rounded-xl space-y-1.5">
                                                            <div className="flex items-center justify-between gap-2">
                                                                <p className="text-[10px] font-bold text-theme-light uppercase tracking-wider flex items-center gap-1">
                                                                    <User size={12} /> {isSent ? t('history.recipientDid') : t('history.senderDid')}
                                                                </p>
                                                                {canSaveAsContact && (
                                                                    <button
                                                                        type="button"
                                                                        onClick={(e) => {
                                                                            e.stopPropagation();
                                                                            setContactModalData({
                                                                                did: counterpartyDid,
                                                                                name: isSent ? undefined : record.senderProfileName
                                                                            });
                                                                        }}
                                                                        className="text-xs font-bold text-theme-primary hover:text-theme-accent transition-colors flex items-center gap-1"
                                                                    >
                                                                        <UserPlus size={12} />
                                                                        <span className="text-[10px]">{t('history.saveAsContact')}</span>
                                                                    </button>
                                                                )}
                                                            </div>
                                                            <TechId value={counterpartyDid} kind="did" />
                                                        </div>
                                                    )}
                                                </div>

                                                {/* Right Column: Honest L1 Verification & Bundle Export */}
                                                <div className="flex flex-col justify-between space-y-3">
                                                    <div className="p-3.5 bg-emerald-50/70 border border-emerald-200/80 rounded-xl">
                                                        <p className="text-[10px] font-bold text-emerald-800 uppercase tracking-wider mb-1">
                                                            {t('history.txStatus')}
                                                        </p>
                                                        <div className="flex items-center gap-2 text-emerald-900">
                                                            <CheckCircle2 size={16} className="text-emerald-600 shrink-0" />
                                                            <span className="text-xs font-bold leading-tight">
                                                                {t('history.verified')}
                                                            </span>
                                                        </div>
                                                    </div>

                                                    {isSent && record.bundleData && record.bundleData.length > 0 && (
                                                        <div className="pt-2 space-y-2">
                                                            <Button 
                                                                size="sm" 
                                                                variant="secondary" 
                                                                className="w-full gap-2 rounded-xl text-xs"
                                                                onClick={(e) => {
                                                                    e.stopPropagation();
                                                                    handleSaveBundle(record);
                                                                }} 
                                                                disabled={isSavingBundle === record.id}
                                                            >
                                                                <Download size={14} />
                                                                {isSavingBundle === record.id ? t('history.saving') : t('history.export')}
                                                            </Button>
                                                            {bundleFeedback[record.id] && (
                                                                <span className="block text-center text-[10px] font-bold text-emerald-600 mt-1 animate-pulse">
                                                                    {bundleFeedback[record.id]}
                                                                </span>
                                                            )}

                                                            {record.bundleId && (
                                                                <div>
                                                                    <Button
                                                                        size="sm"
                                                                        variant="outline"
                                                                        className="w-full gap-1.5 rounded-xl text-xs border-amber-200 text-amber-900 hover:bg-amber-50"
                                                                        onClick={async (e) => {
                                                                            e.stopPropagation();
                                                                            if (!record.bundleId) return;
                                                                            setIsRedepositingId(record.bundleId);
                                                                            try {
                                                                                const res = await protectAction(async (pwd) => {
                                                                                    return await postService.depositTransferToPost(record.bundleId!, pwd || undefined);
                                                                                });
                                                                                if (res && res.depositedTo > 0) {
                                                                                    setRedepositFeedback(prev => ({
                                                                                        ...prev,
                                                                                        [record.bundleId!]: t('history.redepositSuccess', 'In {{count}} Ablage(n) hinterlegt', { count: res.depositedTo })
                                                                                    }));
                                                                                } else if (res && res.errors.length > 0) {
                                                                                    setRedepositFeedback(prev => ({
                                                                                        ...prev,
                                                                                        [record.bundleId!]: res.errors.join('; ')
                                                                                    }));
                                                                                }
                                                                            } catch (err) {
                                                                                setRedepositFeedback(prev => ({ ...prev, [record.bundleId!]: stringifyError(err) }));
                                                                            } finally {
                                                                                setIsRedepositingId(null);
                                                                            }
                                                                        }}
                                                                        disabled={isRedepositingId === record.bundleId}
                                                                        data-testid="history-redeposit-button"
                                                                    >
                                                                        {isRedepositingId === record.bundleId ? <RefreshCw size={12} className="animate-spin" /> : <UploadCloud size={12} />}
                                                                        {isRedepositingId === record.bundleId ? t('transfer.depositing', 'Hinterlege...') : t('history.redepositButton', 'Erneut in Ablage hinterlegen')}
                                                                    </Button>
                                                                    {redepositFeedback[record.bundleId] && (
                                                                        <span className="block text-center text-[10px] font-bold text-amber-700 mt-1">
                                                                            {redepositFeedback[record.bundleId]}
                                                                        </span>
                                                                    )}
                                                                </div>
                                                            )}
                                                        </div>
                                                    )}
                                                </div>
                                            </div>

                                            {/* Voucher Breakdown Table */}
                                            {record.involvedSourcesDetails && record.involvedSourcesDetails.length > 0 && (
                                                <div className="pt-2 space-y-2">
                                                    <p className="text-[10px] font-bold text-theme-light uppercase tracking-wider flex items-center gap-1.5">
                                                        <Layers size={12} /> {t('history.involvedAssets', { count: record.involvedSourcesDetails.length })}
                                                    </p>
                                                    <div className="overflow-hidden border border-theme-subtle rounded-xl bg-white shadow-xs">
                                                        <table className="w-full text-xs text-left">
                                                            <thead className="bg-bg-app border-b border-theme-subtle">
                                                                <tr>
                                                                    <th className="px-3.5 py-2 font-bold text-theme-light uppercase text-[10px] tracking-wider">{t('history.assetStandard')}</th>
                                                                    <th className="px-3.5 py-2 font-bold text-theme-light uppercase text-[10px] tracking-wider text-right">{t('history.value')}</th>
                                                                    <Advanced>
                                                                        <th className="px-3.5 py-2 font-bold text-theme-light uppercase text-[10px] tracking-wider">{t('history.localId')}</th>
                                                                    </Advanced>
                                                                </tr>
                                                            </thead>
                                                            <tbody className="divide-y divide-theme-subtle/50">
                                                                {record.involvedSourcesDetails.map((detail, idx) => (
                                                                    <tr key={idx} className="hover:bg-bg-app/40 transition-colors">
                                                                        <td className="px-3.5 py-2 font-bold text-theme-secondary">{detail.standardName}</td>
                                                                        <td className="px-3.5 py-2 font-mono font-bold text-theme-primary text-right">{detail.amount} {detail.displayCurrency}</td>
                                                                        <Advanced>
                                                                            <td className="px-3.5 py-2 font-mono text-theme-light opacity-60 text-[11px]" title={detail.localInstanceId}>{detail.localInstanceId.slice(0, 10)}...</td>
                                                                        </Advanced>
                                                                    </tr>
                                                                ))}
                                                            </tbody>
                                                        </table>
                                                    </div>
                                                </div>
                                            )}

                                            {/* Related Cryptographic Events (Expert mode only) */}
                                            {(() => {
                                                const matchingEvents = getMatchingEventsForRecord(record);
                                                if (matchingEvents.length === 0) return null;
                                                return (
                                                    <Advanced>
                                                        <div className="pt-2" onClick={(e) => e.stopPropagation()}>
                                                            <Disclosure
                                                                toggleTestId="tx-event-log-toggle"
                                                                contentTestId="tx-event-log-content"
                                                                icon={<History size={13} className="text-theme-light" />}
                                                                title={
                                                                    <span className="text-[11px] font-bold text-theme-secondary">
                                                                        {t('history.txEventsToggle', { count: matchingEvents.length })}
                                                                    </span>
                                                                }
                                                                className="border border-theme-subtle rounded-xl bg-white overflow-hidden"
                                                                buttonClassName="w-full flex items-center justify-between gap-2 px-3.5 py-2.5 text-left hover:bg-bg-app/40 transition-colors cursor-pointer"
                                                                contentClassName="px-3.5 pb-3 pt-1 border-t border-theme-subtle/50 space-y-2"
                                                            >
                                                                {matchingEvents.map((ev) => {
                                                                    const { label, icon: Icon, color, bgColor } = getEventDetails(ev);
                                                                    return (
                                                                        <div
                                                                            key={ev.eventId}
                                                                            className="flex items-center justify-between gap-3 py-1.5 text-xs"
                                                                        >
                                                                            <div className="flex items-center gap-2.5 min-w-0">
                                                                                <span className={`inline-flex items-center justify-center h-6 w-6 rounded-lg shrink-0 ${bgColor} ${color}`}>
                                                                                    <Icon size={13} />
                                                                                </span>
                                                                                <div className="min-w-0">
                                                                                    <p className="font-semibold text-theme-secondary truncate">{label}</p>
                                                                                    <div className="flex items-center gap-2 text-[10px] font-mono text-theme-light">
                                                                                        <span>#{ev.localInstanceId.slice(0, 8)}</span>
                                                                                        {ev.bffData.displayCurrency && (
                                                                                            <span className="font-bold text-theme-accent">
                                                                                                {ev.bffData.amount} {ev.bffData.displayCurrency}
                                                                                            </span>
                                                                                        )}
                                                                                    </div>
                                                                                </div>
                                                                            </div>
                                                                            <span className="text-[10px] text-theme-light shrink-0">
                                                                                {formatDateTime(ev.timestamp)}
                                                                            </span>
                                                                        </div>
                                                                    );
                                                                })}
                                                            </Disclosure>
                                                        </div>
                                                    </Advanced>
                                                );
                                            })()}
                                        </div>
                                    )}
                                </div>
                            );
                        }) : (
                            <div className="text-center py-14 px-6 bg-white/40 backdrop-blur-sm border border-dashed border-theme-subtle rounded-3xl space-y-3">
                                <div className="inline-flex items-center justify-center h-14 w-14 rounded-full bg-theme-subtle/20 text-theme-light">
                                    <Info size={28} />
                                </div>
                                <p className="text-theme-secondary font-bold text-sm">{t('history.noMatchingTx')}</p>
                                <p className="text-xs text-theme-light">{t('history.adjustFilter')}</p>
                                <div className="flex flex-wrap items-center justify-center gap-2 pt-2">
                                    {history.length > 0 ? (
                                        <Button
                                            variant="outline"
                                            size="sm"
                                            onClick={() => {
                                                setSearchQuery('');
                                                setDirectionFilter('all');
                                                setCurrencyFilter('all');
                                                setPeriodFilter('all');
                                            }}
                                        >
                                            {t('voucher.filter.reset')}
                                        </Button>
                                    ) : (
                                        <>
                                            <Button variant="primary" size="sm" onClick={() => navigate({ view: 'create_voucher', previousView: { view: 'transaction_history' } })}>
                                                {t('voucher.create')}
                                            </Button>
                                            <Button variant="outline" size="sm" onClick={() => navigate({ view: 'receive_bundle' })}>
                                                {t('transfer.receive')}
                                            </Button>
                                        </>
                                    )}
                                </div>
                            </div>
                        )}
                    </div>
                )}

                {!isLoading && !error && events.length > 0 && (
                    <Advanced>
                        <Disclosure
                            toggleTestId="history-event-log-toggle"
                            contentTestId="history-event-log-content"
                            icon={<History size={16} className="text-theme-light" />}
                            title={
                                <span className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                    {t('history.eventLogSectionTitle', { count: events.length })}
                                </span>
                            }
                            className="bg-white/80 border border-theme-subtle rounded-2xl overflow-hidden shadow-xs"
                            buttonClassName="w-full flex items-center justify-between gap-3 px-4 py-3 text-left hover:bg-bg-app/50 transition-colors cursor-pointer"
                            contentClassName="px-4 pb-4 pt-2 border-t border-theme-subtle/60 space-y-2"
                        >
                            {events.map((ev) => {
                                const { label, icon: Icon, color, bgColor } = getEventDetails(ev);
                                const isTransferEvent = ev.eventType === 'transferSent' || ev.eventType === 'transferReceived';
                                return (
                                    <button
                                        key={ev.eventId}
                                        type="button"
                                        onClick={() => {
                                            if (!isTransferEvent) {
                                                navigate({
                                                    view: 'voucher_details',
                                                    voucherId: ev.localInstanceId,
                                                    previousView: { view: 'transaction_history' },
                                                });
                                            }
                                        }}
                                        className={`w-full text-left p-3 rounded-xl border border-theme-subtle/70 bg-white flex items-center justify-between gap-3 transition-colors ${
                                            isTransferEvent
                                                ? 'cursor-default'
                                                : 'hover:border-theme-primary/40 hover:bg-bg-app/30 cursor-pointer'
                                        }`}
                                    >
                                        <div className="flex items-center gap-3 min-w-0">
                                            <div className={`flex items-center justify-center h-9 w-9 rounded-xl shrink-0 ${bgColor} ${color}`}>
                                                <Icon size={17} />
                                            </div>
                                            <div className="min-w-0">
                                                <p className="text-xs font-bold text-theme-secondary truncate">{label}</p>
                                                <div className="flex items-center gap-2 mt-0.5 text-[10px] font-mono text-theme-light">
                                                    <span>#{ev.localInstanceId.slice(0, 8)}</span>
                                                    {ev.bffData.displayCurrency && (
                                                        <span className="font-bold text-theme-accent">
                                                            {ev.bffData.amount} {ev.bffData.displayCurrency}
                                                        </span>
                                                    )}
                                                </div>
                                            </div>
                                        </div>
                                        <span className="text-[11px] font-medium text-theme-light shrink-0">
                                            {formatDateTime(ev.timestamp)}
                                        </span>
                                    </button>
                                );
                            })}
                        </Disclosure>
                    </Advanced>
                )}
            </div>

            {/* Quick Add Contact Modal */}
            {contactModalData && (
                <ContactDialog
                    isOpen={true}
                    initialDid={contactModalData.did}
                    initialProfile={contactModalData.name ? { firstName: contactModalData.name } : null}
                    onClose={() => setContactModalData(null)}
                    onSave={async (contact: Contact) => {
                        await contactService.saveContact(contact);
                        await refreshContacts();
                        setContactModalData(null);
                    }}
                />
            )}
        </PageLayout>
    );
}

