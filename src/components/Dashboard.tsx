// src/components/Dashboard.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { VoucherSummary } from "../types";

interface DashboardProps {
    onNavigateToCreateVoucher: () => void;
    onShowDetails: (voucherId: string) => void;
}

export function Dashboard({ onNavigateToCreateVoucher, onShowDetails }: DashboardProps) {
    const [userId, setUserId] = useState("");
    const [balances, setBalances] = useState<Record<string, string>>({});
    const [vouchers, setVouchers] = useState<VoucherSummary[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [copyFeedback, setCopyFeedback] = useState("");

    useEffect(() => {
        // Log when component is displayed
        logger.info("Dashboard component displayed");
        
        async function fetchData() {
            try {
                console.log("Dashboard: Fetching data from backend.");
                const [id, balanceMap, voucherList] = await Promise.all([
                    invoke<string>("get_user_id"),
                    invoke<Record<string, string>>("get_total_balance_by_currency"),
                    invoke<VoucherSummary[]>("get_voucher_summaries")
                ]);
                setUserId(id);
                setBalances(balanceMap);
                setVouchers(voucherList);
                console.log("Dashboard: Data successfully fetched.");
            } catch (e) {
                const msg = `Failed to fetch dashboard data: ${e}`;
                console.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
            }
        }
        fetchData();
    }, []);

    async function handleCopyUserId() {
        if (!userId) return;
        try {
            await writeText(userId);
            setCopyFeedback("ID copied!");
            setTimeout(() => setCopyFeedback(""), 2000); // Reset feedback after 2s
        } catch (e) {
            const msg = `Failed to copy User ID: ${e}`;
            console.error(msg);
            setFeedbackMsg(`Error: ${msg}`);
        }
    }

    // Helper function to extract and format the voucher status
    function getVoucherStatus(status: object): { name: string; color: string } {
        // Handle both simple string statuses (e.g., "Active") and object statuses (e.g., { Incomplete: ... })
        const statusName = (typeof status === 'string' ? status : Object.keys(status)[0])?.toLowerCase() || 'unknown';
        let color = 'text-gray-800 bg-gray-200'; // Default/Incomplete

        switch (statusName) {
            case 'active':
                color = 'text-green-800 bg-green-200';
                break;
            case 'quarantined':
                color = 'text-red-800 bg-red-200';
                break;
            case 'archived':
                color = 'text-indigo-800 bg-indigo-200';
                break;
        }
        return { name: statusName, color: color };
    }

    // Helper function to format the date
    function formatDate(isoString: string): string {
        if (!isoString) return 'N/A';
        return new Date(isoString).toLocaleDateString(undefined, {
            year: 'numeric', month: 'short', day: 'numeric'
        });
    }

    // Helper function to truncate text
    function truncate(text: string, length: number): string {
        if (text.length <= length) return text;
        return text.substring(0, length) + '...';
    }

    const truncatedUserId = userId ? `${userId.substring(0, 15)}...${userId.substring(userId.length - 8)}` : "Lade...";

    return (
        <div className="mx-auto max-w-4xl">
            {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}

            {/* User ID Anzeige */}
            <div className="mb-8 rounded-lg border border-theme-subtle bg-card p-4 shadow-sm">
                <h2 className="text-sm font-semibold text-theme-secondary mb-1">Your User ID</h2>
                <button onClick={handleCopyUserId} className="w-full text-left rounded-md hover:bg-bg-app transition-colors duration-150 ease-in-out focus:outline-none focus:ring-2 focus:ring-theme-primary focus:ring-opacity-50 p-2 -ml-2" title="Click to copy the full ID">
                    <span className="text-sm font-mono text-theme-light truncate block">{truncatedUserId}</span>
                    {copyFeedback && <p className="text-xs text-theme-success font-semibold mt-1">{copyFeedback}</p>}
                </button>
            </div>

            {/* Guthaben-Übersicht */}
            <section className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-8">
                {Object.keys(balances).length > 0 ? (
                    Object.entries(balances).map(([currency, amount]) => (
                        <div key={currency} className="bg-card shadow-lg rounded-xl p-6 text-center border border-theme-subtle">
                            <p className="text-4xl font-bold text-theme-primary">{amount}</p>
                            <p className="text-lg text-theme-light mt-1">{currency}</p>
                        </div>
                    ))
                ) : (
                    <div className="md:col-span-2 bg-card shadow-lg rounded-xl p-6 text-center border border-theme-subtle">
                        <p className="text-2xl font-bold text-theme-light">No balance available</p>
                    </div>
                )}
            </section>

            {/* Hauptaktionen */}
            <section className="flex justify-center gap-6 mb-8">
                <Button className="flex-1">Send</Button>
                <Button className="flex-1">Receive</Button>
                <Button onClick={onNavigateToCreateVoucher} className="flex-1" variant="outline">Create Voucher</Button>
            </section>

            {/* Gutschein-Liste */}
            <section>
                <h2 className="text-2xl font-semibold mb-4 text-theme-secondary">My Vouchers</h2>
                <div className="space-y-4">
                    {vouchers.length > 0 ? vouchers.map(v => {
                        const status = getVoucherStatus(v.status);
                        return (
                            <button 
                                key={v.local_instance_id} 
                                onClick={() => onShowDetails(v.local_instance_id)}
                                className="w-full text-left bg-card rounded-lg border border-theme-subtle shadow-sm p-4 space-y-3 transition-all duration-200 ease-in-out hover:shadow-md hover:border-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary focus:ring-opacity-50"
                            >
                                {/* Header: Amount and Status */}
                                <div className="flex justify-between items-start">
                                    <div>
                                        <div>
                                            <p className="text-2xl font-bold text-theme-primary">{v.current_amount} <span className="text-lg font-normal text-theme-light">{v.unit}</span></p>
                                            <p className="text-sm text-theme-light -mt-1">{v.voucher_standard_name}</p>
                                        </div>
                                        <p className="text-xs text-theme-light font-mono">by {v.creator_first_name} {v.creator_last_name} {v.creator_id ? `(${v.creator_id.substring(0, 25)}...${v.creator_id.substring(v.creator_id.length - 5)})` : ''}</p>
                                    </div>
                                    <div className="flex items-center gap-2">
                                        {/* Indicator for "Own" or "External" */}
                                        {v.creator_id && userId && (
                                            v.creator_id.includes(userId) ? (
                                                <span className="px-2 py-1 text-xs font-bold rounded-full text-sky-800 bg-sky-200">Own</span>
                                            ) : (
                                                <span className="px-2 py-1 text-xs font-bold rounded-full text-yellow-800 bg-yellow-200">External</span>
                                            )
                                        )}
                                        <span className={`px-2 py-1 text-xs font-bold rounded-full capitalize ${status.color}`}>
                                            {status.name}
                                        </span>
                                    </div>
                                </div>

                                {/* Body: Description */}
                                <p className="text-sm text-theme-secondary">{truncate(v.description, 120)}</p>

                                {/* Footer: Validity and Indicators */}
                                <div
                                    className="flex justify-between items-center text-xs text-theme-light border-t border-theme-subtle pt-2">
                                    <p>Valid until: <span className="font-semibold">{formatDate(v.valid_until)}</span>
                                    </p>
                                    <div className="flex items-center space-x-3">
                                        {v.has_collateral && <span title="Has Collateral">🛡️</span>}
                                        <span title="Guarantor Signatures">✍️ {v.guarantor_signatures_count}</span>
                                        <span title="Additional Signatures">➕ {v.additional_signatures_count}</span>
                                    </div>
                                </div>
                            </button>
                        );
                    }) : (
                        <div className="text-center text-theme-light py-8 bg-card rounded-lg border border-theme-subtle">
                            <p>No vouchers found.</p>
                        </div>
                    )}
                </div>
            </section>
        </div>
    );
}