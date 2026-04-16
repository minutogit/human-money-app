// src/components/ConflictDetailsView.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { ProofOfDoubleSpend } from "../types";

interface ConflictDetailsViewProps {
    proofId: string;
    onBack: () => void;
}

export function ConflictDetailsView({ proofId, onBack }: ConflictDetailsViewProps) {
    const [proof, setProof] = useState<ProofOfDoubleSpend | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [errorMsg, setErrorMsg] = useState("");

    useEffect(() => {
        async function fetchProof() {
            setIsLoading(true);
            try {
                logger.info(`Fetching double-spend proof: ${proofId}`);
                const result = await invoke<ProofOfDoubleSpend>("get_proof_of_double_spend", { proofId });
                setProof(result);
            } catch (e) {
                const msg = `Failed to fetch double-spend proof: ${e}`;
                logger.error(msg);
                setErrorMsg(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchProof();
    }, [proofId]);

    if (isLoading) return <div className="text-center p-8 text-theme-light">Loading Proof Details...</div>;
    if (errorMsg) return (
        <div className="p-8 text-center">
            <p className="text-red-500 font-bold mb-4">Error: {errorMsg}</p>
            <Button onClick={onBack}>Back</Button>
        </div>
    );
    if (!proof) return <div className="text-center p-8 text-theme-light">Proof not found.</div>;

    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';

    return (
        <div className="max-w-4xl mx-auto space-y-6 animate-in fade-in duration-500">
            <header className="flex items-center gap-4">
                <button
                    onClick={onBack}
                    className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                    title="Back"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                    </svg>
                </button>
                <h1 className="text-2xl font-bold text-red-600 flex items-center gap-2">
                    <span>🚫</span> Double-Spend Conflict Report
                </h1>
            </header>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div className="space-y-6">
                    <div className="bg-white dark:bg-gray-800 rounded-lg border border-red-200 shadow-sm p-6">
                        <h2 className="text-lg font-bold text-red-800 mb-4 border-b border-red-100 pb-2">Offender Information</h2>
                        <div className="space-y-4">
                            <div>
                                <p className="text-xs font-semibold text-gray-500 uppercase tracking-wider">Offender ID (DID)</p>
                                <p className="font-mono text-sm break-all text-red-700 bg-red-50 p-2 rounded mt-1">{proof.offender_id}</p>
                            </div>
                            <p className="text-sm text-gray-600 leading-relaxed italic">
                                This user has mathematically been proven to have spent the same funds twice. 
                                Their identity was extracted using the identity-trap mechanism.
                            </p>
                        </div>
                    </div>

                    <div className="bg-white dark:bg-gray-800 rounded-lg border border-theme-subtle shadow-sm p-6">
                        <h2 className="text-lg font-bold text-theme-primary mb-4 border-b border-theme-subtle pb-2">Proof Metadata</h2>
                        <div className="space-y-4">
                            <div className="grid grid-cols-2 gap-4">
                                <div>
                                    <p className="text-xs font-semibold text-theme-light uppercase">Report Date</p>
                                    <p className="text-sm">{formatDateTime(proof.report_timestamp)}</p>
                                </div>
                                <div>
                                    <p className="text-xs font-semibold text-theme-light uppercase">Status</p>
                                    <span className={`text-[10px] items-center px-2 py-0.5 rounded-full font-bold uppercase ${proof.resolutions?.length ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'}`}>
                                        {proof.resolutions?.length ? 'Resolved' : 'Unresolved'}
                                    </span>
                                </div>
                            </div>
                            <div>
                                <p className="text-xs font-semibold text-theme-light uppercase">Proof ID</p>
                                <p className="font-mono text-[10px] break-all text-theme-light">{proof.proof_id}</p>
                            </div>
                            <div>
                                <p className="text-xs font-semibold text-theme-light uppercase">Reporter ID</p>
                                <p className="font-mono text-[10px] break-all text-theme-light">{proof.reporter_id}</p>
                            </div>
                        </div>
                    </div>
                </div>

                <div className="bg-white dark:bg-gray-800 rounded-lg border border-theme-subtle shadow-sm p-6">
                    <h2 className="text-lg font-bold text-theme-primary mb-4 border-b border-theme-subtle pb-2">Conflicting Transactions</h2>
                    <p className="text-xs text-theme-light mb-4">Evidence of the two transactions that caused this conflict:</p>
                    
                    <div className="space-y-4">
                        {proof.conflicting_transactions.map((tx, idx) => (
                            <div key={tx.t_id} className={`p-3 rounded border-l-4 ${idx === 0 ? 'border-green-500 bg-green-50/30' : 'border-red-500 bg-red-50/30'}`}>
                                <div className="flex justify-between items-center mb-2">
                                    <span className="text-[10px] font-bold uppercase text-gray-400">Transaction {idx + 1} {idx === 0 && '(Winner)'}</span>
                                    <span className="text-sm font-bold">{tx.amount}</span>
                                </div>
                                <div className="space-y-1 text-[10px] font-mono text-gray-600">
                                    <p><strong>ID:</strong> {tx.t_id}</p>
                                    <p><strong>Time:</strong> {formatDateTime(tx.t_time)}</p>
                                    <p><strong>Recipient:</strong> {tx.recipient_id.slice(0, 20)}...</p>
                                </div>
                            </div>
                        ))}
                    </div>
                    
                    <div className="mt-6 p-3 bg-blue-50 border border-blue-100 rounded text-xs text-blue-800 leading-relaxed">
                        <p><strong>Why is one a winner?</strong></p>
                        <p className="mt-1">In an offline conflict, the transaction with the <strong>earlier time-stamp</strong> is considered valid. The later one is fraudulent.</p>
                    </div>
                </div>
            </div>

            {proof.resolutions && proof.resolutions.length > 0 && (
                <div className="bg-green-50 rounded-lg border border-green-200 p-6">
                    <h2 className="text-lg font-bold text-green-800 mb-4">Resolutions</h2>
                    <div className="space-y-3">
                        {proof.resolutions.map(res => (
                            <div key={res.endorsement_id} className="bg-white p-3 rounded border border-green-100 shadow-sm text-sm">
                                <div className="flex justify-between mb-1">
                                    <p className="font-bold">Compensation Endorsement</p>
                                    <p className="text-xs text-gray-400">{formatDateTime(res.resolution_timestamp)}</p>
                                </div>
                                <p className="text-gray-600 text-xs">The victim ({res.victim_id.slice(0, 15)}...) has confirmed being compensated for the loss.</p>
                                {res.notes && <p className="mt-2 text-xs italic text-gray-500">"{res.notes}"</p>}
                            </div>
                        ))}
                    </div>
                </div>
            )}

            <div className="flex justify-center pt-4">
                <Button size="lg" onClick={onBack} variant="outline" className="px-12">Close Report</Button>
            </div>
        </div>
    );
}
