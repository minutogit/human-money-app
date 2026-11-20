// src/components/ReceiveSuccessView.tsx
import { useEffect, useState } from "react";
import { ReceiveSuccessPayload, PendingReceiveCommit, TrustStatus, L2QuorumInfo, VoucherTrustDto } from "../../types";
import { Button } from "../ui/Button";
import { AlertBanner } from "../ui/AlertBanner";
import { L2QuorumBadge } from "../ui/L2QuorumBadge";
import { TrustBadge } from "../ui/TrustBadge";
import { TechId } from "../ui/TechId";
import { logger } from '../../utils/log';
import { stringifyError, translateError } from '../../utils/errorHelper';
import { profileService } from "../../services/profileService";
import { voucherService } from "../../services/voucherService";
import { transferService } from "../../services/transferService";
import { useOptionalSession } from "../../context/SessionContext";
import { APP_EVENTS, emitAppEvent } from "../../utils/appEvents";
import { useTranslation } from "react-i18next";
import { 
    CheckCircle2, 
    ShieldAlert, 
    ShieldCheck, 
    User, 
    MessageSquare, 
    ArrowRight,
    AlertTriangle,
    Layers,
    UserX,
    Info,
    WifiOff
} from "lucide-react";

interface ReceiveSuccessViewProps {
    payload: ReceiveSuccessPayload;
    pendingCommit?: PendingReceiveCommit;
    onApproveOnTrust?: () => Promise<ReceiveSuccessPayload | void>;
    onDone: () => void;
}

export function ReceiveSuccessView({ payload, pendingCommit, onApproveOnTrust, onDone }: ReceiveSuccessViewProps) {
    const { t } = useTranslation();
    const session = useOptionalSession();
    const [activePayload, setActivePayload] = useState<ReceiveSuccessPayload>(payload);
    const [trustStatus, setTrustStatus] = useState<TrustStatus>("clean");
    const [trustByLocalId, setTrustByLocalId] = useState<Record<string, VoucherTrustDto>>({});
    const [isApprovingOnTrust, setIsApprovingOnTrust] = useState(false);
    const [approvalError, setApprovalError] = useState<string | null>(null);

    useEffect(() => {
        setActivePayload(payload);
        setApprovalError(null);
    }, [payload]);

    useEffect(() => {
        let active = true;
        if (activePayload.involvedVouchers && activePayload.involvedVouchers.length > 0) {
            voucherService.getSummaries()
                .then(summaries => {
                    if (!active) return;
                    const map: Record<string, VoucherTrustDto> = {};
                    for (const s of summaries) {
                        if (s.trust) {
                            map[s.localInstanceId] = s.trust;
                        }
                    }
                    setTrustByLocalId(map);
                })
                .catch(e => logger.error(`Failed to fetch voucher trust summaries: ${stringifyError(e)}`));
        }
        return () => { active = false; };
    }, [activePayload.involvedVouchers]);

    function formatAmount(amountStr: string): string {
        const num = parseFloat(amountStr);
        if (isNaN(num)) return amountStr;
        return num.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    }

    const summable = Object.entries(activePayload.transferSummary.summableAmounts)
        .map(([unit, total]) => `${formatAmount(total)} ${unit}`);

    const countable = Object.entries(activePayload.transferSummary.countableItems)
        .map(([unit, total]) => t('transfer.receiveSuccess.countableItem', { count: total, unit }));

    const summaryString = [...summable, ...countable].join(', ') || t('transfer.receiveSuccess.noAssets');

    useEffect(() => {
        let active = true;
        logger.info(`Receive success screen: ${summaryString}`);
        if (activePayload.senderId) {
            profileService.checkReputation(activePayload.senderId)
                .then(res => {
                    if (active) setTrustStatus(res);
                })
                .catch(e => logger.error(`Reputation check error: ${stringifyError(e)}`));
        }
        return () => { active = false; };
    }, [activePayload, summaryString]);

    const [manualApprovalGranted, setManualApprovalGranted] = useState(
        () => Boolean(activePayload.checkoutVerdict?.isGoodsReleaseAuthorized)
    );

    useEffect(() => {
        setManualApprovalGranted(Boolean(activePayload.checkoutVerdict?.isGoodsReleaseAuthorized));
    }, [activePayload]);

    const handleApproveOnTrust = async () => {
        if (isApprovingOnTrust) return;
        setApprovalError(null);
        setIsApprovingOnTrust(true);
        try {
            if (onApproveOnTrust) {
                const updated = await onApproveOnTrust();
                if (updated) {
                    setActivePayload(updated);
                }
                setManualApprovalGranted(true);
                return;
            }
            if (pendingCommit) {
                const runCommit = async (walletPassword: string | null) =>
                    await transferService.receiveBundle({
                        bundleData: pendingCommit.bundleData,
                        standardDefinitionsToml: pendingCommit.standardDefinitionsToml,
                        password: pendingCommit.bundlePassword || walletPassword || undefined,
                        forceAcceptToleranceBundle: Boolean(pendingCommit.forceAcceptToleranceBundle),
                        commitMode: 'forceTrustOffline',
                        purchaseId: activePayload.purchaseId,
                    });
                const committed = session?.protectAction
                    ? await session.protectAction(runCommit)
                    : await runCommit(null);
                if (committed) {
                    setActivePayload(committed);
                    setManualApprovalGranted(true);
                    if (
                        (committed.verifiableConflicts &&
                            Object.keys(committed.verifiableConflicts).length > 0) ||
                        (committed.conflictSummaries?.some((c) => !c.isResolved && !c.localOverride) ??
                            false)
                    ) {
                        emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
                    }
                }
                return;
            }
            if (activePayload.newlyCommitted !== false) {
                setManualApprovalGranted(true);
            }
        } catch (err) {
            setApprovalError(translateError(err, t));
        } finally {
            setIsApprovingOnTrust(false);
        }
    };

    const isVictim = activePayload.conflictSummaries 
        ? activePayload.conflictSummaries.some(c => c.conflictRole === 'victim' && !c.isResolved && !c.localOverride) 
        : (activePayload.verifiableConflicts && Object.keys(activePayload.verifiableConflicts).length > 0);
    
    const isWitnessOnly = activePayload.conflictSummaries 
        ? (activePayload.conflictSummaries.some(c => c.conflictRole === 'witness' && !c.isResolved && !c.localOverride) && !isVictim) 
        : false;

    const checkoutVerdict = activePayload.checkoutVerdict;
    const outcome = checkoutVerdict?.outcome;
    const isQuarantined = isVictim || Boolean(outcome && outcome.startsWith('Quarantined'));
    const isReplay = !isQuarantined && activePayload.isReplay === true;
    const isPendingCommit =
        !isQuarantined && !isReplay && activePayload.newlyCommitted === false && !manualApprovalGranted;
    const isLiveConfirmed = checkoutVerdict?.canReleaseGoodsAutomatically === true && outcome === 'Confirmed';
    const isConfirmed = isLiveConfirmed && !isReplay && !isQuarantined;
    const isProvisional = outcome === 'Provisional' && checkoutVerdict?.canReleaseGoodsAutomatically === true && !isReplay && !isQuarantined;
    const isOfflineOnly = outcome === 'OfflineOnly';
    const isYellow = Boolean(outcome && outcome.startsWith('Yellow'));
    const isMultiVoucherYellow = outcome === 'Yellow:MultiVoucherManualApproval';
    const receiveKind = activePayload.receiveKind;

    const heroTitle = isQuarantined
        ? t('transfer.receiveSuccess.securityWarning')
        : isPendingCommit
          ? t('transfer.checkout.pendingTrustTitle', 'Prüfung & Bestätigung erforderlich')
          : receiveKind === 'claim'
            ? t('transfer.receiveSuccess.claimComplete', 'Bürgschafts-Anspruch erhalten')
            : receiveKind === 'redeem'
              ? t('transfer.receiveSuccess.redeemComplete', 'Gutschein eingelöst')
              : t('transfer.receiveSuccess.transferComplete');

    const heroDesc = isQuarantined
        ? t('transfer.receiveSuccess.securityIssueAffects')
        : isPendingCommit
          ? t('transfer.checkout.pendingTrustDesc', 'Keine vollständige Online-Bestätigung — bitte prüfe unten und bestätige auf Vertrauensbasis.')
          : isWitnessOnly
            ? t('transfer.receiveSuccess.witnessOnlyDesc')
            : receiveKind === 'claim'
              ? t('transfer.receiveSuccess.claimDesc', 'Der eingereichte Bürgschafts-Anspruch wurde geprüft und in deiner Gutscheinliste erfasst.')
              : receiveKind === 'redeem'
                ? t('transfer.receiveSuccess.redeemDesc', 'Die Einlösung wurde geprüft und im Guthaben berücksichtigt.')
                : t('transfer.receiveSuccess.normalSuccessDesc');

    // Unverified bundle import or raw response DTOs must never confirm payment or release goods automatically.
    const l2Quorum: L2QuorumInfo = isQuarantined
        ? { status: 'conflict', details: checkoutVerdict?.merchantWarning || t('transfer.receiveSuccess.conflictDesc') }
        : isLiveConfirmed
            ? {
                  status: 'final',
                  signerCount: activePayload.l2Quorum?.signerCount,
                  activeNodesCount: activePayload.l2Quorum?.activeNodesCount,
                  details: t('transfer.l2.finalHint'),
              }
            : isProvisional
                ? { status: 'provisional', details: t('transfer.l2.provisionalHint') }
                : isYellow
                    ? { status: 'yellow', details: checkoutVerdict?.merchantWarning || t('transfer.l2.yellowHint') }
                    : isOfflineOnly
                        ? { status: 'offline_only', details: checkoutVerdict?.merchantWarning || t('transfer.l2.offlineOnlyHint') }
                        : { status: 'unverified' };

    return (
        <div className="flex flex-col items-center justify-center min-h-[80vh] py-12 px-4 animate-in fade-in zoom-in-95 duration-500">
            <div className="w-full max-w-xl space-y-8">
                {/* Status Hero */}
                <div className="text-center space-y-4">
                    <div className="relative inline-block">
                        <div className={`p-6 rounded-[32px] ${isQuarantined ? 'bg-rose-500 text-white' : (isWitnessOnly || isPendingCommit) ? 'bg-amber-500 text-white' : 'bg-emerald-500 text-white'} shadow-premium-lg animate-bounce-subtle`}>
                            {isQuarantined ? <ShieldAlert size={64} /> : (isWitnessOnly || isPendingCommit) ? <AlertTriangle size={64} /> : <CheckCircle2 size={64} />}
                        </div>
                        <div className="absolute -bottom-2 -right-2 bg-white p-1.5 rounded-full shadow-md">
                            <div className={`${isQuarantined ? 'bg-rose-500' : (isWitnessOnly || isPendingCommit) ? 'bg-amber-500' : 'bg-emerald-500'} w-4 h-4 rounded-full border-2 border-white`}></div>
                        </div>
                    </div>
                    <div className="space-y-1">
                        <h1 className="text-3xl font-black text-theme-primary tracking-tight">
                            {heroTitle}
                        </h1>
                        <p className="text-theme-light font-medium">
                            {heroDesc}
                        </p>
                        {isConfirmed && (
                            <div
                                data-testid="online-verified-badge"
                                className="inline-flex items-center gap-2 px-4 py-1.5 rounded-full bg-emerald-100 border border-emerald-300 text-emerald-900 text-xs font-extrabold shadow-xs mt-2"
                            >
                                <CheckCircle2 size={16} className="text-emerald-600 shrink-0" />
                                <span>{t('transfer.checkout.onlineVerifiedBadge', 'Online geprüft — Gutscheine sauber')}</span>
                            </div>
                        )}
                    </div>
                </div>

                {/* Conflict Alert (if any) */}
                {isVictim && (
                    <div className="p-6 bg-rose-50 border-2 border-rose-100 rounded-[32px] flex items-start gap-4 shadow-sm animate-in shake duration-1000">
                        <AlertTriangle className="text-rose-500 shrink-0 mt-1" size={24} />
                        <div>
                            <h3 className="text-sm font-black text-rose-900 uppercase tracking-widest mb-1">{t('transfer.receiveSuccess.conflictTitle')}</h3>
                            <p className="text-xs text-rose-800 font-medium leading-relaxed">
                                {t('transfer.receiveSuccess.conflictDesc')}
                            </p>
                        </div>
                    </div>
                )}

                {isWitnessOnly && (
                    <div className="p-6 bg-amber-50 border-2 border-amber-100 rounded-[32px] flex items-start gap-4 shadow-sm animate-in fade-in duration-700">
                        <Info className="text-amber-500 shrink-0 mt-1" size={24} />
                        <div>
                            <h3 className="text-sm font-black text-amber-900 uppercase tracking-widest mb-1">{t('transfer.receiveSuccess.witnessTitle')}</h3>
                            <p className="text-xs text-amber-800 font-medium leading-relaxed" dangerouslySetInnerHTML={{ __html: t('transfer.receiveSuccess.witnessDesc') }} />
                        </div>
                    </div>
                )}

                {/* 2-Pillar Security Verification: L1 Voucher Authenticity & L2 Network Protection */}
                <div className="space-y-3" data-testid="two-pillar-security">
                    {/* Säule 1: Gutschein-Echtheit (L1) */}
                    <div 
                        data-testid="pillar-1-l1"
                        className="p-3.5 sm:p-4 rounded-3xl bg-white border border-emerald-200/80 shadow-xs flex items-center justify-between gap-3"
                    >
                        <div className="flex items-center gap-3 min-w-0">
                            <div className="p-2 rounded-xl bg-emerald-50 border border-emerald-200 text-emerald-600 shrink-0">
                                <ShieldCheck size={20} />
                            </div>
                            <div className="min-w-0">
                                <div className="flex items-center gap-2 flex-wrap">
                                    <span className="text-xs font-bold uppercase tracking-wider text-theme-secondary">
                                        {t('transfer.pillars.l1Title', '1. Gutschein-Echtheit (L1)')}
                                    </span>
                                    <span className="inline-flex items-center gap-1 rounded-full text-[10px] px-2 py-0.5 font-bold uppercase tracking-wider bg-emerald-600 text-white">
                                        <span className="w-1.5 h-1.5 rounded-full bg-white animate-pulse" />
                                        {isVictim ? t('transfer.pillars.l1FormatOnly', 'Format intakt') : t('transfer.pillars.l1Valid', 'Echt & gültig')}
                                    </span>
                                </div>
                                <p className="text-[11px] font-medium text-emerald-800 truncate">
                                    {isVictim 
                                        ? t('transfer.pillars.l1DescVictim', 'Schein-Format mathematisch intakt (aber im Netzwerk als Doppelausgabe blockiert)')
                                        : t('transfer.pillars.l1Desc', 'Schein mathematisch echt & unverfälscht (100 % offline geprüft)')}
                                </p>
                            </div>
                        </div>
                        <div className="hidden sm:flex items-center text-emerald-600 shrink-0 pr-1">
                            <CheckCircle2 size={18} />
                        </div>
                    </div>

                    {/* Säule 2: Netzwerkschutz (L2) */}
                    <L2QuorumBadge 
                        quorum={l2Quorum} 
                        isVerifiedCheckout={Boolean(isLiveConfirmed || isProvisional)} 
                        pillarLabel={t('transfer.pillars.l2Prefix', '2. Netzwerkschutz (L2)')}
                        reducedGuarantorProtection={checkoutVerdict?.reducedGuarantorProtection}
                    />
                </div>

                {/* Checkout & Goods Release (Warenfreigabe) Status: isQuarantined MUST precede isReplay! */}
                {checkoutVerdict && (
                    <div data-testid="goods-release-section" className="space-y-3">
                    {isQuarantined ? (
                        <div data-testid="goods-release-card" className="p-5 bg-rose-50 border-2 border-rose-200 rounded-[28px] flex items-center gap-3 shadow-sm">
                            <ShieldAlert className="text-rose-600 shrink-0" size={28} />
                            <div>
                                <h3 className="text-sm font-black text-rose-950 uppercase tracking-wider">{t('transfer.checkout.goodsBlocked', 'WARE NICHT HERAUSGEBEN!')}</h3>
                                <p className="text-xs text-rose-800 font-medium">{checkoutVerdict?.merchantWarning || t('transfer.checkout.goodsBlockedDesc', 'Doppelausgabe erkannt! Dieser Gutschein wurde bereits anderweitig ausgegeben.')}</p>
                            </div>
                        </div>
                    ) : isReplay ? (
                        <div data-testid="goods-release-card" className="p-5 bg-blue-50 border-2 border-blue-200 rounded-[28px] flex items-center justify-between shadow-sm">
                            <div className="flex items-center gap-3">
                                <Info className="text-blue-600 shrink-0" size={28} />
                                <div>
                                    <h3 className="text-sm font-black text-blue-950 uppercase tracking-wider">
                                        {t('transfer.checkout.replayTitle', 'Bereits verbucht (Wiederholung)')}
                                    </h3>
                                    <p className="text-xs text-blue-800 font-medium">
                                        {t('transfer.checkout.replayDesc', 'Diese Zahlung wurde zuvor bereits verbucht. Bitte Ware nicht doppelt ausgeben!')}
                                    </p>
                                </div>
                            </div>
                            <span className="text-[11px] font-mono font-bold bg-blue-200/70 text-blue-900 px-3 py-1.5 rounded-full uppercase tracking-wider shrink-0 flex items-center gap-1.5">
                                {t('transfer.checkout.replayBadge', 'WIEDERHOLUNG')}
                            </span>
                        </div>
                    ) : isConfirmed ? (
                        <div data-testid="goods-release-card" className="p-5 bg-emerald-50 border-2 border-emerald-200 rounded-[28px] flex items-center justify-between shadow-sm">
                            <div className="flex items-center gap-3">
                                <CheckCircle2 className="text-emerald-600 shrink-0" size={28} />
                                <div>
                                    <h3 className="text-sm font-black text-emerald-950 uppercase tracking-wider">
                                        {t('transfer.checkout.goodsReleasedConfirmed', 'Ware sicher herausgeben')}
                                    </h3>
                                    <p className="text-xs text-emerald-800 font-medium">
                                        {t('transfer.checkout.goodsReleasedConfirmedDesc', 'Weltweit durch großes Server-Netzwerk final bestätigt.')}
                                    </p>
                                </div>
                            </div>
                            <span className="text-[11px] font-mono font-bold bg-emerald-200/70 text-emerald-900 px-3 py-1.5 rounded-full uppercase tracking-wider shrink-0 flex items-center gap-1.5">
                                <span className="text-emerald-600 font-mono">●●</span>
                                {t('transfer.checkout.level2Tag', 'Stufe 2')}
                            </span>
                        </div>
                    ) : isProvisional ? (
                        <div data-testid="goods-release-card" className="p-5 bg-emerald-50/80 border-2 border-emerald-200 rounded-[28px] flex items-center justify-between shadow-sm">
                            <div className="flex items-center gap-3">
                                <CheckCircle2 className="text-emerald-600 shrink-0" size={28} />
                                <div>
                                    <h3 className="text-sm font-black text-emerald-950 uppercase tracking-wider">
                                        {t('transfer.checkout.goodsReleasedProvisional', 'Ware freigegeben')}
                                    </h3>
                                    <p className="text-xs text-emerald-800 font-medium">
                                        {t('transfer.checkout.goodsReleasedProvisionalDesc', 'Zahlung durch Server-Netzwerk gegen Doppelausgabe gesichert.')}
                                    </p>
                                </div>
                            </div>
                            <span className="text-[11px] font-mono font-bold bg-emerald-200/50 text-emerald-900 px-3 py-1.5 rounded-full uppercase tracking-wider shrink-0 flex items-center gap-1.5">
                                <span className="text-emerald-600 font-mono">●○</span>
                                {t('transfer.checkout.level1Tag', 'Stufe 1')}
                            </span>
                        </div>
                    ) : isYellow ? (
                        <div data-testid="goods-release-card" className="p-5 bg-amber-50 border-2 border-amber-200 rounded-[28px] space-y-3 shadow-sm">
                            <div className="flex items-start gap-3">
                                <AlertTriangle className="text-amber-600 shrink-0 mt-0.5" size={24} />
                                <div className="flex-1">
                                    <div className="flex items-center justify-between gap-2">
                                        <h3 className="text-sm font-black text-amber-950 uppercase tracking-wider">
                                            {manualApprovalGranted
                                                ? t('transfer.checkout.acceptedOnTrust', 'Auf Vertrauensbasis angenommen')
                                                : t('transfer.checkout.yellowManualTitle', 'Manuelle Kassen-Prüfung nötig')}
                                        </h3>
                                        <span className="text-[11px] font-mono font-bold bg-amber-200/70 text-amber-900 px-2.5 py-1 rounded-full uppercase tracking-wider shrink-0">
                                            {t('transfer.l2.yellowShort', 'Prüfen')}
                                        </span>
                                    </div>
                                    <p className="text-xs text-amber-900 font-medium mt-1">
                                        {isMultiVoucherYellow
                                            ? t('transfer.checkout.multiVoucherWarning', 'Dieses Bündel enthält mehrere Gutscheine. Bitte prüfe vor Warenherausgabe jeden Schein kurz manuell auf Vertrauen.')
                                            : (checkoutVerdict?.merchantWarning || t('transfer.l2.yellowHint', 'Server-Netzwerk eingeschränkt erreichbar.'))}
                                    </p>
                                </div>
                            </div>
                            {!manualApprovalGranted ? (
                                <div className="space-y-2">
                                    {approvalError && <AlertBanner variant="error">{approvalError}</AlertBanner>}
                                    <Button
                                        data-testid="merchant-approve-btn"
                                        variant="secondary"
                                        size="sm"
                                        disabled={isApprovingOnTrust}
                                        onClick={() => void handleApproveOnTrust()}
                                        className="w-full py-2.5 rounded-2xl text-xs font-bold bg-amber-700 hover:bg-amber-800 text-white border-none shadow-sm cursor-pointer"
                                    >
                                        {isApprovingOnTrust
                                            ? t('transfer.checkout.approvingBtn', 'Wird verbucht...')
                                            : t('transfer.checkout.acceptOnTrustBtn', 'Zahlung auf Vertrauensbasis annehmen')}
                                    </Button>
                                </div>
                            ) : (
                                <div data-testid="merchant-approved-badge" className="text-xs font-bold text-emerald-700 bg-emerald-100/70 px-3 py-1.5 rounded-xl flex items-center gap-2">
                                    <CheckCircle2 size={16} />
                                    {t('transfer.checkout.acceptedOnTrust', 'Auf Vertrauensbasis angenommen')}
                                </div>
                            )}
                        </div>
                    ) : (
                        <div data-testid="goods-release-card" className="p-5 bg-slate-50 border-2 border-slate-200 rounded-[28px] space-y-3 shadow-sm">
                            <div className="flex items-start gap-3">
                                <WifiOff className="text-slate-600 shrink-0 mt-0.5" size={24} />
                                <div className="flex-1">
                                    <div className="flex items-center justify-between gap-2">
                                        <h3 className="text-sm font-black text-slate-900 uppercase tracking-wider">
                                            {manualApprovalGranted ? t('transfer.checkout.acceptedOnTrust', 'Auf Vertrauensbasis angenommen') : t('transfer.checkout.acceptOnTrustTitle', 'Offline-Zahlung (Vertrauensbasis)')}
                                        </h3>
                                        <span className="text-[11px] font-mono font-bold bg-slate-200 text-slate-700 px-2.5 py-1 rounded-full uppercase tracking-wider shrink-0 flex items-center gap-1">
                                            <span className="text-slate-500 font-mono">○○</span>
                                            {t('transfer.l2.offlineTitle', 'Offline-Zahlung')}
                                        </span>
                                    </div>
                                    <p className="text-xs text-slate-700 font-medium mt-1">
                                        {checkoutVerdict?.merchantWarning || t('transfer.checkout.acceptOnTrustDesc', 'Keine Server-Verbindung vorhanden. Zahlung basiert auf Vertrauen (z. B. Stammkunden oder Kleinstbeträge).')}
                                    </p>
                                </div>
                            </div>
                            {!manualApprovalGranted ? (
                                <div className="space-y-2 pt-1">
                                    {approvalError && <AlertBanner variant="error">{approvalError}</AlertBanner>}
                                    <Button
                                        data-testid="merchant-approve-btn"
                                        variant="secondary"
                                        size="sm"
                                        disabled={isApprovingOnTrust}
                                        onClick={() => void handleApproveOnTrust()}
                                        className="w-full py-2.5 rounded-2xl text-xs font-bold bg-slate-700 hover:bg-slate-800 text-white border-none shadow-sm cursor-pointer"
                                    >
                                        {isApprovingOnTrust
                                            ? t('transfer.checkout.approvingBtn', 'Wird verbucht...')
                                            : t('transfer.checkout.acceptOnTrustBtn', 'Zahlung auf Vertrauensbasis annehmen')}
                                    </Button>
                                </div>
                            ) : (
                                <div data-testid="merchant-approved-badge" className="text-xs font-bold text-emerald-700 bg-emerald-100/70 px-3 py-1.5 rounded-xl flex items-center gap-2">
                                    <CheckCircle2 size={16} />
                                    {t('transfer.checkout.acceptedOnTrust', 'Auf Vertrauensbasis angenommen')}
                                </div>
                            )}
                        </div>
                    )}
                </div>
                )}

                {!isQuarantined && (activePayload.unrelatedOpenConflictsCount ?? 0) > 0 && (
                    <div
                        data-testid="unrelated-conflicts-notice"
                        className="p-4 bg-amber-50 border border-amber-200 rounded-2xl flex items-start gap-3 shadow-sm"
                    >
                        <AlertTriangle className="text-amber-600 shrink-0 mt-0.5" size={18} />
                        <p className="text-xs font-medium text-amber-900">
                            {t('transfer.receiveSuccess.unrelatedConflictsNotice', {
                                count: activePayload.unrelatedOpenConflictsCount,
                                defaultValue:
                                    activePayload.unrelatedOpenConflictsCount === 1
                                        ? 'Hinweis: Im Profil gibt es unabhängig von dieser Zahlung 1 ungelösten Doppelausgabe-Konflikt.'
                                        : `Hinweis: Im Profil gibt es unabhängig von dieser Zahlung ${activePayload.unrelatedOpenConflictsCount} ungelöste Doppelausgabe-Konflikte.`,
                            })}
                        </p>
                    </div>
                )}

                {/* Transfer Details Card */}
                <div className="bg-white border border-theme-subtle rounded-[40px] p-8 shadow-premium space-y-8">
                    {/* Amount Hero */}
                    <div className="text-center space-y-2">
                        <span className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light">{t('transfer.receiveSuccess.totalReceived')}</span>
                        <div className="text-4xl font-black text-theme-primary tracking-tighter">
                            {summaryString}
                        </div>
                    </div>

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-6">
                        {/* Sender Info */}
                        <div className="space-y-3">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5">
                                <User size={10} /> {t('transfer.receiveSuccess.sender')}
                            </span>
                            <div className="p-4 bg-theme-subtle/10 rounded-2xl border border-theme-subtle/20">
                                <p className="text-sm font-black text-theme-primary truncate">
                                    {activePayload.senderProfileName || t('transfer.receiveSuccess.unknownSender')}
                                </p>
                                {activePayload.senderId ? (
                                    <div className="mt-1.5">
                                        <TechId value={activePayload.senderId} kind="did" />
                                    </div>
                                ) : (
                                    <p className="text-[10px] font-bold text-theme-light mt-1">
                                        {t('transfer.receiveSuccess.noDidDisclosed')}
                                    </p>
                                )}
                            </div>
                            
                            {(activePayload.wotImportedLists ?? 0) > 0 && (
                                <div className="flex items-center gap-1.5 px-3 py-1.5 bg-emerald-50 text-emerald-800 rounded-xl border border-emerald-200 text-[10px] font-bold">
                                    <ShieldCheck size={13} className="text-emerald-600 shrink-0" />
                                    <span>{t('transfer.receiveSuccess.wotSyncedBadge')}</span>
                                </div>
                            )}

                            {/* Reputation Warning */}
                            {typeof trustStatus === 'object' && 'knownOffender' in trustStatus && (
                                <div className="flex items-center gap-2 px-3 py-2 bg-rose-50 rounded-xl border border-rose-100">
                                    <UserX size={14} className="text-rose-500" />
                                    <span className="text-[10px] font-bold text-rose-700">{t('transfer.receiveSuccess.flaggedPastConflicts')}</span>
                                </div>
                            )}
                        </div>

                        {/* Notes Info */}
                        <div className="space-y-3">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5">
                                <MessageSquare size={10} /> {t('transfer.receiveSuccess.note')}
                            </span>
                            <div className="p-4 bg-theme-subtle/10 rounded-2xl border border-theme-subtle/20 h-[72px] overflow-y-auto no-scrollbar">
                                <p className="text-xs font-medium text-theme-secondary italic leading-relaxed">
                                    {activePayload.notes ? `"${activePayload.notes}"` : t('transfer.receiveSuccess.noMemo')}
                                </p>
                            </div>
                        </div>
                    </div>

                    {/* Breakdown */}
                    {activePayload.involvedVouchersDetails && activePayload.involvedVouchersDetails.length > 0 && (
                        <div className="space-y-3">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5">
                                <Layers size={10} /> {t('transfer.receiveSuccess.includedVouchers', { count: activePayload.involvedVouchersDetails.length })}
                            </span>
                            <div className="grid grid-cols-1 gap-2">
                                {activePayload.involvedVouchersDetails.map((detail, index) => (
                                    <div key={index} className="flex items-center justify-between p-3 bg-white border border-theme-subtle rounded-xl shadow-sm">
                                        <div className="flex items-center gap-2">
                                            <div className="w-1.5 h-1.5 rounded-full bg-theme-primary/40"></div>
                                            <span className="text-[11px] font-bold text-theme-secondary">{detail.standardName}</span>
                                        </div>
                                        <div className="flex items-center gap-2">
                                            <TrustBadge trust={trustByLocalId[detail.localInstanceId]} size="sm" />
                                            <span className="text-[11px] font-black text-theme-primary">
                                                {detail.amount} {detail.unit}
                                            </span>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        </div>
                    )}
                </div>

                {/* Footer Actions */}
                <div className="flex flex-col items-center gap-4">
                    <Button 
                        size="lg" 
                        onClick={onDone} 
                        className="w-full max-w-sm py-4 rounded-3xl shadow-premium-lg text-lg gap-2 cursor-pointer"
                    >
                        {t('transfer.receiveSuccess.backToDashboard')}
                        <ArrowRight size={20} />
                    </Button>
                    {!isPendingCommit && (
                        <p className="text-[10px] font-black uppercase tracking-widest text-theme-light flex items-center gap-2">
                            <ShieldCheck size={12} />
                            {t('transfer.receiveSuccess.walletUpdated')}
                        </p>
                    )}
                </div>
            </div>
        </div>
    );
}