// src/components/VoucherDetailsView.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VoucherDetails, Transaction, GuarantorSignature } from "../types";
import { Button } from "./ui/Button";

interface VoucherDetailsViewProps {
    voucherId: string;
    onBack: () => void;
}

// Helper function to format date and time
function formatDateTime(isoString: string | undefined): string {
    if (!isoString) return 'N/A';
    return new Date(isoString).toLocaleString(undefined, {
        year: 'numeric', month: 'short', day: 'numeric',
        hour: '2-digit', minute: '2-digit'
    });
}

// Helper function to truncate text
function truncate(text: string | undefined, length: number): string {
    if (!text) return 'N/A';
    if (text.length <= length) return text;
    return text.substring(0, length) + '...';
}

export function VoucherDetailsView({ voucherId, onBack }: VoucherDetailsViewProps) {
    const [details, setDetails] = useState<VoucherDetails | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [errorMsg, setErrorMsg] = useState("");
    const [showJson, setShowJson] = useState(false);

    useEffect(() => {
        async function fetchDetails() {
            // Log to Rust terminal that we're fetching details
            invoke("frontend_log", { message: `VoucherDetailsView: Fetching details for voucherId: ${voucherId}` }).catch(console.error);
            setIsLoading(true);
            setErrorMsg("");
            try {
                const result = await invoke<VoucherDetails>("get_voucher_details", { localId: voucherId });
                // Log to Rust terminal the result we received
                invoke("frontend_log", { message: `VoucherDetailsView: Received result for voucherId: ${voucherId}, hasDetails: ${!!result}` }).catch(console.error);
                setDetails(result);
            } catch (e) {
                const msg = `Failed to fetch voucher details: ${e}`;
                // Log error to Rust terminal
                invoke("frontend_log", { message: `VoucherDetailsView: Error details for voucherId: ${voucherId}, error: ${e}` }).catch(console.error);
                setErrorMsg(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchDetails();
    }, [voucherId]);

    if (isLoading) {
        return <div className="text-center p-8">
            <p>Loading voucher details for ID: {voucherId}</p>
            <p className="text-sm text-gray-500 mt-2">This may take a few seconds...</p>
        </div>;
    }

    if (errorMsg) {
        return <div className="text-center p-8 text-red-500">
            <p>Error: {errorMsg}</p>
            <p className="text-sm text-gray-500 mt-2">Voucher ID requested: {voucherId}</p>
        </div>;
    }

    if (!details) {
        return <div className="text-center p-8">
            <p>No voucher details found.</p>
            <p className="text-sm text-gray-500 mt-2">Voucher ID requested: {voucherId}</p>
            <p className="text-sm text-gray-500">Data received was: {JSON.stringify(details)}</p>
        </div>;
    }

    return (
        <div className="max-w-4xl mx-auto space-y-6">
            {/* Header */}
            <div className="flex justify-between items-center">
                <Button onClick={onBack} variant="outline">
                    &larr; Back to Dashboard
                </Button>
                <div className="text-right">
                    <p className="text-4xl font-bold text-theme-primary">{details.nominal_value?.amount || 'N/A'}</p>
                    <p className="text-lg text-theme-light -mt-1">{details.nominal_value?.unit || ''}</p>
                </div>
            </div>

            {/* Toggle Button */}
            <div className="text-center">
                <Button onClick={() => setShowJson(!showJson)} variant="secondary">
                    {showJson ? "Show Formatted View" : "Show Raw JSON Data"}
                </Button>
            </div>

            {/* Content */}
            {showJson ? (
                <div className="bg-card border border-theme-subtle rounded-lg p-4">
                    <pre className="text-xs whitespace-pre-wrap break-all font-mono">
                        {JSON.stringify(details, null, 2)}
                    </pre>
                </div>
            ) : (
                <div className="space-y-6">
                    {/* Main Details Card */}
                    <div className="bg-card border border-theme-subtle rounded-lg p-6 shadow-sm">
                        <h2 className="text-xl font-semibold mb-4 border-b border-theme-subtle pb-2">{details.voucher_standard?.name || 'N/A'}</h2>
                        <p className="text-sm text-theme-secondary mb-4">{details.description || 'N/A'}</p>
                        <dl className="grid grid-cols-1 md:grid-cols-2 gap-x-6 gap-y-3 text-sm">
                            <dt className="font-semibold text-theme-light">Voucher ID</dt>
                            <dd className="font-mono text-xs">{details.voucher_id || 'N/A'}</dd>

                            <dt className="font-semibold text-theme-light">Created</dt>
                            <dd>{formatDateTime(details.creation_date || '')}</dd>

                            <dt className="font-semibold text-theme-light">Valid Until</dt>
                            <dd>{details.valid_until ? formatDateTime(details.valid_until) : 'N/A'}</dd>

                            <dt className="font-semibold text-theme-light">Divisible</dt>
                            <dd>{details.divisible ? 'Yes' : 'No'}</dd>
                        </dl>
                    </div>

                    {/* Creator Card */}
                    <div className="bg-card border border-theme-subtle rounded-lg p-6 shadow-sm">
                        <h3 className="text-lg font-semibold mb-3">Creator</h3>
                        <p>{details.creator?.first_name || ''} {details.creator?.last_name || ''}</p>
                        <p className="text-xs font-mono text-theme-light mt-1">{truncate(details.creator?.id, 40)}</p>
                    </div>

                    {/* Guarantors Card */}
                    <div className="bg-card border border-theme-subtle rounded-lg p-6 shadow-sm">
                        <h3 className="text-lg font-semibold mb-3">Guarantors ({(details.guarantor_signatures?.length || 0)} / {details.needed_guarantors || 0})</h3>
                        <p className="text-sm text-theme-secondary mb-4">{details.guarantor_requirements_description || 'N/A'}</p>
                        <div className="space-y-3">
                            {(details.guarantor_signatures || []).map(g => (
                                <div key={g.signature_id} className="border-t border-theme-subtle pt-2">
                                    <p className="font-semibold">{g.first_name || ''} {g.last_name || ''}</p>
                                    <p className="text-xs font-mono text-theme-light">{truncate(g.guarantor_id, 40)}</p>
                                    <p className="text-xs text-theme-light">Signed on: {formatDateTime(g.signature_time || '')}</p>
                                </div>
                            ))}
                            {(details.guarantor_signatures?.length || 0) === 0 && <p className="text-sm text-theme-light">No guarantors have signed yet.</p>}
                        </div>
                    </div>

                    {/* Transactions Card */}
                    <div className="bg-card border border-theme-subtle rounded-lg p-6 shadow-sm">
                        <h3 className="text-lg font-semibold mb-3">Transaction History ({details.transactions?.length || 0})</h3>
                        <div className="space-y-4 max-h-96 overflow-y-auto pr-2">
                            {(details.transactions || []).slice().reverse().map(t => (
                                <div key={t.t_id} className="border-t border-theme-subtle pt-3">
                                    <div className="flex justify-between items-center mb-1">
                                        <p className="font-semibold capitalize text-theme-primary">{t.t_type}</p>
                                        <p className="text-sm font-semibold">{t.amount} {details.nominal_value?.unit}</p>
                                    </div>
                                    <p className="text-xs text-theme-light">{formatDateTime(t.t_time)}</p>
                                    <p className="text-xs font-mono mt-2">
                                        <span className="font-semibold">From:</span> {truncate(t.sender_id, 30)}<br/>
                                        <span className="font-semibold">To: &nbsp;&nbsp;</span> {truncate(t.recipient_id, 30)}
                                    </p>
                                </div>
                            ))}
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}