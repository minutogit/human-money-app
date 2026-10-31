// src/components/Dashboard.tsx
import { useState, useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { voucherService } from "../services/voucherService";
import { transferService } from "../services/transferService";
import { profileService } from "../services/profileService";
import { contactService } from "../services/contactService";
import { settingsService } from "../services/settingsService";
import { copyToClipboard } from "../utils/clipboard";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { AggregatedBalance, VoucherSummary, WalletEvent, AppSettings } from "../types";
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
    AlertCircle,
    Archive,
    Info
} from "lucide-react";
import { truncateUserId } from "../utils/userIdHelper";
import { stringifyError } from "../utils/errorHelper";

import { useNavigation } from "../context/NavigationContext";
import { useContactResolver } from "../hooks/useContactResolver";

export function Dashboard() {
    const { t } = useTranslation();
    const { navigate } = useNavigation();
    const { profileName, integrityReport } = useSession();
    const { resolveIdentity } = useContactResolver();

    const [userId, setUserId] = useState("");
    const [balances, setBalances] = useState<AggregatedBalance[]>([]);
    const [recentEvents, setRecentEvents] = useState<WalletEvent[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [copied, setCopied] = useState(false);
    const [activeVouchersCount, setActiveVouchersCount] = useState(0);
    const [incompleteCount, setIncompleteCount] = useState(0);
    const [quarantinedCount, setQuarantinedCount] = useState(0);
    const [expiringSoonCount, setExpiringSoonCount] = useState(0);
    const [isLoading, setIsLoading] = useState(true);
    const [voucherCountsByStandard, setVoucherCountsByStandard] = useState<Record<string, number>>({});
    const [showIntegrityModal, setShowIntegrityModal] = useState(false);
    const [showBackupReminder, setShowBackupReminder] = useState(false);
    const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
    const [balanceViewMode, setBalanceViewMode] = useState<'spendable' | 'net'>('spendable');
    const [activeInfoPopover, setActiveInfoPopover] = useState<string | null>(null);
    const [popoverAnchorRect, setPopoverAnchorRect] = useState<DOMRect | null>(null);
    const popoverRef = useRef<HTMLDivElement | null>(null);
    const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    useEffect(() => {
        return () => {
            if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
        };
    }, []);

    useEffect(() => {
        if (!activeInfoPopover) return;

        const handleScrollOrResize = () => {
            if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
            setActiveInfoPopover(null);
            setPopoverAnchorRect(null);
        };

        const handlePointerDown = (e: MouseEvent | TouchEvent) => {
            const isTrigger = (e.target as Element)?.closest?.('[data-popover-trigger]');
            if (isTrigger) return;
            if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
                if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
                setActiveInfoPopover(null);
                setPopoverAnchorRect(null);
            }
        };

        window.addEventListener('scroll', handleScrollOrResize, true);
        window.addEventListener('resize', handleScrollOrResize);
        document.addEventListener('pointerdown', handlePointerDown);

        return () => {
            window.removeEventListener('scroll', handleScrollOrResize, true);
            window.removeEventListener('resize', handleScrollOrResize);
            document.removeEventListener('pointerdown', handlePointerDown);
        };
    }, [activeInfoPopover]);

    useEffect(() => {
        logger.info("Dashboard component displayed");
        async function fetchData() {
            try {
                const [id, balanceList, , voucherSummaries, , , eventHistory, userSettings] = await Promise.all([
                    profileService.getUserId(),
                    voucherService.getTotalBalanceByCurrency(),
                    transferService.getHistory().catch(() => []),
                    voucherService.getSummaries().catch(() => []),
                    profileService.getProfile().catch(() => null),
                    contactService.getContacts().catch(() => []),
                    transferService.getEventHistory(0, 5).catch(() => []),
                    settingsService.getSettings().catch(() => null)
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

                const now = new Date();
                const thirtyDays = new Date(now.getTime() + 30 * 24 * 60 * 60 * 1000);
                const expiringCount = (voucherSummaries || []).filter((v: VoucherSummary) => {
                    if (v.status !== 'active' || !v.validUntil) return false;
                    const exp = new Date(v.validUntil);
                    return exp > now && exp <= thirtyDays;
                }).length;
                setExpiringSoonCount(expiringCount);

                setAppSettings(userSettings);
                const initialMode = (userSettings?.balanceViewMode as 'spendable' | 'net') || 
                    (localStorage.getItem('hmc_balance_view_mode') as 'spendable' | 'net') || 
                    'spendable';
                setBalanceViewMode(initialMode);

                const hasRealActiveVouchers = (voucherSummaries || []).some(
                    (v: VoucherSummary) => v.status === "active" && !v.isTestVoucher
                );
                const isBackupOutdatedOrMissing = !userSettings?.lastBackupAt || (() => {
                    const backupDate = new Date(userSettings.lastBackupAt);
                    if (isNaN(backupDate.getTime())) return true;
                    const fourteenDaysAgo = new Date(Date.now() - 14 * 24 * 60 * 60 * 1000);
                    return backupDate < fourteenDaysAgo;
                })();
                const needsBackup = !userSettings?.backupSyncFolder && isBackupOutdatedOrMissing;
                setShowBackupReminder(hasRealActiveVouchers && needsBackup);
            } catch (e) {
                const msg = `Failed to fetch dashboard data: ${stringifyError(e)}`;
                console.error(msg);
                setFeedbackMsg(`Error: ${msg}`);
            } finally {
                setIsLoading(false);
            }
        }
        fetchData();
    }, []);

    async function handleViewModeChange(mode: 'spendable' | 'net') {
        setBalanceViewMode(mode);
        try {
            localStorage.setItem('hmc_balance_view_mode', mode);
            if (appSettings) {
                const updated = { ...appSettings, balanceViewMode: mode };
                setAppSettings(updated);
                await settingsService.saveSettings(updated).catch(() => {});
            }
        } catch {
            // ignore best-effort storage
        }
    }

    async function handleCopyUserId() {
        if (!userId) return;
        try {
            const ok = await copyToClipboard(userId);
            if (ok) {
                setCopied(true);
                setTimeout(() => setCopied(false), 2000);
            } else {
                setFeedbackMsg("Error: Failed to copy User ID");
            }
        } catch (e) {
            const msg = `Failed to copy User ID: ${stringifyError(e)}`;
            console.error(msg);
            setFeedbackMsg(`Error: ${msg}`);
        }
    }

    function calculatePopoverPosition(rect: DOMRect, popoverEl?: HTMLElement | null): { top: number; left: number } {
        const popoverWidth = popoverEl?.offsetWidth || 256;
        const popoverHeight = popoverEl?.offsetHeight || 180;
        const margin = 12;

        let left = rect.right - popoverWidth;
        if (left < margin) {
            left = margin;
        }
        if (typeof window !== 'undefined' && left + popoverWidth > window.innerWidth - margin) {
            left = window.innerWidth - popoverWidth - margin;
        }

        const spaceBelow = typeof window !== 'undefined' ? window.innerHeight - rect.bottom - margin : 300;
        let top: number;
        if (spaceBelow < popoverHeight && rect.top > popoverHeight + margin) {
            top = rect.top - popoverHeight - 8;
        } else {
            top = rect.bottom + 8;
        }

        if (top < margin) {
            top = margin;
        }
        if (typeof window !== 'undefined' && top + popoverHeight > window.innerHeight - margin) {
            top = window.innerHeight - popoverHeight - margin;
        }

        return { top, left };
    }

    function formatAmount(amountStr: string): string {
        const num = parseFloat(amountStr);
        if (isNaN(num)) return amountStr;
        return num.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    }

    function formatNetBalance(amountStr?: string): string {
        if (!amountStr) return "0.00";
        const num = parseFloat(amountStr);
        if (isNaN(num)) return amountStr;
        const formatted = Math.abs(num).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
        if (num > 0) return `+${formatted}`;
        if (num < 0) return `-${formatted}`;
        return `0.00`;
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
                    return { label: t('dashboard.eventVoucherCreated'), icon: Plus, color: 'text-blue-600', bgColor: 'bg-blue-50' };
                case 'transferSent': {
                    const recipientName = resolveIdentity(bff.counterpartyId, bff.counterpartyName);
                    return { label: t('dashboard.eventSent', { name: recipientName }), icon: ArrowUpRight, color: 'text-rose-600', bgColor: 'bg-rose-50' };
                }
                case 'transferReceived': {
                    const senderName = resolveIdentity(bff.counterpartyId, bff.counterpartyName);
                    return { label: t('dashboard.eventReceived', { name: senderName }), icon: ArrowDownLeft, color: 'text-emerald-600', bgColor: 'bg-emerald-50' };
                }
                case 'voucherQuarantined':
                    return { label: t('dashboard.eventVoucherQuarantined'), icon: AlertCircle, color: 'text-amber-600', bgColor: 'bg-amber-50' };
                case 'voucherActivated':
                    return { label: t('dashboard.eventVoucherActivated'), icon: CheckCircle2, color: 'text-emerald-600', bgColor: 'bg-emerald-50' };
                case 'voucherVoided':
                    return { label: t('dashboard.eventVoucherVoided'), icon: AlertCircle, color: 'text-gray-600', bgColor: 'bg-gray-50' };
                case 'voucherExpired':
                    return { label: t('dashboard.eventVoucherExpired'), icon: Clock, color: 'text-orange-600', bgColor: 'bg-orange-50' };
            }
        }
        return { label: t('dashboard.eventUnknown'), icon: AlertCircle, color: 'text-gray-600', bgColor: 'bg-gray-50' };
    }
    const truncatedUserId = userId ? truncateUserId(userId) : t('common.loading');

    return (
        <PageLayout 
            customHeader={
                <header className="flex-shrink-0 sticky top-0 z-30 px-6 py-2 glass-panel border-b-0 rounded-b-3xl mb-4">
                    <div className="flex items-center justify-between gap-3 text-[10px] sm:text-xs font-bold text-theme-light">
                        <div className="flex items-center gap-3 min-w-0">
                            <span className="relative flex h-2 w-2 shrink-0">
                                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                                <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                            </span>
                            <div className="flex items-center gap-1.5 px-2 py-1 bg-white/50 rounded-full border border-theme-subtle/30 shadow-inner-soft min-w-0">
                                <UserCircle size={12} className="text-theme-primary shrink-0" />
                                <span className="text-theme-secondary truncate">{profileName}</span>
                            </div>
                        </div>
                        
                        <div className="flex items-center gap-2 shrink-0">
                            <button 
                                onClick={handleCopyUserId}
                                title={t('dashboard.copyUserIdHint', { id: userId })}
                                className={`flex items-center gap-2 px-3 py-1.5 rounded-full transition-all duration-300 border ${
                                    copied 
                                    ? 'bg-emerald-50 text-emerald-600 border-emerald-200' 
                                    : 'bg-white/50 text-theme-secondary border-theme-subtle/50 hover:border-theme-primary hover:bg-white hover:shadow-sm'
                                }`}
                            >
                                <span className="font-mono text-[10px]">{copied ? t('common.copied') : truncatedUserId}</span>
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
                        <h1 className="text-3xl font-black text-theme-primary tracking-tighter">{t('dashboard.welcomeTitle')}</h1>
                        <p className="text-sm font-medium text-theme-light mt-1">{t('dashboard.welcomeSubtitle')}</p>
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
                                    <p className="font-bold text-sm text-theme-secondary">{t('dashboard.securityWarning')}</p>
                                    <p className="text-xs opacity-70">{t('dashboard.securityWarningDesc')}</p>
                                </div>
                            </div>
                            <ArrowUpRight size={18} className="opacity-30" />
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
                                    <p className="font-bold text-sm text-theme-secondary">{t('dashboard.quarantineLocked', { count: quarantinedCount })}</p>
                                    <p className="text-xs opacity-70">{t('dashboard.quarantineDesc')}</p>
                                </div>
                            </div>
                            <ArrowUpRight size={18} className="opacity-30" />
                        </Card>
                    )}

                    {expiringSoonCount > 0 && (
                        <Card 
                            variant="glass" 
                            className="py-2 px-4 flex items-center justify-between border-l-4 border-l-amber-500 cursor-pointer"
                            hover
                            onClick={() => navigate({ view: 'wallet', initialStatusFilter: 'active', initialFilter: 'expiringSoon' })}
                        >
                            <div className="flex items-center gap-4">
                                <div className="p-2 rounded-xl bg-amber-500 text-white">
                                    <Clock size={20} />
                                </div>
                                <div>
                                    <p className="font-bold text-sm text-theme-secondary">{t('dashboard.expiringSoonTitle', { count: expiringSoonCount })}</p>
                                    <p className="text-xs opacity-70">{t('dashboard.expiringSoonDesc')}</p>
                                </div>
                            </div>
                            <ArrowUpRight size={18} className="opacity-30" />
                        </Card>
                    )}

                    {showBackupReminder && (
                        <Card 
                            variant="glass" 
                            className="py-2 px-4 flex flex-wrap gap-3 items-center justify-between border-l-4 border-l-theme-primary"
                            hover
                            onClick={() => navigate({ view: 'settings' })}
                        >
                            <div className="flex items-center gap-4 min-w-0">
                                <div className="p-2 rounded-xl bg-theme-primary text-white shrink-0">
                                    <Archive size={20} />
                                </div>
                                <div className="min-w-0">
                                    <p className="font-bold text-sm text-theme-secondary truncate">{t('dashboard.backupReminderTitle')}</p>
                                    <p className="text-xs opacity-70 truncate">{t('dashboard.backupReminderDesc')}</p>
                                </div>
                            </div>
                            <Button variant="primary" size="xs" className="shadow-none shrink-0">{t('dashboard.backupReminderButton')}</Button>
                        </Card>
                    )}
                </div>

                {/* Zone 2: Balances (Wallet Cards) */}
                <section>
                    <div className="flex items-center justify-between mb-4 px-2">
                        <h2 className="text-xs font-black text-theme-light uppercase tracking-[0.2em]">{t('dashboard.yourBalances')}</h2>
                        <div className="flex items-center bg-white/40 p-0.5 rounded-xl border border-theme-subtle/50 text-xs shadow-inner-soft">
                            <button
                                type="button"
                                onClick={() => handleViewModeChange('spendable')}
                                className={`px-3 py-1 rounded-lg font-bold transition-all text-[11px] ${
                                    balanceViewMode === 'spendable'
                                        ? 'bg-white text-theme-primary shadow-sm'
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                {t('dashboard.modeSpendable')}
                            </button>
                            <button
                                type="button"
                                onClick={() => handleViewModeChange('net')}
                                className={`px-3 py-1 rounded-lg font-bold transition-all text-[11px] ${
                                    balanceViewMode === 'net'
                                        ? 'bg-white text-theme-primary shadow-sm'
                                        : 'text-theme-light hover:text-theme-secondary'
                                }`}
                            >
                                {t('dashboard.modeNet')}
                            </button>
                        </div>
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
                                            <div className="flex items-center gap-2">
                                                <div className="px-2 py-1 bg-white/20 backdrop-blur-md rounded-lg border border-white/20 text-[10px] font-bold text-white">
                                                    {t('dashboard.vouchersCount', { count })}
                                                </div>
                                                <div className="relative">
                                                    <button
                                                        type="button"
                                                        data-popover-trigger={balance.standardUuid}
                                                        onClick={(e) => {
                                                            e.stopPropagation();
                                                            if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
                                                            const rect = e.currentTarget.getBoundingClientRect();
                                                            if (activeInfoPopover === balance.standardUuid) {
                                                                setActiveInfoPopover(null);
                                                                setPopoverAnchorRect(null);
                                                            } else {
                                                                setActiveInfoPopover(balance.standardUuid);
                                                                setPopoverAnchorRect(rect);
                                                            }
                                                        }}
                                                        onMouseEnter={(e) => {
                                                            if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
                                                            const rect = e.currentTarget.getBoundingClientRect();
                                                            setActiveInfoPopover(balance.standardUuid);
                                                            setPopoverAnchorRect(rect);
                                                        }}
                                                        onMouseLeave={() => {
                                                            closeTimerRef.current = setTimeout(() => {
                                                                setActiveInfoPopover(null);
                                                                setPopoverAnchorRect(null);
                                                            }, 200);
                                                        }}
                                                        className="p-1 rounded-full bg-white/20 hover:bg-white/30 text-white transition-colors"
                                                        title={t('dashboard.balanceDetailsTooltip')}
                                                        aria-label={t('dashboard.balanceDetailsTooltip')}
                                                    >
                                                        <Info size={14} />
                                                    </button>
                                                </div>
                                            </div>
                                        </div>

                                        <div className="relative z-10 flex flex-col items-start">
                                            <div className="flex items-baseline gap-2">
                                                <span className="text-4xl font-black text-white tracking-tighter">
                                                    {balanceViewMode === 'net' 
                                                        ? formatNetBalance(balance.netBalance || balance.totalAmount) 
                                                        : formatAmount(balance.totalAmount)}
                                                </span>
                                            </div>
                                            {balanceViewMode === 'net' && balance.ownAvailableAmount && parseFloat(balance.ownAvailableAmount) > 0 && (
                                                <span className="text-[11px] font-medium text-white/80 mt-1">
                                                    {t('dashboard.ownFrameAvailable', { amount: formatAmount(balance.ownAvailableAmount) })}
                                                </span>
                                            )}
                                        </div>

                                        <div className="relative z-10 flex justify-end">
                                            <div className="text-[10px] font-bold text-white/40 uppercase tracking-widest group-hover:text-white transition-colors">
                                                {t('dashboard.viewDetails')}
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
                                    <span className="text-sm font-bold uppercase tracking-widest">{t('voucher.create')}</span>
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
                        {t('transfer.send')}
                    </Button>
                    <Button 
                        onClick={() => navigate({ view: 'receive_bundle' })} 
                        className="min-w-[140px] gap-2 shadow-premium"
                        variant="secondary"
                        size="lg"
                    >
                        <ArrowDownLeft size={20} />
                        {t('transfer.receive')}
                    </Button>
                    <Button 
                        onClick={() => navigate({ view: 'create_voucher' })}
                        className="min-w-[140px] gap-2 shadow-premium"
                        variant="secondary"
                        size="lg"
                    >
                        <Plus size={20} />
                        {t('common.create')}
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
                                    <p className="font-black text-theme-secondary uppercase tracking-wider text-xs">{t('auth.actionRequired')}</p>
                                    <p className="text-lg font-bold text-theme-primary">{t('dashboard.needSignatures', { count: incompleteCount })}</p>
                                </div>
                            </div>
                            <Button variant="primary" size="sm" className="rounded-full px-6">{t('common.view')}</Button>
                        </Card>
                    </section>
                )}

                {/* Zone 5: Recent Activity */}
                <section>
                    <div className="flex items-center justify-between mb-6 px-2">
                        <h2 className="text-xs font-black text-theme-light uppercase tracking-[0.2em]">{t('dashboard.recentActivity')}</h2>
                        <button 
                            onClick={() => navigate({ view: 'transaction_history' })}
                            className="text-[10px] font-black text-theme-accent uppercase tracking-widest hover:underline"
                        >
                            {t('dashboard.historyLink')}
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
                                    <div className="flex items-center gap-4 flex-1 min-w-0">
                                        <div className={`p-3 rounded-2xl ${bgColor} ${color} transition-transform group-hover:scale-110 shrink-0`}>
                                            <Icon size={20} />
                                        </div>
                                        <div className="min-w-0">
                                            <p className="font-bold text-sm text-theme-secondary truncate">{label}</p>
                                            <p className="text-[10px] font-bold text-theme-light uppercase tracking-widest mt-0.5">
                                                {event.bffData.displayCurrency && `${event.bffData.amount} ${event.bffData.displayCurrency}`}
                                            </p>
                                        </div>
                                    </div>
                                    <div className="text-right shrink-0">
                                        <span className="text-[10px] font-bold text-theme-placeholder uppercase tracking-widest">
                                            {formatTimestamp(event.timestamp)}
                                        </span>
                                    </div>
                                </Card>
                            );
                        }) : (
                            <div className="text-center py-12 px-6">
                                <Card variant="none" className="border-2 border-dashed border-theme-subtle p-8 opacity-50">
                                    <p className="text-sm font-bold uppercase tracking-widest text-theme-placeholder">{t('dashboard.noRecentActivity')}</p>
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

            {activeInfoPopover && popoverAnchorRect && typeof document !== 'undefined' && (() => {
                const activeBalance = balances.find(b => b.standardUuid === activeInfoPopover);
                if (!activeBalance) return null;
                const pos = calculatePopoverPosition(popoverAnchorRect, popoverRef.current);

                return createPortal(
                    <div 
                        ref={popoverRef}
                        style={{ top: `${pos.top}px`, left: `${pos.left}px` }}
                        onClick={(e) => e.stopPropagation()}
                        onMouseEnter={() => {
                            if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
                        }}
                        onMouseLeave={() => {
                            closeTimerRef.current = setTimeout(() => {
                                setActiveInfoPopover(null);
                                setPopoverAnchorRect(null);
                            }, 200);
                        }}
                        className="fixed z-[9999] w-64 p-3 bg-slate-900/95 backdrop-blur-md text-white rounded-2xl shadow-2xl border border-white/20 text-xs space-y-2 pointer-events-auto animate-in fade-in zoom-in-95 duration-150"
                    >
                        <div className="font-bold border-b border-white/10 pb-1 text-white/90">
                            {t('dashboard.balanceBreakdownTitle')}
                        </div>
                        <div className="flex justify-between items-center text-white/80">
                            <span>{t('dashboard.breakdownReceived')}:</span>
                            <span className="font-mono font-bold text-emerald-400">
                                +{formatAmount(activeBalance.receivedAmount || activeBalance.totalAmount)}
                            </span>
                        </div>
                        <div className="flex justify-between items-center text-white/80">
                            <span>{t('dashboard.breakdownOwnAvailable')}:</span>
                            <span className="font-mono font-bold text-sky-400">
                                +{formatAmount(activeBalance.ownAvailableAmount || "0")}
                            </span>
                        </div>
                        <div className="flex justify-between items-center text-white/80">
                            <span>{t('dashboard.breakdownOwnCirculating')}:</span>
                            <span className="font-mono font-bold text-amber-400">
                                -{formatAmount(activeBalance.ownCirculatingAmount || "0")}
                            </span>
                        </div>
                        <div className="border-t border-white/10 pt-1 flex justify-between items-center font-bold">
                            <span className="text-white/90">{t('dashboard.breakdownNet')}:</span>
                            <span className={`font-mono ${parseFloat(activeBalance.netBalance || "0") < 0 ? 'text-rose-400' : 'text-emerald-400'}`}>
                                {formatNetBalance(activeBalance.netBalance || activeBalance.totalAmount)}
                            </span>
                        </div>
                    </div>,
                    document.body
                );
            })()}
        </PageLayout>
    );
}
