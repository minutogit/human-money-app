// src/components/Dashboard.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { info, error } from "@tauri-apps/plugin-log";
import { Button } from "./ui/Button";
import { VoucherSummary } from "../types";

interface DashboardProps {
    onLogout: () => void;
}

export function Dashboard({ onLogout }: DashboardProps) {
    const [userId, setUserId] = useState("");
    const [balances, setBalances] = useState<Record<string, string>>({});
    const [vouchers, setVouchers] = useState<VoucherSummary[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState("");

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

    async function handleLogout() {
        try {
            await invoke("logout");
            info("User logged out successfully.");
            onLogout();
        } catch (e) {
            const msg = `Logout failed: ${e}`;
            error(msg);
            setFeedbackMsg(`Error: ${msg}`);
        }
    }

    const truncatedUserId = userId ? `${userId.substring(0, 15)}...${userId.substring(userId.length - 8)}` : "Loading...";

    return (
        <div className="flex flex-col min-h-screen bg-bg-app p-4 font-sans text-theme-secondary">
            {/* Header */}
            <header className="w-full max-w-4xl mx-auto flex justify-between items-center pb-4 mb-4 border-b border-theme-subtle">
                <h1 className="text-2xl font-bold text-theme-primary">Voucher Wallet</h1>
                <div className="flex items-center space-x-4">
                    <span className="text-sm font-mono" title={userId}>{truncatedUserId}</span>
                    <button onClick={handleLogout} className="px-4 py-2 text-sm font-semibold text-theme-error border border-theme-error rounded-md hover:bg-theme-error hover:text-white transition-colors duration-150 ease-in-out">
                        Logout
                    </button>
                </div>
            </header>

            <main className="w-full max-w-4xl mx-auto flex-grow">
                {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}
                
                {/* Balance Summary */}
                <section className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-8">
                    {Object.entries(balances).map(([currency, amount]) => (
                        <div key={currency} className="bg-bg-card shadow-lg rounded-xl p-6 text-center border border-theme-subtle">
                            <p className="text-4xl font-bold text-theme-primary">{amount}</p>
                            <p className="text-lg text-theme-light mt-1">{currency}</p>
                        </div>
                    ))}
                </section>

                {/* Primary Actions */}
                <section className="flex justify-center gap-6 mb-8">
                    <Button className="flex-1">Send</Button>
                    <Button className="flex-1">Receive</Button>
                </section>

                {/* Voucher List */}
                <section>
                    <h2 className="text-2xl font-semibold mb-4">My Vouchers</h2>
                    <div className="space-y-3 bg-bg-card shadow-lg rounded-xl p-4 border border-theme-subtle">
                        {vouchers.length > 0 ? vouchers.map(v => (
                            <div key={v.local_id} className="flex justify-between items-center p-3 border-b border-theme-subtle last:border-b-0">
                                <div>
                                    <p className="font-mono font-semibold">{v.amount} {v.currency}</p>
                                    <p className="text-xs text-theme-light font-mono">ID: {v.local_id}</p>
                                </div>
                                <span className="px-2 py-1 text-xs font-bold text-green-800 bg-green-200 rounded-full">{v.status}</span>
                            </div>
                        )) : <p className="text-center text-theme-light py-4">No vouchers found.</p>}
                    </div>
                </section>
            </main>
        </div>
    );
}
