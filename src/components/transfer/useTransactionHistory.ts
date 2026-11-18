// src/components/transfer/useTransactionHistory.ts
import { useState, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { transferService } from '../../services/transferService';
import { postService, POST_INBOX_UPDATED_EVENT } from '../../services/postService';
import { useNavigation } from '../../context/NavigationContext';
import { useOptionalSession } from '../../context/SessionContext';
import { useContactResolver } from '../../hooks/useContactResolver';
import { saveBytesToFile } from '../../utils/fileIo';
import { buildStatementCsv } from '../../utils/csvExport';
import { formatSummary } from '../../utils/format';
import { suggestFilename } from '../../utils/userIdHelper';
import { summarizeTransactionHistory } from '../../utils/walletEvents';
import { logger } from '../../utils/log';
import { translateError, stringifyError } from '../../utils/errorHelper';
import type { DirectionFilter, PeriodFilter } from './HistoryFilterBar';
import type { TransactionRecord, WalletEvent } from '../../types';

export function useTransactionHistory() {
    const { t } = useTranslation();
    const { navigate, goBack } = useNavigation();
    const session = useOptionalSession();
    const protectAction =
        session?.protectAction ??
        (<T,>(action: (password: string | null) => Promise<T>): Promise<T | void> => action(null));
    const { resolveIdentity, isKnownContact, refresh: refreshContacts } = useContactResolver();

    const [history, setHistory] = useState<TransactionRecord[]>([]);
    const [events, setEvents] = useState<WalletEvent[]>([]);
    const [error, setError] = useState('');
    const [isLoading, setIsLoading] = useState(true);
    const [bundleFeedback, setBundleFeedback] = useState<Record<string, string>>({});
    const [isSavingBundle, setIsSavingBundle] = useState<string | null>(null);
    const [expandedId, setExpandedId] = useState<string | null>(null);
    const [unacknowledgedPayments, setUnacknowledgedPayments] = useState<string[]>([]);
    const [isRedepositingId, setIsRedepositingId] = useState<string | null>(null);
    const [redepositFeedback, setRedepositFeedback] = useState<Record<string, string>>({});

    const [searchQuery, setSearchQuery] = useState('');
    const [directionFilter, setDirectionFilter] = useState<DirectionFilter>('all');
    const [currencyFilter, setCurrencyFilter] = useState<string>('all');
    const [periodFilter, setPeriodFilter] = useState<PeriodFilter>('all');
    const [exportSuccessMsg, setExportSuccessMsg] = useState<string | null>(null);
    const [exportErrorMsg, setExportErrorMsg] = useState<string | null>(null);
    const [contactModalData, setContactModalData] = useState<{ did: string; name?: string } | null>(
        null,
    );

    useEffect(() => {
        let active = true;
        async function fetchHistory() {
            logger.info('TransactionHistoryView: Loading history.');
            try {
                const [records, eventList] = await Promise.all([
                    transferService.getHistory(),
                    typeof transferService.getEventHistory === 'function'
                        ? Promise.resolve(transferService.getEventHistory(0, 100)).catch(() => [])
                        : Promise.resolve([]),
                ]);
                records.sort(
                    (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime(),
                );
                if (!active) return;
                setHistory(records);
                setEvents(Array.isArray(eventList) ? eventList : []);
            } catch (e) {
                if (!active) return;
                logger.error(`Failed to load transaction history: ${stringifyError(e)}`);
                setError(t('history.loadFailed', { error: translateError(e, t) }));
            } finally {
                if (active) setIsLoading(false);
            }
        }
        fetchHistory();

        const loadPostSummary = () => {
            postService
                .getPostInboxSummary()
                .then((summary) => {
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

    const availableCurrencies = useMemo(() => {
        const set = new Set<string>();
        for (const record of history) {
            if (record.summableAmounts) {
                for (const unit of Object.keys(record.summableAmounts)) set.add(unit);
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

    const filteredHistory = useMemo(() => {
        const now = new Date();
        const thirtyDaysAgo = new Date(now.getTime() - 30 * 24 * 60 * 60 * 1000);
        const currentYear = now.getFullYear();
        const query = searchQuery.trim().toLowerCase();

        return history.filter((record) => {
            if (directionFilter !== 'all' && record.direction !== directionFilter) return false;

            if (periodFilter === 'last30Days') {
                if (new Date(record.timestamp) < thirtyDaysAgo) return false;
            } else if (periodFilter === 'thisYear') {
                if (new Date(record.timestamp).getFullYear() !== currentYear) return false;
            }

            if (currencyFilter !== 'all') {
                const hasInSummable =
                    record.summableAmounts &&
                    Object.keys(record.summableAmounts).includes(currencyFilter);
                const hasInDetails = record.involvedSourcesDetails?.some(
                    (d) => d.displayCurrency === currencyFilter || d.unit === currencyFilter,
                );
                if (!hasInSummable && !hasInDetails) return false;
            }

            if (query) {
                const isSent = record.direction === 'sent';
                const resolvedName = resolveIdentity(
                    isSent ? record.recipientId : record.senderId,
                    isSent ? null : record.senderProfileName,
                ).toLowerCase();
                const summaryText = formatSummary(
                    record.summableAmounts,
                    record.countableItems,
                ).toLowerCase();

                const matches = [
                    resolvedName,
                    record.notes || '',
                    record.bundleId || '',
                    record.id,
                    record.recipientId || '',
                    record.senderId || '',
                    record.senderProfileName || '',
                    summaryText,
                ].some((field) => field.toLowerCase().includes(query));

                if (!matches) return false;
            }

            return true;
        });
    }, [history, directionFilter, periodFilter, currencyFilter, searchQuery, resolveIdentity]);

    const summaryData = useMemo(
        () => summarizeTransactionHistory(filteredHistory),
        [filteredHistory],
    );

    const handleSaveBundle = async (record: TransactionRecord) => {
        if (!record.bundleData || record.bundleData.length === 0) {
            setBundleFeedback((f) => ({ ...f, [record.id]: t('history.noBundleData') }));
            return;
        }
        setIsSavingBundle(record.id);
        try {
            const recipientName = suggestFilename(record.recipientId ?? '');
            const dateTimePart = new Date(record.timestamp)
                .toISOString()
                .substring(0, 16)
                .replace(/-/g, '')
                .replace('T', '_')
                .replace(/:/g, '');
            const saved = await saveBytesToFile(record.bundleData, {
                defaultFilename: `${recipientName}_${dateTimePart}.transfer`,
                filters: [{ name: t('history.saveFilterName'), extensions: ['transfer'] }],
            });
            if (saved) {
                setBundleFeedback((f) => ({ ...f, [record.id]: t('history.saved') }));
            }
        } catch (e) {
            setBundleFeedback((f) => ({
                ...f,
                [record.id]: t('history.saveError', { error: translateError(e, t) }),
            }));
        } finally {
            setIsSavingBundle(null);
        }
    };

    const handleRedeposit = async (bundleId: string) => {
        setIsRedepositingId(bundleId);
        try {
            const res = await protectAction(async (pwd) => {
                return await postService.depositTransferToPost(bundleId, pwd || undefined);
            });
            if (res && res.depositedTo > 0) {
                setRedepositFeedback((prev) => ({
                    ...prev,
                    [bundleId]: t('history.redepositSuccess', 'In {{count}} Ablage(n) hinterlegt', {
                        count: res.depositedTo,
                    }),
                }));
            } else if (res && res.errors.length > 0) {
                setRedepositFeedback((prev) => ({ ...prev, [bundleId]: res.errors.join('; ') }));
            }
        } catch (err) {
            setRedepositFeedback((prev) => ({ ...prev, [bundleId]: stringifyError(err) }));
        } finally {
            setIsRedepositingId(null);
        }
    };

    const handleExportCsv = async () => {
        try {
            const datePart = new Date().toISOString().split('T')[0];
            const csvString = buildStatementCsv(filteredHistory, {
                resolveContactName: (did, name) => resolveIdentity(did, name),
                typeLabels: {
                    sent: t('history.filterOutbound'),
                    received: t('history.filterInbound'),
                },
                headers: {
                    date: t('history.date'),
                    type: t('history.txStatus'),
                    counterparty: t('history.counterparty'),
                    did: 'ID',
                    amount: t('history.value'),
                    currency: t('history.currency'),
                    purpose: t('history.notes'),
                    txId: t('history.receiptNumber'),
                },
            });

            const saved = await saveBytesToFile(new TextEncoder().encode(csvString), {
                defaultFilename: `kontoauszug-${datePart}.csv`,
                filters: [{ name: 'CSV (*.csv)', extensions: ['csv'] }],
                mimeType: 'text/csv;charset=utf-8',
            });
            if (saved) {
                setExportSuccessMsg(t('history.exportCsvSuccess'));
                setTimeout(() => setExportSuccessMsg(null), 3500);
            }
        } catch (e) {
            logger.error(`CSV export error: ${stringifyError(e)}`);
            setExportErrorMsg(t('history.exportCsvError', { error: translateError(e, t) }));
            setTimeout(() => setExportErrorMsg(null), 4000);
        }
    };

    return {
        navigate,
        goBack,
        resolveIdentity,
        isKnownContact,
        refreshContacts,
        history,
        events,
        error,
        isLoading,
        bundleFeedback,
        isSavingBundle,
        expandedId,
        setExpandedId,
        unacknowledgedPayments,
        isRedepositingId,
        redepositFeedback,
        searchQuery,
        setSearchQuery,
        directionFilter,
        setDirectionFilter,
        currencyFilter,
        setCurrencyFilter,
        periodFilter,
        setPeriodFilter,
        exportSuccessMsg,
        exportErrorMsg,
        contactModalData,
        setContactModalData,
        availableCurrencies,
        filteredHistory,
        summaryData,
        handleSaveBundle,
        handleRedeposit,
        handleExportCsv,
        resetFilters: () => {
            setSearchQuery('');
            setDirectionFilter('all');
            setCurrencyFilter('all');
            setPeriodFilter('all');
        },
    };
}
