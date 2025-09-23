// src/components/Dashboard.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Button } from "./ui/Button";
import { VoucherSummary } from "../types";

interface DashboardProps {
    onNavigateToCreateVoucher: () => void;
}

export function Dashboard({ onNavigateToCreateVoucher }: DashboardProps) {
    const [userId, setUserId] = useState("");
    const [balances, setBalances] = useState<Record<string, string>>({});
    const [vouchers, setVouchers] = useState<VoucherSummary[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [copyFeedback, setCopyFeedback] = useState("");

    useEffect(() => {
        async function fetchData() {
            try {
                info("Dashboard: Fetching data from backend.");
                const [id, balanceMap, voucherList] = await Promise.all([
                    invoke<string>("get_user_id"),
                    invoke<Record<string, string>>("get_total_balance_by_currency"),
                    invoke<VoucherSummary[]>("get_voucher_summaries")
                ]);
                setUserId(id);
                setBalances(balanceMap);
                setVouchers(voucherList);
                info("Dashboard: Data successfully fetched.");
            } catch (e) {
                const msg = `Failed to fetch dashboard data: ${e}`;
                error(msg);
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
            error(msg);
            setFeedbackMsg(`Error: ${msg}`);
        }
    }

    // Helper function to safely render the voucher status
    function renderVoucherStatus(status: string | object): string {
        if (typeof status === 'string') {
            return status;
        }
        if (typeof status === 'object' && status !== null) {
            // Returns the key of the status object, e.g., "Incomplete"
            return Object.keys(status)[0] || 'unknown';
        }
        return 'unknown';
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
                <div className="space-y-3 bg-card shadow-lg rounded-xl p-4 border border-theme-subtle">
                    {vouchers.length > 0 ? vouchers.map(v => (
                        <div key={v.local_id} className="flex justify-between items-center p-3 border-b border-theme-subtle last:border-b-0">
                            <div>
                                <p className="font-mono font-semibold text-theme-secondary">{v.amount} {v.currency}</p>
                                <p className="text-xs text-theme-light font-mono">ID: {v.local_id}</p>
                            </div>
                            <span className={`px-2 py-1 text-xs font-bold rounded-full capitalize ${renderVoucherStatus(v.status) === 'active' ? 'text-green-800 bg-green-200' : 'text-gray-800 bg-gray-200'}`}>
                                {renderVoucherStatus(v.status)}
                            </span>
                        </div>
                    )) : <p className="text-center text-theme-light py-4">No vouchers found.</p>}
                </div>
            </section>
        </div>
    );
}