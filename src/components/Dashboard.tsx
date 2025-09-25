// src/components/Dashboard.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { AggregatedBalance, VoucherSummary } from "../types";

interface DashboardProps {
    onNavigateToCreateVoucher: () => void;
    onShowDetails: (voucherId: string) => void;
}

export function Dashboard({ onNavigateToCreateVoucher, onShowDetails }: DashboardProps) {
    const [userId, setUserId] = useState("");
    const [balances, setBalances] = useState<AggregatedBalance[]>([]);
    const [vouchers, setVouchers] = useState<VoucherSummary[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [copied, setCopied] = useState(false);

    useEffect(() => {
        logger.info("Dashboard component displayed");
        async function fetchData() {
            try {
                const [id, balanceList, voucherList] = await Promise.all([
                    invoke<string>("get_user_id"),
                    invoke<AggregatedBalance[]>("get_total_balance_by_currency"),
                    invoke<VoucherSummary[]>("get_voucher_summaries")
                ]);
                setUserId(id);
                setBalances(balanceList);
                setVouchers(voucherList);
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
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        } catch (e) {
            const msg = `Failed to copy User ID: ${e}`;
            console.error(msg);
            setFeedbackMsg(`Error: ${msg}`);
        }
    }

    function getVoucherStatus(status: object): { name: string; color: string; tooltip: string } {
        const statusName = (typeof status === 'string' ? status : Object.keys(status)[0])?.toLowerCase() || 'unknown';
        let color = 'text-gray-800 bg-gray-200';
        let tooltip = '';

        switch (statusName) {
            case 'active':
                color = 'text-green-800 bg-green-200';
                tooltip = 'This voucher is active and ready to be used or transferred.';
                break;
            case 'quarantined':
                color = 'text-red-800 bg-red-200';
                tooltip = 'This voucher has been identified as an illegal copy (double-spend) and is no longer usable.';
                break;
            case 'archived':
                color = 'text-indigo-800 bg-indigo-200';
                tooltip = 'This voucher has been fully used or archived. It cannot be used for new transfers.';
                break;
            case 'incomplete':
                color = 'text-gray-800 bg-gray-200';
                tooltip = 'This voucher cannot be used yet because not all conditions have been met (e.g., missing guarantor signatures).';
                break;
            default:
                tooltip = `Status: ${statusName}`;
                break;
        }
        return { name: statusName, color: color, tooltip: tooltip };
    }

    function formatDate(isoString: string): string {
        if (!isoString) return 'N/A';
        return new Date(isoString).toLocaleDateString(undefined, {
            year: 'numeric', month: 'short', day: 'numeric'
        });
    }


    function truncate(text: string, length: number): string {
        if (text.length <= length) return text;
        return text.substring(0, length) + '...';
    }


    function formatAmount(amountStr: string): string {
        const num = parseFloat(amountStr);
        if (isNaN(num)) return amountStr;
        return num.toString();
    }

    const truncatedUserId = userId ? `${userId.substring(0, 15)}...${userId.substring(userId.length - 8)}` : "Lade...";

    return (
        <div className="mx-auto max-w-4xl p-4 sm:p-6">
            {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}

            {/* User ID Anzeige */}
            <div className="mb-8 flex h-12 items-center justify-between gap-4 rounded-full bg-input-readonly px-4 shadow-sm border border-theme-subtle">
                <div className="flex items-center gap-3 overflow-hidden">
                    <span className="text-sm font-bold text-theme-secondary flex-shrink-0">User ID</span>
                    <span className="text-sm font-mono text-theme-light bg-card px-3 py-1 rounded-full truncate">
                        {truncatedUserId}
                    </span>
                </div>
                <button onClick={handleCopyUserId} title="Copy User ID" className="p-2 rounded-full hover:bg-card focus:outline-none focus:ring-2 focus:ring-theme-accent flex-shrink-0">
                    {copied ? (
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5 text-theme-success" viewBox="0 0 20 20" fill="currentColor">
                            <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                        </svg>
                    ) : (
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5 text-theme-light" viewBox="0 0 20 20" fill="currentColor">
                            <path d="M7 9a2 2 0 012-2h6a2 2 0 012 2v6a2 2 0 01-2 2H9a2 2 0 01-2-2V9z" />
                            <path d="M5 3a2 2 0 00-2 2v6a2 2 0 002 2V5h6a2 2 0 00-2-2H5z" />
                        </svg>
                    )}
                </button>
            </div>

            {/* Guthaben-Übersicht */}
            <section className="grid grid-cols-[repeat(auto-fit,minmax(18rem,1fr))] gap-6 mb-8">
                {balances.length > 0 ? (
                    balances.map((balance) => (
                        <div key={balance.standard_uuid} className="bg-input-readonly shadow-lg rounded-lg p-4 text-center border border-theme-subtle">
                            <p className="text-base font-semibold text-theme-light">{balance.standard_name}</p>
                            <p className="text-3xl font-bold text-theme-primary mt-1">{formatAmount(balance.total_amount)} <span className="text-xl font-normal">{balance.unit}</span></p>
                        </div>
                    ))
                ) : (
                    <div className="md:col-span-2 bg-input-readonly shadow-lg rounded-xl p-6 text-center border border-theme-subtle">
                        <p className="text-lg text-theme-light">No balance available</p>
                    </div>
                )}
            </section>

            {/* Hauptaktionen */}
            <section className="flex justify-center gap-6 mb-8">
                <Button className="flex-1">Send</Button>
                <Button className="flex-1">Receive</Button>
                <Button onClick={onNavigateToCreateVoucher} className="flex-1">Create Voucher</Button>
            </section>

            {/* Gutschein-Liste */}
            <section>
                <h2 className="text-2xl font-semibold mb-4 text-theme-secondary">My Vouchers</h2>
                <div className="space-y-4">
                    {vouchers.length > 0 ? vouchers.map(v => {
                        const status = getVoucherStatus(v.status);
                        return (
                            <div key={v.local_instance_id} className="relative">
                                {v.non_redeemable_test_voucher && (
                                    <div className="absolute inset-0 flex items-center justify-center pointer-events-none z-20">
                                        <span className="text-[90px] font-bold text-gray-800/20 transform -rotate-12 select-none pointer-events-none">TEST</span>
                                    </div>
                                )}
                                <button
                                    onClick={() => onShowDetails(v.local_instance_id)}
                                    className="w-full text-left bg-bg-card-alternate rounded-lg border border-theme-subtle shadow-sm p-4 space-y-3 transition-all duration-200 ease-in-out hover:shadow-md hover:border-theme-primary focus:outline-none focus:ring-2 focus:ring-theme-primary focus:ring-opacity-50 relative z-10"
                                >
                                    {/* Header: Amount and Voucher Name */}
                                    <div className="flex justify-between items-start">
                                        <div>
                                            <div className="flex items-baseline text-2xl font-bold text-theme-primary">
                                                <span className="inline-block min-w-[4rem] text-right">{formatAmount(v.current_amount)}</span>
                                                <span className="ml-2 text-lg font-normal text-theme-light">{v.unit}</span>
                                            </div>
                                            <p className="text-xs text-theme-light font-mono">by {v.creator_first_name} {v.creator_last_name}</p>
                                        </div>
                                        <div className="text-right">
                                            <p className="text-xl font-normal text-theme-light">{v.voucher_standard_name}</p>
                                        </div>
                                    </div>

                                    {/* Body: Description */}
                                    <p className="text-sm text-theme-secondary">{truncate(v.description, 120)}</p>

                                    {/* Footer: Validity and Indicators */}
                                    <div className="border-t border-theme-subtle pt-2">
                                        <div className="flex justify-between items-center text-xs text-theme-light">
                                            <p>Valid until: <span className="font-semibold">{formatDate(v.valid_until)}</span></p>
                                            <div className="flex items-center space-x-3 text-sm">
                                                {v.has_collateral && <span title="Has Collateral">🛡️</span>}
                                                <span title="Guarantor Signatures">✍️ {v.guarantor_signatures_count}</span>
                                                <span title="Additional Signatures">➕ {v.additional_signatures_count}</span>
                                            </div>
                                            <div className="flex items-center gap-2">
                                                <span className={`px-2 py-1 text-xs font-bold rounded-full capitalize ${status.color}`} title={status.tooltip}>
                                                    {status.name}
                                                </span>
                                                {v.non_redeemable_test_voucher && (
                                                    <span className="px-3 py-1 text-xs font-bold rounded-full text-purple-800 bg-purple-200" title="Non-redeemable test voucher">Test</span>
                                                )}
                                            </div>
                                        </div>
                                    </div>
                                </button>
                            </div>
                        );
                    }) : (
                        <div className="text-center text-theme-light py-8 bg-input-readonly rounded-lg border border-theme-subtle">
                            <p>No vouchers found.</p>
                        </div>
                    )}
                </div>
            </section>
        </div>
    );
}