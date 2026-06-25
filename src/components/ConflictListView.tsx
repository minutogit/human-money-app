// src/components/ConflictListView.tsx
import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { integrityService } from "../services/integrityService";
import { logger } from "../utils/log";
import { translateError, stringifyError } from "../utils/errorHelper";
import { ProofOfDoubleSpendSummary } from "../types";
import { Button } from "./ui/Button";

interface ConflictListViewProps {
    onBack: () => void;
    onViewConflict: (proofId: string) => void;
}

export function ConflictListView({ onBack, onViewConflict }: ConflictListViewProps) {
    const { t } = useTranslation();
    const [conflicts, setConflicts] = useState<ProofOfDoubleSpendSummary[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [errorMsg, setErrorMsg] = useState("");

    useEffect(() => {
        logger.info("ConflictListView: Loading conflicts");
        async function fetchConflicts() {
            setIsLoading(true);
            setErrorMsg("");
            try {
                const result = await integrityService.getDoubleSpendConflicts();
                setConflicts(result);
            } catch (e) {
                const msg = t('conflict.fetchFailed', { error: translateError(e, t) });
                logger.error(`Failed to fetch conflicts: ${stringifyError(e)}`);
                setErrorMsg(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchConflicts();
    }, [t]);

    function formatTimestamp(isoString: string): string {
        return new Date(isoString).toLocaleString();
    }

    function truncateId(id: string): string {
        return `${id.substring(0, 12)}...${id.substring(id.length - 8)}`;
    }

    if (isLoading) {
        return (
            <div className="flex h-full items-center justify-center">
                <p className="text-theme-light">{t('conflict.loading')}</p>
            </div>
        );
    }

    if (errorMsg) {
        return (
            <div className="p-6">
                <div className="text-center p-8 text-theme-error bg-red-100 rounded-lg">
                    <p className="font-bold">{t('conflict.loadErrorTitle')}</p>
                    <p className="text-sm mt-2 font-mono">{errorMsg}</p>
                    <Button onClick={onBack} variant="secondary" className="mt-4">
                        {t('common.back')}
                    </Button>
                </div>
            </div>
        );
    }

    const victimConflicts = conflicts.filter(c => c.conflictRole === "victim");
    const witnessConflicts = conflicts.filter(c => c.conflictRole === "witness");

    const ConflictCard = ({ conflict }: { conflict: ProofOfDoubleSpendSummary }) => (
        <div
            key={conflict.proofId}
            className={`border rounded-lg shadow-sm hover:shadow-md transition-all ${
                conflict.conflictRole === 'victim' 
                    ? 'bg-red-50 border-red-200 hover:border-red-400' 
                    : 'bg-bg-card-alternate border-theme-subtle hover:border-theme-primary'
            }`}
        >
            <div className="p-6">
                <div className="flex items-start justify-between mb-4">
                    <div className="flex-grow">
                        <div className="flex flex-wrap items-center gap-2 mb-2">
                            <span className={`px-3 py-1 text-xs font-bold rounded-full ${
                                conflict.conflictRole === 'victim'
                                    ? 'bg-red-600 text-white'
                                    : 'bg-gray-500 text-white'
                            }`}>
                                {conflict.conflictRole === 'victim' ? t('conflict.actionRequired') : t('conflict.securityInsight')}
                            </span>
                            {conflict.isResolved ? (
                                <span className="px-3 py-1 text-xs font-bold rounded-full bg-green-100 text-green-800 border border-green-200">
                                    {t('conflict.resolvedOfficially')}
                                </span>
                            ) : conflict.localOverride ? (
                                <span className="px-3 py-1 text-xs font-bold rounded-full bg-blue-100 text-blue-800 border border-blue-200">
                                    {t('conflict.resolvedLocally')}
                                </span>
                            ) : (
                                <span className="px-3 py-1 text-xs font-bold rounded-full bg-red-100 text-red-800 border border-red-200">
                                    {t('conflict.unresolved')}
                                </span>
                            )}
                            {conflict.hasL2Verdict && (
                                <span className="px-3 py-1 text-xs font-bold rounded-full bg-purple-100 text-purple-800">
                                    {t('conflict.l2VerdictAvailable')}
                                </span>
                            )}
                        </div>
                        <h3 className="font-semibold text-theme-primary mb-1">
                            {conflict.affectedVoucherName || t('conflict.unknownVoucher')}
                        </h3>
                        {conflict.localNote && (
                             <div className="mb-3">
                                <p className="text-[10px] font-bold text-blue-600 uppercase tracking-tight mb-1">{t('conflict.personalNote')}</p>
                                <p className="text-xs italic text-blue-800 bg-blue-50 px-2 py-1.5 rounded border border-blue-100 line-clamp-1 max-w-lg">
                                   "{conflict.localNote}"
                                </p>
                             </div>
                        )}
                        <p className="text-sm text-theme-light">
                            {t('conflict.idLabel')} <span className="font-mono text-xs">{truncateId(conflict.proofId)}</span>
                        </p>
                        <p className="text-sm text-theme-light">
                            {t('conflict.offenderLabel')} <span className="font-mono text-xs">{truncateId(conflict.offenderId)}</span>
                        </p>
                        <p className="text-sm text-theme-light">
                            {t('conflict.reportedLabel', { date: formatTimestamp(conflict.reportTimestamp) })}
                        </p>
                    </div>
                    <Button
                        onClick={() => onViewConflict(conflict.proofId)}
                        variant={conflict.conflictRole === 'victim' && !conflict.localOverride && !conflict.isResolved ? 'primary' : 'secondary'}
                        size="sm"
                        className="whitespace-nowrap"
                    >
                        {t('conflict.viewDetails')}
                    </Button>
                </div>
                <div className="border-t border-theme-subtle pt-3">
                    <p className="text-xs text-theme-light">
                        <strong>{t('conflict.forkPointLabel')}</strong> <span className="font-mono">{truncateId(conflict.forkPointPrevHash)}</span>
                    </p>
                </div>
            </div>
        </div>
    );

    return (
        <div className="flex flex-col h-full">
            <header className="flex items-center gap-4 border-b border-theme-subtle px-6 py-4 bg-bg-card">
                <button
                    onClick={onBack}
                    className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                    title={t('common.back')}
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                    </svg>
                </button>
                <h1 className="text-2xl font-bold text-theme-primary">{t('conflict.title')}</h1>
                <div className="flex-grow"></div>
                <div className="text-sm text-theme-light">
                    {conflicts.length === 1 ? t('conflict.reportTotalCount', { count: conflicts.length }) : t('conflict.reportsTotalCount', { count: conflicts.length })}
                </div>
            </header>

            <div className="flex-grow overflow-y-auto p-4 sm:p-6 bg-bg-main">
                {conflicts.length === 0 ? (
                    <div className="text-center py-12">
                        <div className="text-6xl mb-4">✅</div>
                        <h2 className="text-xl font-semibold text-theme-primary mb-2">{t('conflict.noConflictsTitle')}</h2>
                        <p className="text-theme-light">{t('conflict.noConflictsDescription')}</p>
                    </div>
                ) : (
                    <div className="max-w-4xl mx-auto space-y-8">
                        {victimConflicts.length > 0 && (
                            <section className="space-y-4">
                                <h2 className="text-lg font-bold text-red-700 flex items-center gap-2">
                                    <span className="flex h-3 w-3 rounded-full bg-red-600 animate-pulse"></span>
                                    {t('conflict.victimConflictsHeader', { count: victimConflicts.length })}
                                </h2>
                                <div className="space-y-4">
                                    {victimConflicts.map(c => <ConflictCard key={c.proofId} conflict={c} />)}
                                </div>
                            </section>
                        )}

                        {witnessConflicts.length > 0 && (
                            <section className="space-y-4">
                                <h2 className="text-lg font-bold text-theme-light flex items-center gap-2">
                                    {t('conflict.witnessConflictsHeader', { count: witnessConflicts.length })}
                                </h2>
                                <details className="group">
                                    <summary className="list-none cursor-pointer flex items-center gap-2 text-theme-light hover:text-theme-primary transition-colors py-2 font-medium">
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4 transform group-open:rotate-180 transition-transform" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                                        </svg>
                                        {t('conflict.showWitnessConflicts')}
                                    </summary>
                                    <div className="space-y-4 mt-4 pt-4 border-t border-theme-subtle">
                                        {witnessConflicts.map(c => <ConflictCard key={c.proofId} conflict={c} />)}
                                    </div>
                                </details>
                            </section>
                        )}
                    </div>
                )}
            </div>
        </div>
    );
}
