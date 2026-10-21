// src/components/ReceiveSuccessView.tsx
import { useEffect, useState } from "react";
import { ReceiveSuccessPayload, TrustStatus, L2QuorumInfo } from "../types";
import { Button } from "./ui/Button";
import { L2QuorumBadge } from "./ui/L2QuorumBadge";
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';
import { profileService } from "../services/profileService";
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
    onDone: () => void;
}

export function ReceiveSuccessView({ payload, onDone }: ReceiveSuccessViewProps) {
    const { t } = useTranslation();
    const [trustStatus, setTrustStatus] = useState<TrustStatus>("clean");

    function formatAmount(amountStr: string): string {
        const num = parseFloat(amountStr);
        if (isNaN(num)) return amountStr;
        return num.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    }

    const summable = Object.entries(payload.transferSummary.summableAmounts)
        .map(([unit, total]) => `${formatAmount(total)} ${unit}`);

    const countable = Object.entries(payload.transferSummary.countableItems)
        .map(([unit, total]) => t('transfer.receiveSuccess.countableItem', { count: total, unit }));

    const summaryString = [...summable, ...countable].join(', ') || t('transfer.receiveSuccess.noAssets');

    useEffect(() => {
        logger.info(`Receive success screen: ${summaryString}`);
        if (payload.senderId) {
            profileService.checkReputation(payload.senderId)
                .then(setTrustStatus)
                .catch(e => logger.error(`Reputation check error: ${stringifyError(e)}`));
        }
    }, [payload, summaryString]);

    const [manualApprovalGranted, setManualApprovalGranted] = useState(false);

    const isVictim = payload.conflictSummaries 
        ? payload.conflictSummaries.some(c => c.conflictRole === 'victim' && !c.isResolved && !c.localOverride) 
        : (payload.verifiableConflicts && Object.keys(payload.verifiableConflicts).length > 0);
    
    const isWitnessOnly = payload.conflictSummaries 
        ? (payload.conflictSummaries.some(c => c.conflictRole === 'witness' && !c.isResolved && !c.localOverride) && !isVictim) 
        : false;

    const checkoutVerdict = payload.checkoutVerdict;
    const outcome = checkoutVerdict?.outcome;
    const isLiveConfirmed = checkoutVerdict?.canReleaseGoodsAutomatically === true && outcome === 'Confirmed';
    const isConfirmed = isLiveConfirmed;
    const isQuarantined = isVictim || (checkoutVerdict ? checkoutVerdict.outcome.startsWith('Quarantined') : false);
    const isProvisional = outcome === 'Provisional' && checkoutVerdict?.canReleaseGoodsAutomatically === true;

    // Only a verified Confirmed checkout verdict with full 14/20 live quorum can yield 'final'.
    // Unverified bundle import or raw response DTOs must never confirm payment or release goods automatically.
    const l2Quorum: L2QuorumInfo = isQuarantined
        ? { status: 'conflict', details: t('transfer.receiveSuccess.conflictDesc') }
        : isLiveConfirmed
            ? { status: 'final', signerCount: 14, activeNodesCount: 20, details: t('transfer.l2.finalHint') }
            : isProvisional
                ? { status: 'provisional', details: t('transfer.l2.provisionalHint') }
                : { status: 'unverified' };

    return (
        <div className="flex flex-col items-center justify-center min-h-[80vh] py-12 px-4 animate-in fade-in zoom-in-95 duration-500">
            <div className="w-full max-w-xl space-y-8">
                {/* Status Hero */}
                <div className="text-center space-y-4">
                    <div className="relative inline-block">
                        <div className={`p-6 rounded-[32px] ${isVictim ? 'bg-rose-500 text-white' : isWitnessOnly ? 'bg-amber-500 text-white' : 'bg-emerald-500 text-white'} shadow-premium-lg animate-bounce-subtle`}>
                            {isVictim ? <ShieldAlert size={64} /> : isWitnessOnly ? <AlertTriangle size={64} /> : <CheckCircle2 size={64} />}
                        </div>
                        <div className="absolute -bottom-2 -right-2 bg-white p-1.5 rounded-full shadow-md">
                            <div className={`${isVictim ? 'bg-rose-500' : isWitnessOnly ? 'bg-amber-500' : 'bg-emerald-500'} w-4 h-4 rounded-full border-2 border-white`}></div>
                        </div>
                    </div>
                    <div className="space-y-1">
                        <h1 className="text-3xl font-black text-theme-primary tracking-tight">
                            {isVictim ? t('transfer.receiveSuccess.securityWarning') : t('transfer.receiveSuccess.transferComplete')}
                        </h1>
                        <p className="text-theme-light font-medium">
                            {isVictim 
                                ? t('transfer.receiveSuccess.securityIssueAffects') 
                                : isWitnessOnly
                                    ? t('transfer.receiveSuccess.witnessOnlyDesc')
                                    : t('transfer.receiveSuccess.normalSuccessDesc')}
                        </p>
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
                    />
                </div>

                {/* Checkout & Goods Release (Warenfreigabe) Status */}
                {checkoutVerdict && (
                    <div data-testid="goods-release-section" className="space-y-3">
                    {isConfirmed ? (
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
                    ) : isQuarantined ? (
                        <div data-testid="goods-release-card" className="p-5 bg-rose-50 border-2 border-rose-200 rounded-[28px] flex items-center gap-3 shadow-sm">
                            <ShieldAlert className="text-rose-600 shrink-0" size={28} />
                            <div>
                                <h3 className="text-sm font-black text-rose-950 uppercase tracking-wider">{t('transfer.checkout.goodsBlocked', 'WARE NICHT HERAUSGEBEN!')}</h3>
                                <p className="text-xs text-rose-800 font-medium">{t('transfer.checkout.goodsBlockedDesc', 'Doppelausgabe erkannt! Dieser Gutschein wurde bereits anderweitig ausgegeben.')}</p>
                            </div>
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
                                    <Button
                                        data-testid="merchant-approve-btn"
                                        variant="secondary"
                                        size="sm"
                                        onClick={() => setManualApprovalGranted(true)}
                                        className="w-full py-2.5 rounded-2xl text-xs font-bold bg-slate-700 hover:bg-slate-800 text-white border-none shadow-sm cursor-pointer"
                                    >
                                        {t('transfer.checkout.acceptOnTrustBtn', 'Zahlung auf Vertrauensbasis annehmen')}
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
                                    {payload.senderProfileName || t('transfer.receiveSuccess.unknownSender')}
                                </p>
                                <p className="text-[10px] font-mono font-bold text-theme-light truncate mt-1">
                                    {payload.senderId || t('transfer.receiveSuccess.noDidDisclosed')}
                                </p>
                            </div>
                            
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
                                    {payload.notes ? `"${payload.notes}"` : t('transfer.receiveSuccess.noMemo')}
                                </p>
                            </div>
                        </div>
                    </div>

                    {/* Breakdown */}
                    {payload.involvedVouchersDetails && payload.involvedVouchersDetails.length > 0 && (
                        <div className="space-y-3">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5">
                                <Layers size={10} /> {t('transfer.receiveSuccess.includedVouchers', { count: payload.involvedVouchersDetails.length })}
                            </span>
                            <div className="grid grid-cols-1 gap-2">
                                {payload.involvedVouchersDetails.map((detail, index) => (
                                    <div key={index} className="flex items-center justify-between p-3 bg-white border border-theme-subtle rounded-xl shadow-sm">
                                        <div className="flex items-center gap-2">
                                            <div className="w-1.5 h-1.5 rounded-full bg-theme-primary/40"></div>
                                            <span className="text-[11px] font-bold text-theme-secondary">{detail.standardName}</span>
                                        </div>
                                        <span className="text-[11px] font-black text-theme-primary">
                                            {detail.amount} {detail.unit}
                                        </span>
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
                    <p className="text-[10px] font-black uppercase tracking-widest text-theme-light flex items-center gap-2">
                        <ShieldCheck size={12} />
                        {t('transfer.receiveSuccess.walletUpdated')}
                    </p>
                </div>
            </div>
        </div>
    );
}