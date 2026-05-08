// src/components/Dashboard.tsx
import { useState, useEffect } from "react";
import { voucherService, utilityService } from "../services/voucherService";
import { transferService } from "../services/transferService";
import { profileService } from "../services/profileService";
import { contactService } from "../services/contactService";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { AggregatedBalance, VoucherSummary, WalletEvent } from "../types";
import { useSession } from "../context/SessionContext";
import { IntegrityReportModal } from "./IntegrityReportModal";
import { PageLayout } from "./ui/PageLayout";
import { 
    Plus, 
    ArrowUpRight, 
    ArrowDownLeft, 
    ShieldAlert, 
    UserCircle,
    Copy,
    CheckCircle2,
    Clock,
    AlertCircle
} from "lucide-react";
import { truncateUserId } from "../utils/userIdHelper";

import { useNavigation } from "../context/NavigationContext";

export function Dashboard() {
    const { navigate } = useNavigation();
    const { profileName, integrityReport } = useSession();

    const [userId, setUserId] = useState("");
    const [balances, setBalances] = useState<AggregatedBalance[]>([]);
    const [recentEvents, setRecentEvents] = useState<WalletEvent[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [copied, setCopied] = useState(false);
    const [activeVouchersCount, setActiveVouchersCount] = useState(0);
    const [incompleteCount, setIncompleteCount] = useState(0);
    const [quarantinedCount, setQuarantinedCount] = useState(0);
    const [isLoading, setIsLoading] = useState(true);
    const [isProfileComplete, setIsProfileComplete] = useState(true);
    const [voucherCountsByStandard, setVoucherCountsByStandard] = useState<Record<string, number>>({});
    const [showIntegrityModal, setShowIntegrityModal] = useState(false);

    useEffect(() => {
        logger.info("Dashboard component displayed");
        async function fetchData() {
            try {
                const [id, balanceList, , voucherSummaries, userProfile, , eventHistory] = await Promise.all([
                    utilityService.getUserId(),
                    voucherService.getTotalBalanceByCurrency(),
                    transferService.getHistory().catch(() => []),
                    voucherService.getSummaries().catch(() => []),
                    profileService.getProfile().catch(() => null),
                    contactService.getContacts().catch(() => []),
                    transferService.getEventHistory(0, 5).catch(() => [])
                ]);
                setUserId(id);
                setBalances(balanceList || []);
                setRecentEvents(eventHistory || []);
                
                const activeCount = (voucherSummaries || []).filter((v: VoucherSummary) => v.status === "active").length;
                const incompleteCount = (voucherSummaries || []).filter((v: VoucherSummary) => v.status === "incomplete").length;
                const quarantinedCount = (voucherSummaries || []).filter((v: VoucherSummary) => v.status === "quarantined").length;

                const countsByStandard: Record<string, number> = {};
                (voucherSummaries || [])
                    .filter((v: VoucherSummary) => v.status === "active")
                    .forEach((v: VoucherSummary) => {
                        const standardUuid = v.voucherStandardUuid;
                        if (standardUuid) {
                            countsByStandard[standardUuid] = (countsByStandard[standardUuid] || 0) + 1;
                        }
                    });
                setVoucherCountsByStandard(countsByStandard);

                setActiveVouchersCount(activeCount);
                setIncompleteCount(incompleteCount);
                setQuarantinedCount(quarantinedCount);

                const profileComplete = userProfile && userProfile.firstName && userProfile.lastName && userProfile.address?.city;
                setIsProfileComplete(!!profileComplete);
            } catch (e) {
                const msg = `Failed to fetch dashboard data: ${e}`;
                console.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
            } finally {
                setIsLoading(false);
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

    function formatAmount(amountStr: string): string {
        const num = parseFloat(amountStr);
        if (isNaN(num)) return amountStr;
        return num.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    }

    function formatTimestamp(isoString: string): string {
        return new Date(isoString).toLocaleDateString(undefined, {
            month: 'short',
            day: 'numeric'
        });
    }

    function getEventDetails(event: WalletEvent): { label: string; icon: React.ElementType; color: string; bgColor: string } {
        const type = event.eventType;
        const bff = event.bffData;

        if (typeof type === 'string') {
            switch (type) {
                case 'voucherCreated':
                    return { label: 'Voucher Created', icon: Plus, color: 'text-blue-600', bgColor: 'bg-blue-50' };
                case 'transferSent':
                    return { label: `Sent to ${bff.counterpartyName || 'Anonymous'}`, icon: ArrowUpRight, color: 'text-rose-600', bgColor: 'bg-rose-50' };
                case 'transferReceived':
                    return { label: `Received from ${bff.counterpartyName || 'Anonymous'}`, icon: ArrowDownLeft, color: 'text-emerald-600', bgColor: 'bg-emerald-50' };
                case 'voucherQuarantined':
                    return { label: 'Voucher Quarantined', icon: AlertCircle, color: 'text-amber-600', bgColor: 'bg-amber-50' };
                case 'voucherActivated':
                    return { label: 'Voucher Activated', icon: CheckCircle2, color: 'text-emerald-600', bgColor: 'bg-emerald-50' };
                case 'voucherVoided':
                    return { label: 'Voucher Voided', icon: AlertCircle, color: 'text-gray-600', bgColor: 'bg-gray-50' };
                case 'voucherExpired':
                    return { label: 'Voucher Expired', icon: Clock, color: 'text-orange-600', bgColor: 'bg-orange-50' };
            }
        }
        return { label: 'Unknown Event', icon: AlertCircle, color: 'text-gray-600', bgColor: 'bg-gray-50' };
    }
    const truncatedUserId = userId ? truncateUserId(userId) : "Lade...";

    return (
        <PageLayout 
            customHeader={
                <header className="flex-shrink-0 sticky top-0 z-30 px-6 py-2 glass-panel border-b-0 rounded-b-3xl mb-4">
                    <div className="flex items-center justify-between gap-3 text-[10px] sm:text-xs font-bold text-theme-light">
                        <div className="flex items-center gap-3">
                            <span className="relative flex h-2 w-2">
                                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                                <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                            </span>
                            <div className="flex items-center gap-1.5 px-2 py-1 bg-white/50 rounded-full border border-theme-subtle/30 shadow-inner-soft">
                                <UserCircle size={12} className="text-theme-primary" />
                                <span className="text-theme-secondary">{profileName}</span>
                            </div>
                        </div>
                        
                        <div className="flex items-center gap-2">
                            <button 
                                onClick={handleCopyUserId}
                                className={`flex items-center gap-2 px-3 py-1.5 rounded-full transition-all duration-300 border ${
                                    copied 
                                    ? 'bg-emerald-50 text-emerald-600 border-emerald-200' 
                                    : 'bg-white/50 text-theme-secondary border-theme-subtle/50 hover:border-theme-primary hover:bg-white hover:shadow-sm'
                                }`}
                            >
                                <span className="font-mono text-[10px]">{copied ? "Copied!" : truncatedUserId}</span>
                                {copied ? <CheckCircle2 size={12} /> : <Copy size={12} className="opacity-50" />}
                            </button>
                        </div>
                    </div>
                </header>
            }
        >
            <div className="max-w-5xl mx-auto space-y-8">
                {/* Welcome Message */}
                {balances.length === 0 && (
                    <div className="px-2 pt-4 text-center">
                        <h1 className="text-3xl font-black text-theme-primary tracking-tighter">👋 Welcome to your Wallet!</h1>
                        <p className="text-sm font-medium text-theme-light mt-1">Manage your community vouchers and local currency.</p>
                    </div>
                )}

                {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}

                {/* Warning Banners Section */}
                <div className="space-y-3">
                    {integrityReport && integrityReport.type !== 'valid' && (
                        <Card 
                            variant="glass" 
                            className={`py-2 px-4 flex items-center justify-between cursor-pointer border-l-4 ${
                                integrityReport.type === 'unknownItems' || integrityReport.type === 'missingIntegrityRecord'
                                ? 'border-l-blue-500' : 'border-l-red-500'
                            }`}
                            onClick={() => setShowIntegrityModal(true)}
                            hover
                        >
                            <div className="flex items-center gap-4">
                                <div className={`p-2 rounded-xl ${
                                    integrityReport.type === 'unknownItems' || integrityReport.type === 'missingIntegrityRecord'
                                    ? 'bg-blue-500 text-white' : 'bg-red-500 text-white'
                                }`}>
                                    <ShieldAlert size={20} />
                                </div>
                                <div>
                                    <p className="font-bold text-sm text-theme-secondary">Security Warning</p>
                                    <p className="text-xs opacity-70">Action required to repair your wallet.</p>
                                </div>
                            </div>
                            <ArrowUpRight size={18} className="opacity-30" />
                        </Card>
                    )}

                    {!isProfileComplete && (
                        <Card 
                            variant="glass" 
                            className="py-2 px-4 flex items-center justify-between border-l-4 border-l-amber-500"
                            hover
                            onClick={() => navigate({ view: 'settings' })}
                        >
                            <div className="flex items-center gap-4">
                                <div className="p-2 rounded-xl bg-amber-500 text-white">
                                    <UserCircle size={20} />
                                </div>
                                <div>
                                    <p className="font-bold text-sm text-theme-secondary">Profile Incomplete</p>
                                    <p className="text-xs opacity-70">Building trust requires a complete identity.</p>
                                </div>
                            </div>
                            <Button variant="primary" size="xs" className="shadow-none">Fix Now</Button>
                        </Card>
                    )}

                    {quarantinedCount > 0 && (
                        <Card 
                            variant="glass" 
                            className="py-2 px-4 flex items-center justify-between border-l-4 border-l-rose-500"
                            hover
                            onClick={() => navigate({ view: 'conflict_list' })}
                        >
                            <div className="flex items-center gap-4">
                                <div className="p-2 rounded-xl bg-rose-500 text-white">
                                    <AlertCircle size={20} />
                                </div>
                                <div>
                                    <p className="font-bold text-sm text-theme-secondary">Quarantine: {quarantinedCount} Vouchers Locked</p>
                                    <p className="text-xs opacity-70">Double-spend detected. Resolve conflicts now.</p>
                                </div>
                            </div>
                            <ArrowUpRight size={18} className="opacity-30" />
                        </Card>
                    )}
                </div>

                {/* Zone 2: Balances (Wallet Cards) */}
                <section>
                    <div className="flex items-center justify-between mb-4 px-2">
                        <h2 className="text-xs font-black text-theme-light uppercase tracking-[0.2em]">Your Balances</h2>
                    </div>
                    
                    {isLoading ? (
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                            {[1, 2].map(i => <div key={i} className="h-48 bg-white/50 animate-pulse rounded-3xl border border-theme-subtle/50"></div>)}
                        </div>
                    ) : (
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                            {balances.map((balance, idx) => {
                                const count = voucherCountsByStandard[balance.standardUuid] || 0;
                                return (
                                    <Card 
                                        key={balance.standardUuid}
                                        variant="default"
                                        className={`relative overflow-hidden p-6 h-48 flex flex-col justify-between group ${
                                            idx % 2 === 0 
                                            ? 'bg-gradient-to-br from-slate-500 to-slate-700' 
                                            : 'bg-gradient-to-br from-theme-primary to-theme-accent'
                                        }`}
                                        hover
                                        onClick={() => navigate({ view: 'wallet', initialStandardFilter: balance.displayStandardName, initialStatusFilter: 'active' })}
                                    >
                                        {/* Decorative Circles */}
                                        <div className="absolute -right-8 -top-8 w-32 h-32 bg-white/10 rounded-full blur-2xl group-hover:bg-white/20 transition-all duration-500"></div>
                                        <div className="absolute -left-4 -bottom-4 w-24 h-24 bg-black/5 rounded-full blur-xl"></div>
                                        
                                        <div className="relative z-10 flex justify-between items-start">
                                            <div className="flex flex-col">
                                                <span className="text-[10px] font-black text-white/60 uppercase tracking-[0.15em] mb-1">
                                                    {balance.displayStandardName}
                                                </span>
                                                <h3 className="text-xl font-bold text-white leading-tight">
                                                    {balance.displayCurrency}
                                                </h3>
                                            </div>
                                            <div className="px-2 py-1 bg-white/20 backdrop-blur-md rounded-lg border border-white/20 text-[10px] font-bold text-white">
                                                {count} Vouchers
                                            </div>
                                        </div>

                                        <div className="relative z-10 flex items-baseline gap-2">
                                            <span className="text-4xl font-black text-white tracking-tighter">
                                                {formatAmount(balance.totalAmount)}
                                            </span>
                                        </div>

                                        <div className="relative z-10 flex justify-end">
                                            <div className="text-[10px] font-bold text-white/40 uppercase tracking-widest group-hover:text-white transition-colors">
                                                View Details ➔
                                            </div>
                                        </div>
                                    </Card>
                                );
                            })}
                            
                            {/* Empty State / Add More Placeholder */}
                            {balances.length < 2 && (
                                <Card 
                                    variant="none" 
                                    className="border-2 border-dashed border-theme-subtle bg-transparent p-6 h-48 flex flex-col items-center justify-center text-theme-placeholder hover:border-theme-primary hover:text-theme-primary transition-all group"
                                    hover
                                    onClick={() => navigate({ view: 'create_voucher' })}
                                >
                                    <Plus size={32} className="mb-2 opacity-50 group-hover:scale-110 transition-transform" />
                                    <span className="text-sm font-bold uppercase tracking-widest">Create New Voucher</span>
                                </Card>
                            )}
                        </div>
                    )}
                </section>

                {/* Zone 3: Quick Actions */}
                <section className="flex flex-wrap justify-center gap-4">
                    <Button 
                        onClick={() => navigate({ view: 'send_vouchers' })} 
                        className="min-w-[140px] gap-2 shadow-premium"
                        size="lg"
                        disabled={activeVouchersCount === 0}
                    >
                        <ArrowUpRight size={20} />
                        Send
                    </Button>
                    <Button 
                        onClick={() => navigate({ view: 'receive_bundle' })} 
                        className="min-w-[140px] gap-2 shadow-premium"
                        variant="secondary"
                        size="lg"
                    >
                        <ArrowDownLeft size={20} />
                        Receive
                    </Button>
                    <Button 
                        onClick={() => navigate({ view: 'create_voucher' })}
                        className="min-w-[140px] gap-2 shadow-premium"
                        variant="secondary"
                        size="lg"
                    >
                        <Plus size={20} />
                        Create
                    </Button>
                </section>

                {/* Zone 4: Tasks / Pending */}
                {incompleteCount > 0 && (
                    <section>
                        <Card 
                            variant="accent" 
                            className="p-5 flex items-center justify-between border-l-4 border-l-theme-accent"
                            hover
                            onClick={() => navigate({ view: 'wallet', initialStatusFilter: 'incomplete' })}
                        >
                            <div className="flex items-center gap-4">
                                <div className="p-3 rounded-2xl bg-theme-accent text-white shadow-md">
                                    <Clock size={24} />
                                </div>
                                <div>
                                    <p className="font-black text-theme-secondary uppercase tracking-wider text-xs">Action Required</p>
                                    <p className="text-lg font-bold text-theme-primary">{incompleteCount} Vouchers need signatures</p>
                                </div>
                            </div>
                            <Button variant="primary" size="sm" className="rounded-full px-6">View</Button>
                        </Card>
                    </section>
                )}

                {/* Zone 5: Recent Activity */}
                <section>
                    <div className="flex items-center justify-between mb-6 px-2">
                        <h2 className="text-xs font-black text-theme-light uppercase tracking-[0.2em]">Recent Activity</h2>
                        <button 
                            onClick={() => navigate({ view: 'activities' })}
                            className="text-[10px] font-black text-theme-accent uppercase tracking-widest hover:underline"
                        >
                            History ➔
                        </button>
                    </div>

                    <div className="space-y-3">
                        {recentEvents.length > 0 ? recentEvents.map(event => {
                            const { label, icon: Icon, color, bgColor } = getEventDetails(event);
                            return (
                                <Card 
                                    key={event.eventId}
                                    variant="default"
                                    className="p-4 flex items-center justify-between group"
                                    hover
                                    onClick={() => {
                                        const type = event.eventType;
                                        if (type === 'transferSent' || type === 'transferReceived') {
                                            navigate({ view: 'transaction_history' });
                                        } else {
                                            navigate({ view: 'voucher_details', voucherId: event.localInstanceId, previousView: { view: 'logged_in' } });
                                        }
                                    }}
                                >
                                    <div className="flex items-center gap-4">
                                        <div className={`p-3 rounded-2xl ${bgColor} ${color} transition-transform group-hover:scale-110`}>
                                            <Icon size={20} />
                                        </div>
                                        <div>
                                            <p className="font-bold text-sm text-theme-secondary">{label}</p>
                                            <p className="text-[10px] font-bold text-theme-light uppercase tracking-widest mt-0.5">
                                                {event.bffData.displayCurrency && `${event.bffData.amount} ${event.bffData.displayCurrency}`}
                                            </p>
                                        </div>
                                    </div>
                                    <div className="text-right">
                                        <span className="text-[10px] font-bold text-theme-placeholder uppercase tracking-widest">
                                            {formatTimestamp(event.timestamp)}
                                        </span>
                                    </div>
                                </Card>
                            );
                        }) : (
                            <div className="text-center py-12 px-6">
                                <Card variant="none" className="border-2 border-dashed border-theme-subtle p-8 opacity-50">
                                    <p className="text-sm font-bold uppercase tracking-widest text-theme-placeholder">No recent activity</p>
                                </Card>
                            </div>
                        )}
                    </div>
                </section>
            </div>

            {showIntegrityModal && integrityReport && (
                <IntegrityReportModal 
                    report={integrityReport} 
                    onClose={() => setShowIntegrityModal(false)} 
                />
            )}
        </PageLayout>
    );
}
