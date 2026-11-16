// src/components/Dashboard.tsx
import { useState, useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { voucherService } from "../services/voucherService";
import { transferService } from "../services/transferService";
import { profileService } from "../services/profileService";
import { contactService } from "../services/contactService";
import { settingsService } from "../services/settingsService";
import { marketService } from "../services/marketService";
import { recoveryService } from "../services/recoveryService";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";
import { Card } from "./ui/Card";
import { AggregatedBalance, VoucherSummary, WalletEvent, AppSettings, FrontendPendingSignRequest, FrontendPendingWotHandshake, IntegrityReport, RecoveryProtectionStatusDto } from "../types";
import { postService, POST_INBOX_UPDATED_EVENT } from "../services/postService";
import { useSession } from "../context/SessionContext";
import { IntegrityReportModal } from "./IntegrityReportModal";
import { FirstStepsChecklist } from "./FirstStepsChecklist";
import { InfoHint } from "./ui/InfoHint";
import { PageLayout } from "./ui/PageLayout";
import { Advanced } from "./ui/Advanced";
import { Disclosure } from "./ui/Disclosure";
import { TechId } from "./ui/TechId";
import { 
    Plus, 
    ArrowUpRight, 
    ArrowDownLeft, 
    ShieldAlert, 
    UserCircle,
    CheckCircle2,
    Clock,
    AlertCircle,
    Archive,
    Info,
    FileSignature,
    UserCheck,
    X,
    RefreshCw,
    Layers
} from "lucide-react";
import { truncateUserId } from "../utils/userIdHelper";
import { stringifyError, translateError } from "../utils/errorHelper";

import { useNavigation } from "../context/NavigationContext";
import { useContactResolver } from "../hooks/useContactResolver";

export function Dashboard() {
    const { t } = useTranslation();
    const tRef = useRef(t);
    tRef.current = t;
    const { navigate } = useNavigation();
    const { profileName, integrityReport, protectAction } = useSession();
    const { resolveIdentity } = useContactResolver();

    const [userId, setUserId] = useState("");
    const [balances, setBalances] = useState<AggregatedBalance[]>([]);
    const [activeVouchers, setActiveVouchers] = useState<VoucherSummary[]>([]);
    const [recentEvents, setRecentEvents] = useState<WalletEvent[]>([]);
    const [feedbackMsg, setFeedbackMsg] = useState("");
    const [activeVouchersCount, setActiveVouchersCount] = useState(0);
    const [incompleteCount, setIncompleteCount] = useState(0);
    const [quarantinedCount, setQuarantinedCount] = useState(0);

    // Channel post inbox items (§6.4.1 & §6.4.2)
    const [pendingSignRequests, setPendingSignRequests] = useState<FrontendPendingSignRequest[]>([]);
    const [pendingWotHandshakes, setPendingWotHandshakes] = useState<FrontendPendingWotHandshake[]>([]);
    const [isOpeningSignRequest, setIsOpeningSignRequest] = useState<string | null>(null);
    const [expiringSoonCount, setExpiringSoonCount] = useState(0);
    const [isLoading, setIsLoading] = useState(true);
    const [voucherCountsByStandard, setVoucherCountsByStandard] = useState<Record<string, number>>({});
    const [showIntegrityModal, setShowIntegrityModal] = useState(false);
    const [showBackupReminder, setShowBackupReminder] = useState(false);
    const [appSettings, setAppSettings] = useState<AppSettings | null>(null);
    const [protectionStatus, setProtectionStatus] = useState<RecoveryProtectionStatusDto | null>(null);
    const [contactsCount, setContactsCount] = useState(0);
    const [syncConnectedCount, setSyncConnectedCount] = useState(0);
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
        let active = true;
        logger.info("Dashboard component displayed");
        async function fetchData() {
            try {
                const [id, balanceList, , voucherSummaries, , contacts, eventHistory, userSettings, syncOverview, recoveryProtection] = await Promise.all([
                    profileService.getUserId(),
                    voucherService.getTotalBalanceByCurrency(),
                    transferService.getHistory().catch(() => []),
                    voucherService.getSummaries().catch(() => []),
                    profileService.getProfile().catch(() => null),
                    contactService.getContacts().catch(() => []),
                    transferService.getEventHistory(0, 5).catch(() => []),
                    settingsService.getSettings().catch(() => null),
                    marketService.getSyncOverviewStatus().catch(() => null),
                    recoveryService.getRecoveryProtectionStatus().catch(() => null),
                ]);
                if (!active) return;
                setUserId(id);
                setBalances(balanceList || []);
                setRecentEvents(eventHistory || []);
                setContactsCount((contacts || []).length);
                setProtectionStatus(recoveryProtection);
                setSyncConnectedCount(syncOverview?.connectedCount || 0);
                
                const activeVoucherList = (voucherSummaries || [])
                    .filter((v: VoucherSummary) => v.status === "active")
                    .sort((a: VoucherSummary, b: VoucherSummary) => {
                        const aTime = a.validUntil ? new Date(a.validUntil).getTime() : Number.MAX_SAFE_INTEGER;
                        const bTime = b.validUntil ? new Date(b.validUntil).getTime() : Number.MAX_SAFE_INTEGER;
                        return aTime - bTime;
                    });
                setActiveVouchers(activeVoucherList);
                const activeCount = activeVoucherList.length;
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
                if (!active) return;
                const msg = `Failed to fetch dashboard data: ${stringifyError(e)}`;
                console.error(msg);
                setFeedbackMsg(translateError(e, tRef.current));
            } finally {
                if (active) setIsLoading(false);
            }
        }
        fetchData();

        const loadPostInbox = () => {
            postService.getPostInboxSummary()
                .then(summary => {
                    if (!active) return;
                    setPendingSignRequests(summary.pendingSignRequests || []);
                    setPendingWotHandshakes(summary.pendingWotHandshakes || []);
                })
                .catch(() => {});
        };

        loadPostInbox();
        postService.syncPostChannels().catch(() => {});
        window.addEventListener(POST_INBOX_UPDATED_EVENT, loadPostInbox);

        return () => {
            active = false;
            window.removeEventListener(POST_INBOX_UPDATED_EVENT, loadPostInbox);
        };
    }, []);

    const handleOpenSignRequest = async (req: FrontendPendingSignRequest) => {
        setIsOpeningSignRequest(req.msgId);
        try {
            const voucherData = await protectAction(async (pwd) => {
                return await voucherService.openSigningRequest(req.containerBytes, pwd || undefined);
            });
            if (voucherData) {
                navigate({
                    view: 'sign_request',
                    voucherData,
                    requestMsgId: req.msgId,
                    senderDid: req.senderDid,
                });
            }
        } catch (e) {
            logger.error(`Failed to open signing request: ${stringifyError(e)}`);
            setFeedbackMsg(translateError(e, t));
        } finally {
            setIsOpeningSignRequest(null);
        }
    };

    const handleDismissSignRequest = async (msgId: string) => {
        try {
            await postService.dismissPendingPostItem('sign_request', msgId);
            setPendingSignRequests(prev => prev.filter(r => r.msgId !== msgId));
        } catch (e) {
            logger.error(`Failed to dismiss sign request: ${stringifyError(e)}`);
            setFeedbackMsg(translateError(e, t));
        }
    };

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

    const getIntegrityDetails = (report: IntegrityReport) => {
        switch (report.type) {
            case 'integrityOutdated':
                return {
                    title: t('dashboard.warningOutdatedTitle', 'Veralteter Profil-Stand erkannt'),
                    desc: t('dashboard.warningOutdatedDesc', 'Dein lokaler Datenstand passt nicht mehr zum gesicherten Profil-Siegel. Ohne Reparatur können Zahlungen fehlschlagen.'),
                    topic: 'offlineProtection' as const,
                };
            case 'manipulatedItems':
            case 'missingItems':
                return {
                    title: t('dashboard.warningConflictTitle', 'Gutschein-Konflikt oder Datenabweichung erkannt'),
                    desc: t('dashboard.warningConflictDesc', 'Gutscheindaten weichen vom kryptographischen Nachweis ab. Repariere dein Profil jetzt, um Datenverlust zu vermeiden.'),
                    topic: 'conflicts' as const,
                };
            case 'invalidSignature':
                return {
                    title: t('dashboard.warningInvalidSigTitle', 'Ungültige Profil-Signatur erkannt'),
                    desc: t('dashboard.warningInvalidSigDesc', 'Die Signatur deines Profil-Zustands stimmt nicht überein. Zahlungen sind bis zur Reparatur gesperrt.'),
                    topic: 'offlineProtection' as const,
                };
            case 'unknownItems':
            case 'missingIntegrityRecord':
            default:
                return {
                    title: t('dashboard.warningSealTitle', 'Profil-Integrität muss aktualisiert werden'),
                    desc: t('dashboard.warningSealDesc', 'Für dein Profil liegt noch kein aktuelles Siegel vor. Aktualisiere es jetzt, um dein Profil zu schützen.'),
                    topic: 'offlineProtection' as const,
                };
        }
    };

    function formatRelativeExpiry(validUntil?: string): string {
        if (!validUntil) return '';
        const targetMs = new Date(validUntil).getTime();
        if (isNaN(targetMs)) return '';
        const diffDays = Math.ceil((targetMs - Date.now()) / (24 * 60 * 60 * 1000));
        if (diffDays < 0) return t('dashboard.voucherExpiredShort');
        if (diffDays === 0) return t('dashboard.expiresToday');
        if (diffDays < 60) return t('dashboard.expiresInDays', { count: diffDays });
        if (diffDays < 730) return t('dashboard.expiresInMonths', { count: Math.max(1, Math.round(diffDays / 30)) });
        return t('dashboard.expiresInYears', { count: Math.max(1, Math.round(diffDays / 365)) });
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

    return (
        <PageLayout 
            customHeader={
                <header className="flex-shrink-0 sticky top-0 z-20 px-4 sm:px-6 py-2.5 glass-panel border-b-0 rounded-b-3xl mb-5">
                    <div className="flex flex-wrap items-center justify-between gap-3 text-[10px] sm:text-xs font-bold text-theme-light">
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
                        
                        <Advanced>
                            <div className="flex items-center gap-2 shrink-0">
                                <TechId value={userId} kind="did" />
                            </div>
                        </Advanced>
                    </div>
                </header>
            }
        >
            <div className="max-w-5xl mx-auto space-y-6 pb-24 md:pb-8">
                {/* Primary Quick Actions Bar - Always visible without scrolling */}
                <div 
                    className="fixed bottom-0 inset-x-0 z-30 p-3 bg-white/95 dark:bg-gray-900/95 backdrop-blur-md border-t border-theme-subtle flex items-center justify-center gap-3 md:static md:p-0 md:bg-transparent md:border-0 md:mb-6"
                    data-testid="dashboard-primary-actions"
                >
                    <Button 
                        onClick={() => navigate({ view: 'send_vouchers' })} 
                        className="flex-1 md:flex-initial min-w-[130px] sm:min-w-[150px] gap-2 shadow-sm"
                        size="md"
                        disabled={activeVouchersCount === 0}
                        data-testid="dashboard-action-send"
                    >
                        <ArrowUpRight size={18} />
                        <span>{t('transfer.send')}</span>
                    </Button>
                    <Button 
                        onClick={() => navigate({ view: 'receive_bundle' })} 
                        className="flex-1 md:flex-initial min-w-[130px] sm:min-w-[150px] gap-2 shadow-sm"
                        variant="secondary"
                        size="md"
                        data-testid="dashboard-action-receive"
                    >
                        <ArrowDownLeft size={18} />
                        <span>{t('transfer.receive')}</span>
                    </Button>
                    <Button 
                        onClick={() => navigate({ view: 'create_voucher', previousView: { view: 'logged_in' } })}
                        className="flex-1 md:flex-initial min-w-[130px] sm:min-w-[150px] gap-2 shadow-sm"
                        variant="secondary"
                        size="md"
                        data-testid="dashboard-action-create"
                    >
                        <Plus size={18} />
                        <span>{t('common.create')}</span>
                    </Button>
                </div>

                {/* Welcome Message */}
                {balances.length === 0 && (
                    <div className="px-2 pt-2 text-center">
                        <h1 className="text-3xl font-black text-theme-primary tracking-tighter">{t('dashboard.welcomeTitle')}</h1>
                        <p className="text-sm font-medium text-theme-light mt-1">{t('dashboard.welcomeSubtitle')}</p>
                    </div>
                )}

                {feedbackMsg && <p className="text-center text-red-500 mb-4">{feedbackMsg}</p>}

                {/* Warning Banners Section */}
                <div className="space-y-3">
                    {/* Incoming Pending Sign Requests (§6.4.1) */}
                    {pendingSignRequests.map(req => {
                        const senderName = req.senderName || resolveIdentity(req.senderDid, null) || truncateUserId(req.senderDid);
                        const isOpening = isOpeningSignRequest === req.msgId;
                        return (
                            <Card
                                key={req.msgId}
                                variant="glass"
                                className="py-3 px-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-l-4 border-l-teal-500 bg-teal-50/40"
                                data-testid="pending-sign-request-card"
                            >
                                <div className="flex items-center gap-3 min-w-0">
                                    <div className="p-2 rounded-xl bg-teal-500 text-white shrink-0">
                                        <FileSignature size={20} />
                                    </div>
                                    <div className="min-w-0">
                                        <p className="font-bold text-sm text-theme-secondary truncate">
                                            {t('dashboard.pendingSignRequestTitle', { name: senderName })}
                                        </p>
                                        <p className="text-xs text-theme-light truncate">
                                            {t('dashboard.pendingSignRequestDesc')}
                                        </p>
                                    </div>
                                </div>
                                <div className="flex items-center gap-2 shrink-0 self-end sm:self-center">
                                    <Button
                                        variant="outline"
                                        size="xs"
                                        disabled={isOpening}
                                        onClick={() => handleDismissSignRequest(req.msgId)}
                                        data-testid="dismiss-sign-request-button"
                                        className="text-theme-light hover:text-rose-600 gap-1"
                                    >
                                        <X size={12} />
                                        <span>{t('common.dismiss', 'Verwerfen')}</span>
                                    </Button>
                                    <Button
                                        variant="primary"
                                        size="xs"
                                        disabled={isOpening}
                                        onClick={() => handleOpenSignRequest(req)}
                                        data-testid="open-sign-request-button"
                                        className="gap-1 bg-teal-600 hover:bg-teal-700 text-white"
                                    >
                                        {isOpening ? <RefreshCw size={12} className="animate-spin" /> : <FileSignature size={12} />}
                                        <span>{t('dashboard.checkAndSign', 'Prüfen & Unterschreiben')}</span>
                                    </Button>
                                </div>
                            </Card>
                        );
                    })}

                    {/* Incoming Pending WoT Handshakes (§6.4.2) */}
                    {pendingWotHandshakes.length > 0 && (
                        <Card 
                            variant="glass" 
                            className="py-2.5 px-4 flex items-center justify-between border-l-4 border-l-sky-500 cursor-pointer bg-sky-50/40"
                            hover
                            onClick={() => navigate({ view: 'address_book' })}
                            data-testid="pending-wot-handshake-dashboard-card"
                        >
                            <div className="flex items-center gap-4">
                                <div className="p-2 rounded-xl bg-sky-500 text-white shrink-0">
                                    <UserCheck size={20} />
                                </div>
                                <div>
                                    <p className="font-bold text-sm text-theme-secondary">
                                        {t('dashboard.pendingWotHandshakesCount', { count: pendingWotHandshakes.length })}
                                    </p>
                                    <p className="text-xs opacity-70">
                                        {t('dashboard.pendingWotHandshakesHint')}
                                    </p>
                                </div>
                            </div>
                            <ArrowUpRight size={18} className="opacity-30" />
                        </Card>
                    )}

                    {integrityReport && integrityReport.type !== 'valid' && integrityReport.type !== 'missingIntegrityRecord' && (() => {
                        const details = getIntegrityDetails(integrityReport);
                        return (
                            <Card 
                                variant="glass" 
                                className="py-3 px-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-l-4 border-l-amber-500 bg-amber-50/60"
                                data-testid="security-warning-card"
                            >
                                <div className="flex items-start gap-3 min-w-0">
                                    <div className="p-2 rounded-xl shrink-0 mt-0.5 bg-amber-500 text-white">
                                        <ShieldAlert size={20} />
                                    </div>
                                    <div className="min-w-0 space-y-1">
                                        <p className="font-bold text-sm text-theme-secondary">
                                            {details.title}
                                        </p>
                                        <p className="text-xs text-theme-light">
                                            {details.desc}
                                        </p>
                                    </div>
                                </div>
                                <Button
                                    variant="primary"
                                    size="xs"
                                    onClick={() => setShowIntegrityModal(true)}
                                    data-testid="fix-integrity-button"
                                    className="shrink-0 self-end sm:self-center bg-amber-600 hover:bg-amber-700 text-white"
                                >
                                    {t('dashboard.fixNow', 'Jetzt beheben')}
                                </Button>
                            </Card>
                        );
                    })()}

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

                    {/* Two-Tier Protection Warning Banner */}
                    {(activeVouchersCount > 0 || (balances || []).length > 0) &&
                        (!protectionStatus?.hasIdentityRecovery || !protectionStatus?.hasVoucherBackup) && (
                        <Card 
                            variant="glass" 
                            className="py-3 px-4 flex flex-wrap gap-3 items-center justify-between border-l-4 border-l-amber-500 bg-amber-50/40 dark:bg-amber-950/20"
                            hover
                            onClick={() => navigate({ view: 'settings', initialSection: !protectionStatus?.hasIdentityRecovery ? 'security' : 'backup' })}
                            data-testid="protection-warning-banner"
                        >
                            <div className="flex items-center gap-4 min-w-0">
                                <div className="p-2 rounded-xl bg-amber-500 text-white shrink-0">
                                    <ShieldAlert size={20} />
                                </div>
                                <div className="min-w-0">
                                    <p className="font-bold text-sm text-theme-secondary truncate">
                                        {t('dashboard.protectionWarningTitle')}
                                    </p>
                                    <p className="text-xs text-theme-light line-clamp-2">
                                        {!protectionStatus?.hasIdentityRecovery && !protectionStatus?.hasVoucherBackup
                                            ? t('dashboard.protectionWarningBoth')
                                            : protectionStatus?.hasIdentityRecovery && !protectionStatus?.hasVoucherBackup
                                            ? t('dashboard.protectionWarningSyncOnly')
                                            : t('dashboard.protectionWarningIdentityOnly')}
                                    </p>
                                </div>
                            </div>
                            <Button 
                                variant="primary" 
                                size="xs" 
                                className="shadow-none shrink-0 bg-amber-500 hover:bg-amber-600 border-amber-600 text-white"
                            >
                                {t('dashboard.protectionWarningButton')}
                            </Button>
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

                {/* Zone 1.5: Getting Started Checklist */}
                <FirstStepsChecklist 
                    hasBackup={Boolean(appSettings?.lastBackupAt || appSettings?.backupSyncFolder || protectionStatus?.hasVoucherBackup)}
                    hasContact={contactsCount > 0}
                    hasVoucher={activeVouchersCount > 0 || (balances || []).length > 0}
                    hasSync={syncConnectedCount > 0}
                    hasIdentityRecovery={Boolean(protectionStatus?.hasIdentityRecovery)}
                    userId={userId}
                />

                {/* Zone 2: Balances (Wallet Cards) */}
                <section>
                    <div className="flex items-center justify-between mb-4 px-2">
                        <h2 className="text-xs font-black text-theme-light uppercase tracking-[0.2em]">{t('dashboard.yourBalances')}</h2>
                        <Advanced>
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
                        </Advanced>
                    </div>
                    
                    {isLoading ? (
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                            {[1, 2].map(i => <div key={i} className="h-48 bg-white/50 animate-pulse rounded-3xl border border-theme-subtle/50"></div>)}
                        </div>
                    ) : balances.length === 0 ? (
                        <Card 
                            variant="none" 
                            className="border-2 border-dashed border-theme-subtle bg-white/30 dark:bg-gray-800/20 p-8 rounded-3xl flex flex-col items-center justify-center text-center space-y-4"
                            data-testid="dashboard-empty-balances"
                        >
                            <div className="max-w-md mx-auto space-y-2">
                                <h3 className="text-base font-bold text-theme-primary">
                                    {t('dashboard.noVouchersTitle', 'Noch keine Gutscheine')}
                                </h3>
                                <p className="text-sm text-theme-secondary leading-relaxed">
                                    {t('dashboard.emptyBalanceExplanation', 'Gutscheine sind Versprechen von Menschen – z. B. eine Stunde Hilfe. Erstelle deinen ersten oder lass dir einen schicken.')}
                                </p>
                                <div className="pt-2 flex justify-center">
                                    <InfoHint topic="voucherBasics" />
                                </div>
                            </div>
                            <div className="flex flex-wrap justify-center gap-3 pt-2">
                                <Button 
                                    onClick={() => navigate({ view: 'create_voucher', previousView: { view: 'logged_in' } })}
                                    className="gap-2 shadow-sm"
                                    size="sm"
                                    data-testid="empty-balance-create-btn"
                                >
                                    <Plus size={16} />
                                    <span>{t('voucher.create')}</span>
                                </Button>
                                <Button 
                                    onClick={() => navigate({ view: 'receive_bundle' })} 
                                    className="gap-2 shadow-sm"
                                    variant="secondary"
                                    size="sm"
                                    data-testid="empty-balance-receive-btn"
                                >
                                    <ArrowDownLeft size={16} />
                                    <span>{t('transfer.receive')}</span>
                                </Button>
                            </div>
                        </Card>
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
                            
                            {/* Add More Placeholder when 1 balance */}
                            {balances.length === 1 && (
                                <Card 
                                    variant="none" 
                                    className="border-2 border-dashed border-theme-subtle bg-transparent p-6 h-48 flex flex-col items-center justify-center text-theme-placeholder hover:border-theme-primary hover:text-theme-primary transition-all group cursor-pointer"
                                    hover
                                    onClick={() => navigate({ view: 'create_voucher', previousView: { view: 'logged_in' } })}
                                >
                                    <Plus size={32} className="mb-2 opacity-50 group-hover:scale-110 transition-transform" />
                                    <span className="text-sm font-bold uppercase tracking-widest">{t('voucher.create')}</span>
                                </Card>
                            )}
                        </div>
                    )}
                </section>

                {/* Zone 2b: Merged Voucher Overview (Simple + Expert; full wallet button Expert-only) */}
                {!isLoading && activeVouchers.length > 0 && (
                    <section>
                        <Disclosure
                            toggleTestId="dashboard-vouchers-toggle"
                            contentTestId="dashboard-vouchers-list"
                            icon={<Layers size={16} className="text-theme-primary" />}
                            title={
                                <span className="text-xs font-black text-theme-secondary uppercase tracking-wider">
                                    {t('dashboard.showMyVouchers', { count: activeVouchers.length })}
                                </span>
                            }
                            className="bg-white/80 dark:bg-slate-900/80 border border-theme-subtle rounded-2xl overflow-hidden shadow-xs"
                            buttonClassName="w-full flex items-center justify-between gap-3 px-5 py-3.5 text-left hover:bg-bg-app/50 transition-colors cursor-pointer"
                            contentClassName="px-5 pb-4 pt-2 border-t border-theme-subtle/60 space-y-2.5"
                        >
                            <div className="space-y-2">
                                {activeVouchers.map((voucher) => {
                                    const fullCreatorName = [voucher.creatorFirstName, voucher.creatorLastName].filter(Boolean).join(' ');
                                    const issuerLabel = fullCreatorName || resolveIdentity(voucher.creatorId);
                                    const expiryLabel = formatRelativeExpiry(voucher.validUntil);
                                    return (
                                        <button
                                            key={voucher.localInstanceId}
                                            type="button"
                                            data-testid="dashboard-voucher-row"
                                            onClick={() => navigate({
                                                view: 'voucher_details',
                                                voucherId: voucher.localInstanceId,
                                                previousView: { view: 'logged_in' },
                                            })}
                                            className="w-full text-left px-4 py-3 rounded-xl border border-theme-subtle/80 bg-white dark:bg-slate-900 hover:border-theme-primary/40 hover:bg-bg-app/40 transition-all flex items-center justify-between gap-3 cursor-pointer"
                                        >
                                            <div className="min-w-0 space-y-0.5">
                                                <div className="flex items-center gap-2 flex-wrap">
                                                    <span className="font-bold text-sm text-theme-secondary tabular-nums">
                                                        {formatAmount(voucher.currentAmount)} {voucher.displayCurrency}
                                                    </span>
                                                    {voucher.isTestVoucher && (
                                                        <span className="px-1.5 py-0.5 text-[10px] font-bold uppercase rounded bg-amber-100 text-amber-800 border border-amber-200">
                                                            {t('voucher.testVoucher')}
                                                        </span>
                                                    )}
                                                </div>
                                                <p className="text-xs text-theme-light truncate">
                                                    {t('dashboard.voucherIssuedBy', { issuer: issuerLabel })}
                                                </p>
                                            </div>
                                            {expiryLabel && (
                                                <span className="text-[11px] font-medium text-theme-light shrink-0">
                                                    {expiryLabel}
                                                </span>
                                            )}
                                        </button>
                                    );
                                })}
                            </div>

                            <Advanced>
                                <div className="pt-1 flex justify-end">
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        data-testid="dashboard-open-wallet-btn"
                                        onClick={() => navigate({ view: 'wallet' })}
                                    >
                                        {t('dashboard.openFullVoucherList')}
                                    </Button>
                                </div>
                            </Advanced>
                        </Disclosure>
                    </section>
                )}

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
                            <div className="text-center py-8 px-6">
                                <Card variant="none" className="border-2 border-dashed border-theme-subtle p-8 space-y-4">
                                    <p className="text-sm font-bold uppercase tracking-widest text-theme-placeholder">{t('dashboard.noRecentActivity')}</p>
                                    <div className="flex flex-wrap items-center justify-center gap-2">
                                        <Button
                                            variant="primary"
                                            size="sm"
                                            onClick={() => navigate({ view: 'create_voucher', previousView: { view: 'logged_in' } })}
                                        >
                                            {t('voucher.create')}
                                        </Button>
                                        <Button
                                            variant="outline"
                                            size="sm"
                                            onClick={() => navigate({ view: 'receive_bundle' })}
                                        >
                                            {t('transfer.receive')}
                                        </Button>
                                    </div>
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
