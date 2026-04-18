import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { FullProofDetails } from "../types";

interface ConflictDetailsViewProps {
    proofId: string;
    onBack: () => void;
}

export function ConflictDetailsView({ proofId, onBack }: ConflictDetailsViewProps) {
    const [details, setDetails] = useState<FullProofDetails | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [isOverriding, setIsOverriding] = useState(false);
    const [errorMsg, setErrorMsg] = useState("");
    
    const [showPasswordModal, setShowPasswordModal] = useState(false);
    const [password, setPassword] = useState("");
    const [localNote, setLocalNote] = useState("");
    const [overrideError, setOverrideError] = useState("");

    useEffect(() => {
        async function fetchProof() {
            setIsLoading(true);
            try {
                logger.info(`Fetching double-spend proof details: ${proofId}`);
                const result = await invoke<FullProofDetails>("get_proof_of_double_spend", { proofId });
                setDetails(result);
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

    const handleLocalOverride = async () => {
        setIsOverriding(true);
        setOverrideError("");
        try {
            await invoke("set_conflict_local_override", { 
                proofId, 
                value: true, 
                note: localNote || null,
                password: password || null 
            });
            // Refresh details
            const updated = await invoke<FullProofDetails>("get_proof_of_double_spend", { proofId });
            setDetails(updated);
            setShowPasswordModal(false);
            setPassword("");
            setLocalNote("");
        } catch (e) {
            setOverrideError(String(e));
        } finally {
            setIsOverriding(false);
        }
    };

    if (isLoading) return <div className="text-center p-8 text-theme-light">Loading Proof Details...</div>;
    if (errorMsg) return (
        <div className="p-8 text-center">
            <p className="text-red-500 font-bold mb-4">Error: {errorMsg}</p>
            <Button onClick={onBack}>Back</Button>
        </div>
    );
    if (!details) return <div className="text-center p-8 text-theme-light">Proof not found.</div>;

    const { proof, local_override, conflict_role } = details;
    const formatDateTime = (iso?: string) => iso ? new Date(iso).toLocaleString() : 'N/A';

    return (
        <div className="max-w-4xl mx-auto space-y-6 animate-in fade-in duration-500 pb-12">
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
                <h1 className="text-2xl font-bold text-theme-primary flex items-center gap-2">
                    <span>{conflict_role === 'Victim' ? '🛑' : '👁️'}</span> 
                    {conflict_role === 'Victim' ? 'Critical Payment Conflict' : 'Network Observation Report'}
                </h1>
            </header>

            {conflict_role === 'Victim' ? (
                <div className="bg-red-50 border-l-4 border-red-500 p-4 rounded-r-lg">
                    <div className="flex">
                        <div className="flex-shrink-0">
                            <span className="text-red-500 text-xl">⚠️</span>
                        </div>
                        <div className="ml-3">
                            <h3 className="text-sm font-bold text-red-800">Action Required</h3>
                            <div className="mt-1 text-sm text-red-700 leading-relaxed">
                                This conflict directly affects your vouchers. The funds involved are currently <strong>Quarantined</strong> and cannot be spent until this is settled. Please contact the sender to resolve this discrepancy.
                            </div>
                        </div>
                    </div>
                </div>
            ) : (
                <div className="bg-blue-50 border-l-4 border-blue-500 p-4 rounded-r-lg">
                    <div className="flex">
                        <div className="flex-shrink-0">
                            <span className="text-blue-500 text-xl">ℹ️</span>
                        </div>
                        <div className="ml-3">
                            <h3 className="text-sm font-bold text-blue-800">Information Only</h3>
                            <div className="mt-1 text-sm text-blue-700 leading-relaxed">
                                This conflict was observed in the network but does not involve your active wallet balance. It serves as a warning about the offender for future interactions.
                            </div>
                        </div>
                    </div>
                </div>
            )}

            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div className="space-y-6">
                    <div className="bg-bg-card-alternate rounded-lg border border-theme-subtle shadow-sm p-6">
                        <h2 className="text-lg font-bold text-theme-primary mb-4 border-b border-theme-subtle pb-2">Offender Information</h2>
                        <div className="space-y-4">
                            <div>
                                <p className="text-xs font-semibold text-theme-light uppercase tracking-wider">Offender ID (DID)</p>
                                <p className="font-mono text-sm break-all text-red-700 bg-red-50 p-2 rounded mt-1">{proof.offender_id}</p>
                            </div>
                            <p className="text-sm text-theme-light leading-relaxed italic">
                                This user has been mathematically proven to have spent funds multiple times. 
                                Their identity was revealed using the network's identity-trap mechanism.
                            </p>
                        </div>
                    </div>

                    <div className="bg-bg-card rounded-lg border border-theme-subtle shadow-sm p-6">
                        <h2 className="text-lg font-bold text-theme-primary mb-4 border-b border-theme-subtle pb-2">Report Metadata</h2>
                        <div className="space-y-4">
                            <div className="grid grid-cols-2 gap-4">
                                <div>
                                    <p className="text-xs font-semibold text-theme-light uppercase">Report Date</p>
                                    <p className="text-sm">{formatDateTime(proof.report_timestamp)}</p>
                                </div>
                                <div>
                                    <p className="text-xs font-semibold text-theme-light uppercase">Trust Status</p>
                                    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-bold uppercase ${local_override ? 'bg-blue-100 text-blue-800' : proof.is_resolved ? 'bg-green-100 text-green-800' : 'bg-red-100 text-red-800'}`}>
                                        {proof.is_resolved ? 'Officially Resolved' : local_override ? 'Locally Settled' : 'Unresolved'}
                                    </span>
                                </div>
                            </div>
                            {details.local_note && (
                                <div className="space-y-1">
                                    <p className="text-[10px] font-bold text-blue-600 uppercase tracking-tight">Personal Resolution Note:</p>
                                    <div className="p-4 bg-blue-50 border border-blue-100 rounded-lg text-sm italic text-blue-900 shadow-sm leading-relaxed">
                                        "{details.local_note}"
                                    </div>
                                </div>
                            )}
                            <div>
                                <p className="text-xs font-semibold text-theme-light uppercase">Affected Voucher</p>
                                <p className="text-sm font-medium">{proof.affected_voucher_name || 'Generic Voucher-Instance'}</p>
                            </div>
                            <div>
                                <p className="text-xs font-semibold text-theme-light uppercase">Proof ID</p>
                                <p className="font-mono text-[10px] break-all text-theme-light">{proof.proof_id}</p>
                            </div>
                        </div>
                    </div>
                </div>

                <div className="bg-bg-card rounded-lg border border-theme-subtle shadow-sm p-6">
                    <h2 className="text-lg font-bold text-theme-primary mb-4 border-b border-theme-subtle pb-2">Evidence</h2>
                    <p className="text-xs text-theme-light mb-4">Cryptographic proof of conflicting transactions:</p>
                    
                    <div className="space-y-4">
                        {proof.conflicting_transactions.map((tx, idx) => (
                            <div key={tx.t_id} className={`p-3 rounded border-l-4 ${idx === 0 ? 'border-green-500 bg-green-50/30' : 'border-red-500 bg-red-50/30'}`}>
                                <div className="flex justify-between items-center mb-2">
                                    <span className="text-[10px] font-bold uppercase text-gray-400">Transaction {idx + 1} {idx === 0 && '(Winner)'}</span>
                                    <span className="text-sm font-bold">{tx.amount}</span>
                                </div>
                                <div className="space-y-1 text-[10px] font-mono text-theme-light">
                                    <p><strong>ID:</strong> {tx.t_id}</p>
                                    <p><strong>Time:</strong> {formatDateTime(tx.t_time)}</p>
                                    <p><strong>Recipient:</strong> {tx.recipient_id.slice(0, 20)}...</p>
                                </div>
                            </div>
                        ))}
                    </div>
                    
                    <div className="mt-6 p-3 bg-blue-50 border border-blue-100 rounded text-xs text-blue-800 leading-relaxed">
                        <p><strong>Protocol Rules:</strong></p>
                        <p className="mt-1">In this de-centralized network, the transaction with the <strong>earlier timestamp</strong> is valid. Subsequent transactions of the same funds are fraudulent.</p>
                    </div>
                </div>
            </div>

            {!local_override && !proof.is_resolved && (
                <div className="bg-white rounded-lg border border-theme-subtle p-8 shadow-sm text-center">
                    <h3 className="text-lg font-bold text-theme-primary mb-2">Resolution</h3>
                    <p className="text-sm text-theme-light mb-6 max-w-lg mx-auto">
                        If you have settled this issue with the person involved (e.g. they paid you through other means), 
                        you can mark this conflict as locally resolved to regain trust and unlock your vouchers.
                    </p>
                    <div className="max-w-md mx-auto space-y-4">
                        <textarea
                            className="w-full bg-bg-main border border-theme-subtle rounded-lg px-4 py-2 text-sm focus:ring-2 focus:ring-theme-primary outline-none transition-all h-24"
                            placeholder="Optional: Why are you settling this? (e.g. 'Paid via cash')"
                            value={localNote}
                            onChange={(e) => setLocalNote(e.target.value)}
                        />
                        <Button 
                            size="lg" 
                            variant="secondary"
                            className="w-full"
                            onClick={() => setShowPasswordModal(true)}
                        >
                            Mark as Locally Settled
                        </Button>
                    </div>
                </div>
            )}

            {proof.resolutions && proof.resolutions.length > 0 && (
                <div className="bg-green-50 rounded-lg border border-green-200 p-6">
                    <h2 className="text-lg font-bold text-green-800 mb-4">Official Resolutions</h2>
                    <div className="space-y-3">
                        {proof.resolutions.map(res => (
                            <div key={res.endorsement_id} className="bg-white p-3 rounded border border-green-100 shadow-sm text-sm">
                                <div className="flex justify-between mb-1">
                                    <p className="font-bold">Compensation Endorsement</p>
                                    <p className="text-xs text-gray-400">{formatDateTime(res.resolution_timestamp)}</p>
                                </div>
                                <p className="text-gray-600 text-xs">The victim ({res.victim_id.slice(0, 15)}...) confirmed solution.</p>
                                {res.notes && <p className="mt-2 text-xs italic text-gray-500">"{res.notes}"</p>}
                            </div>
                        ))}
                    </div>
                </div>
            )}

            <div className="flex justify-center pt-8">
                <Button size="lg" onClick={onBack} variant="outline" className="px-12">Close Report</Button>
            </div>

            {/* Password Modal */}
            {showPasswordModal && (
                <div className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-4">
                    <div className="bg-white rounded-xl shadow-2xl max-w-sm w-full p-6 animate-in zoom-in-95 duration-200">
                        <h3 className="text-xl font-bold text-theme-primary mb-2">Verify Security</h3>
                        <p className="text-sm text-theme-light mb-6">
                            Marking a conflict as settled requires your wallet password to sign the local policy update.
                        </p>
                        
                        <input
                            type="password"
                            placeholder="Enter Wallet Password"
                            className="w-full bg-bg-main border border-theme-subtle rounded-lg px-4 py-3 mb-4 focus:ring-2 focus:ring-theme-primary outline-none transition-all"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                            autoFocus
                        />

                        {overrideError && (
                            <div className="p-3 bg-red-50 text-red-600 text-xs rounded-lg mb-4 border border-red-100">
                                {overrideError}
                            </div>
                        )}

                        <div className="flex gap-3">
                            <Button 
                                className="flex-1" 
                                variant="secondary" 
                                onClick={() => { setShowPasswordModal(false); setOverrideError(""); setPassword(""); }}
                            >
                                Cancel
                            </Button>
                            <Button 
                                className="flex-1" 
                                variant="primary" 
                                onClick={handleLocalOverride}
                                isLoading={isOverriding}
                            >
                                Confirm
                            </Button>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}
