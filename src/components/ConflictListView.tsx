// src/components/ConflictListView.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logger } from "../utils/log";
import { ProofOfDoubleSpendSummary } from "../types";
import { Button } from "./ui/Button";

interface ConflictListViewProps {
    onBack: () => void;
    onViewConflict: (proofId: string) => void;
}

export function ConflictListView({ onBack, onViewConflict }: ConflictListViewProps) {
    const [conflicts, setConflicts] = useState<ProofOfDoubleSpendSummary[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [errorMsg, setErrorMsg] = useState("");

    useEffect(() => {
        logger.info("ConflictListView: Loading conflicts");
        async function fetchConflicts() {
            setIsLoading(true);
            setErrorMsg("");
            try {
                const result = await invoke<ProofOfDoubleSpendSummary[]>("get_double_spend_conflicts");
                setConflicts(result);
            } catch (e) {
                const msg = `Failed to fetch conflicts: ${e}`;
                logger.error(msg);
                setErrorMsg(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchConflicts();
    }, []);

    function formatTimestamp(isoString: string): string {
        return new Date(isoString).toLocaleString();
    }

    function truncateId(id: string): string {
        return `${id.substring(0, 12)}...${id.substring(id.length - 8)}`;
    }

    if (isLoading) {
        return (
            <div className="flex h-full items-center justify-center">
                <p className="text-theme-light">Loading fraud reports...</p>
            </div>
        );
    }

    if (errorMsg) {
        return (
            <div className="p-6">
                <div className="text-center p-8 text-theme-error bg-red-100 rounded-lg">
                    <p className="font-bold">Error Loading Fraud Reports</p>
                    <p className="text-sm mt-2 font-mono">{errorMsg}</p>
                    <Button onClick={onBack} variant="secondary" className="mt-4">
                        Back
                    </Button>
                </div>
            </div>
        );
    }

    const victimConflicts = conflicts.filter(c => c.conflict_role === "Victim");
    const witnessConflicts = conflicts.filter(c => c.conflict_role === "Witness");

    const ConflictCard = ({ conflict }: { conflict: ProofOfDoubleSpendSummary }) => (
        <div
            key={conflict.proof_id}
            className={`border rounded-lg shadow-sm hover:shadow-md transition-all ${
                conflict.conflict_role === 'Victim' 
                    ? 'bg-red-50 border-red-200 hover:border-red-400' 
                    : 'bg-bg-card-alternate border-theme-subtle hover:border-theme-primary'
            }`}
        >
            <div className="p-6">
                <div className="flex items-start justify-between mb-4">
                    <div className="flex-grow">
                        <div className="flex flex-wrap items-center gap-2 mb-2">
                            <span className={`px-3 py-1 text-xs font-bold rounded-full ${
                                conflict.conflict_role === 'Victim'
                                    ? 'bg-red-600 text-white'
                                    : 'bg-gray-500 text-white'
                            }`}>
                                {conflict.conflict_role === 'Victim' ? 'URGENT: Affected Your Vouchers' : 'Network Observation'}
                            </span>
                            {conflict.is_resolved ? (
                                <span className="px-3 py-1 text-xs font-bold rounded-full bg-green-100 text-green-800 border border-green-200">
                                    ✓ Officially Resolved
                                </span>
                            ) : conflict.local_override ? (
                                <span className="px-3 py-1 text-xs font-bold rounded-full bg-blue-100 text-blue-800 border border-blue-200">
                                    ✓ Locally Settled
                                </span>
                            ) : (
                                <span className="px-3 py-1 text-xs font-bold rounded-full bg-red-100 text-red-800 border border-red-200">
                                    ⚠ Unresolved
                                </span>
                            )}
                            {conflict.has_l2_verdict && (
                                <span className="px-3 py-1 text-xs font-bold rounded-full bg-purple-100 text-purple-800">
                                    L2 Verdict Available
                                </span>
                            )}
                        </div>
                        <h3 className="font-semibold text-theme-primary mb-1">
                            {conflict.affected_voucher_name || "Unknown Voucher"}
                        </h3>
                        {conflict.local_note && (
                             <div className="mb-3">
                                <p className="text-[10px] font-bold text-blue-600 uppercase tracking-tight mb-1">Personal Note:</p>
                                <p className="text-xs italic text-blue-800 bg-blue-50 px-2 py-1.5 rounded border border-blue-100 line-clamp-1 max-w-lg">
                                   "{conflict.local_note}"
                                </p>
                             </div>
                        )}
                        <p className="text-sm text-theme-light">
                            ID: <span className="font-mono text-xs">{truncateId(conflict.proof_id)}</span>
                        </p>
                        <p className="text-sm text-theme-light">
                            Offender: <span className="font-mono text-xs">{truncateId(conflict.offender_id)}</span>
                        </p>
                        <p className="text-sm text-theme-light">
                            Reported: {formatTimestamp(conflict.report_timestamp)}
                        </p>
                    </div>
                    <Button
                        onClick={() => onViewConflict(conflict.proof_id)}
                        variant={conflict.conflict_role === 'Victim' && !conflict.local_override && !conflict.is_resolved ? 'primary' : 'secondary'}
                        size="sm"
                        className="whitespace-nowrap"
                    >
                        View Details
                    </Button>
                </div>
                <div className="border-t border-theme-subtle pt-3">
                    <p className="text-xs text-theme-light">
                        <strong>Fork Point:</strong> <span className="font-mono">{truncateId(conflict.fork_point_prev_hash)}</span>
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
                    title="Back"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                    </svg>
                </button>
                <h1 className="text-2xl font-bold text-theme-primary">Fraud Reports</h1>
                <div className="flex-grow"></div>
                <div className="text-sm text-theme-light">
                    {conflicts.length} {conflicts.length === 1 ? 'report' : 'reports'} total
                </div>
            </header>

            <div className="flex-grow overflow-y-auto p-4 sm:p-6 bg-bg-main">
                {conflicts.length === 0 ? (
                    <div className="text-center py-12">
                        <div className="text-6xl mb-4">✅</div>
                        <h2 className="text-xl font-semibold text-theme-primary mb-2">No Fraud Reports</h2>
                        <p className="text-theme-light">No double-spend conflicts have been detected.</p>
                    </div>
                ) : (
                    <div className="max-w-4xl mx-auto space-y-8">
                        {victimConflicts.length > 0 && (
                            <section className="space-y-4">
                                <h2 className="text-lg font-bold text-red-700 flex items-center gap-2">
                                    <span className="flex h-3 w-3 rounded-full bg-red-600 animate-pulse"></span>
                                    Requires Action: Your Payments ({victimConflicts.length})
                                </h2>
                                <div className="space-y-4">
                                    {victimConflicts.map(c => <ConflictCard key={c.proof_id} conflict={c} />)}
                                </div>
                            </section>
                        )}

                        {witnessConflicts.length > 0 && (
                            <section className="space-y-4">
                                <h2 className="text-lg font-bold text-theme-light flex items-center gap-2">
                                    Network Safety Reports ({witnessConflicts.length})
                                </h2>
                                <details className="group">
                                    <summary className="list-none cursor-pointer flex items-center gap-2 text-theme-light hover:text-theme-primary transition-colors py-2 font-medium">
                                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4 transform group-open:rotate-180 transition-transform" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                                        </svg>
                                        Show indirect network observations
                                    </summary>
                                    <div className="space-y-4 mt-4 pt-4 border-t border-theme-subtle">
                                        {witnessConflicts.map(c => <ConflictCard key={c.proof_id} conflict={c} />)}
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
