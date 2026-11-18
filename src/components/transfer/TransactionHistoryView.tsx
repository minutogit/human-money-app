// src/components/transfer/TransactionHistoryView.tsx
import { useTranslation } from 'react-i18next';
import { Info, FileSpreadsheet, History } from 'lucide-react';
import { contactService } from '../../services/contactService';
import { formatDateTime } from '../../utils/format';
import { getWalletEventPresentation } from '../../utils/walletEvents';
import { Button } from '../ui/Button';
import { PageLayout } from '../ui/PageLayout';
import { AlertBanner } from '../ui/AlertBanner';
import { Advanced } from '../ui/Advanced';
import { Disclosure } from '../ui/Disclosure';
import ContactDialog from '../contacts/ContactDialog';
import { HistoryFilterBar } from './HistoryFilterBar';
import { TransactionHistoryRow } from './TransactionHistoryRow';
import { HISTORY_EVENT_ICONS } from './TransactionDetailsModal';
import { useTransactionHistory } from './useTransactionHistory';
import type { Contact } from '../../types';

export function TransactionHistoryView() {
    const { t } = useTranslation();
    const state = useTransactionHistory();

    return (
        <PageLayout
            title={t('history.transactionHistory')}
            description={t('history.auditDescription')}
            onBack={state.goBack}
            advancedActions={
                <Button
                    variant="secondary"
                    size="sm"
                    onClick={state.handleExportCsv}
                    className="gap-1.5 text-xs justify-start"
                    title={t('history.exportCsv')}
                    disabled={state.filteredHistory.length === 0}
                >
                    <FileSpreadsheet size={14} />
                    <span>{t('history.exportCsv')}</span>
                </Button>
            }
        >
            <div className="max-w-4xl mx-auto space-y-5">
                {state.exportSuccessMsg && (
                    <AlertBanner variant="success">{state.exportSuccessMsg}</AlertBanner>
                )}
                {state.exportErrorMsg && (
                    <AlertBanner variant="error">{state.exportErrorMsg}</AlertBanner>
                )}

                <HistoryFilterBar
                    searchQuery={state.searchQuery}
                    onSearchChange={state.setSearchQuery}
                    directionFilter={state.directionFilter}
                    onDirectionChange={state.setDirectionFilter}
                    currencyFilter={state.currencyFilter}
                    onCurrencyChange={state.setCurrencyFilter}
                    periodFilter={state.periodFilter}
                    onPeriodChange={state.setPeriodFilter}
                    availableCurrencies={state.availableCurrencies}
                    summaryData={state.summaryData}
                />

                {state.isLoading && (
                    <div className="text-center py-20 text-theme-light animate-pulse font-black uppercase tracking-[0.2em]">
                        {t('history.loading')}
                    </div>
                )}

                {state.error && (
                    <div className="p-8 text-center bg-rose-50 border border-rose-100 rounded-3xl text-rose-500 font-bold">
                        {state.error}
                    </div>
                )}

                {!state.isLoading && !state.error && (
                    <div className="space-y-2.5">
                        {state.filteredHistory.length > 0 ? (
                            state.filteredHistory.map((record) => (
                                <TransactionHistoryRow
                                    key={record.id}
                                    record={record}
                                    events={state.events}
                                    isExpanded={state.expandedId === record.id}
                                    isUnacknowledged={Boolean(
                                        record.bundleId &&
                                            state.unacknowledgedPayments.includes(record.bundleId),
                                    )}
                                    isSavingBundle={state.isSavingBundle === record.id}
                                    bundleFeedback={state.bundleFeedback[record.id]}
                                    isRedepositing={
                                        Boolean(record.bundleId) &&
                                        state.isRedepositingId === record.bundleId
                                    }
                                    redepositFeedback={
                                        record.bundleId
                                            ? state.redepositFeedback[record.bundleId]
                                            : undefined
                                    }
                                    resolveIdentity={state.resolveIdentity}
                                    isKnownContact={state.isKnownContact}
                                    onToggleExpand={(id) =>
                                        state.setExpandedId((prev) => (prev === id ? null : id))
                                    }
                                    onSaveBundle={state.handleSaveBundle}
                                    onRedeposit={state.handleRedeposit}
                                    onOpenContactDialog={state.setContactModalData}
                                />
                            ))
                        ) : (
                            <div className="text-center py-14 px-6 bg-white/40 backdrop-blur-sm border border-dashed border-theme-subtle rounded-3xl space-y-3">
                                <div className="inline-flex items-center justify-center h-14 w-14 rounded-full bg-theme-subtle/20 text-theme-light">
                                    <Info size={28} />
                                </div>
                                <p className="text-theme-secondary font-bold text-sm">
                                    {t('history.noMatchingTx')}
                                </p>
                                <p className="text-xs text-theme-light">
                                    {t('history.adjustFilter')}
                                </p>
                                <div className="flex flex-wrap items-center justify-center gap-2 pt-2">
                                    {state.history.length > 0 ? (
                                        <Button
                                            variant="outline"
                                            size="sm"
                                            onClick={state.resetFilters}
                                        >
                                            {t('voucher.filter.reset')}
                                        </Button>
                                    ) : (
                                        <>
                                            <Button
                                                variant="primary"
                                                size="sm"
                                                onClick={() =>
                                                    state.navigate({
                                                        view: 'create_voucher',
                                                        previousView: {
                                                            view: 'transaction_history',
                                                        },
                                                    })
                                                }
                                            >
                                                {t('voucher.create')}
                                            </Button>
                                            <Button
                                                variant="outline"
                                                size="sm"
                                                onClick={() =>
                                                    state.navigate({ view: 'receive_bundle' })
                                                }
                                            >
                                                {t('transfer.receive')}
                                            </Button>
                                        </>
                                    )}
                                </div>
                            </div>
                        )}
                    </div>
                )}

                {!state.isLoading && !state.error && state.events.length > 0 && (
                    <Advanced>
                        <Disclosure
                            toggleTestId="history-event-log-toggle"
                            contentTestId="history-event-log-content"
                            icon={<History size={16} className="text-theme-light" />}
                            title={
                                <span className="text-xs font-bold text-theme-secondary uppercase tracking-wider">
                                    {t('history.eventLogSectionTitle', {
                                        count: state.events.length,
                                    })}
                                </span>
                            }
                            className="bg-white/80 border border-theme-subtle rounded-2xl overflow-hidden shadow-xs"
                            buttonClassName="w-full flex items-center justify-between gap-3 px-4 py-3 text-left hover:bg-bg-app/50 transition-colors cursor-pointer"
                            contentClassName="px-4 pb-4 pt-2 border-t border-theme-subtle/60 space-y-2"
                        >
                            {state.events.map((ev) => {
                                const pres = getWalletEventPresentation(
                                    ev,
                                    'history',
                                    t,
                                    state.resolveIdentity,
                                );
                                const Icon = HISTORY_EVENT_ICONS[pres.kind];
                                return (
                                    <button
                                        key={ev.eventId}
                                        type="button"
                                        onClick={() => {
                                            if (!pres.isTransfer) {
                                                state.navigate({
                                                    view: 'voucher_details',
                                                    voucherId: ev.localInstanceId,
                                                    previousView: { view: 'transaction_history' },
                                                });
                                            }
                                        }}
                                        className={`w-full text-left p-3 rounded-xl border border-theme-subtle/70 bg-white flex items-center justify-between gap-3 transition-colors ${
                                            pres.isTransfer
                                                ? 'cursor-default'
                                                : 'hover:border-theme-primary/40 hover:bg-bg-app/30 cursor-pointer'
                                        }`}
                                    >
                                        <div className="flex items-center gap-3 min-w-0">
                                            <div
                                                className={`flex items-center justify-center h-9 w-9 rounded-xl shrink-0 ${pres.bgColor} ${pres.color}`}
                                            >
                                                <Icon size={17} />
                                            </div>
                                            <div className="min-w-0">
                                                <p className="text-xs font-bold text-theme-secondary truncate">
                                                    {pres.label}
                                                </p>
                                                <div className="flex items-center gap-2 mt-0.5 text-[10px] font-mono text-theme-light">
                                                    <span>#{ev.localInstanceId.slice(0, 8)}</span>
                                                    {pres.amountDisplay && (
                                                        <span className="font-bold text-theme-accent">
                                                            {pres.amountDisplay}
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

            {state.contactModalData && (
                <ContactDialog
                    isOpen={true}
                    initialDid={state.contactModalData.did}
                    initialProfile={
                        state.contactModalData.name
                            ? { firstName: state.contactModalData.name }
                            : null
                    }
                    onClose={() => state.setContactModalData(null)}
                    onSave={async (contact: Contact) => {
                        await contactService.saveContact(contact);
                        await state.refreshContacts();
                        state.setContactModalData(null);
                    }}
                />
            )}
        </PageLayout>
    );
}
