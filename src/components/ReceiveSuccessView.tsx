// src/components/ReceiveSuccessView.tsx
import { useEffect, useState } from "react";
import { ReceiveSuccessPayload, TrustStatus } from "../types";
import { Button } from "./ui/Button";
import { logger } from '../utils/log';
import { profileService } from "../services/profileService";
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
    Info
} from "lucide-react";

interface ReceiveSuccessViewProps {
    payload: ReceiveSuccessPayload;
    onDone: () => void;
}

function formatAmount(amountStr: string): string {
    const num = parseFloat(amountStr);
    if (isNaN(num)) return amountStr;
    return num.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

export function ReceiveSuccessView({ payload, onDone }: ReceiveSuccessViewProps) {
    const [trustStatus, setTrustStatus] = useState<TrustStatus>("clean");

    const summable = Object.entries(payload.transferSummary.summableAmounts)
        .map(([unit, total]) => `${formatAmount(total)} ${unit}`);

    const countable = Object.entries(payload.transferSummary.countableItems)
        .map(([unit, total]) => `${total} ${unit}${total > 1 ? 's' : ''}`);

    const summaryString = [...summable, ...countable].join(', ') || 'No assets detected';

    useEffect(() => {
        logger.info(`Receive success screen: ${summaryString}`);
        if (payload.senderId) {
            profileService.checkReputation(payload.senderId)
                .then(setTrustStatus)
                .catch(e => logger.error(`Reputation check error: ${e}`));
        }
    }, [payload, summaryString]);

    const isVictim = payload.conflictSummaries 
        ? payload.conflictSummaries.some(c => c.conflictRole === 'victim' && !c.isResolved && !c.localOverride) 
        : (payload.verifiableConflicts && Object.keys(payload.verifiableConflicts).length > 0);
    
    const isWitnessOnly = payload.conflictSummaries 
        ? (payload.conflictSummaries.some(c => c.conflictRole === 'witness' && !c.isResolved && !c.localOverride) && !isVictim) 
        : false;

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
                            {isVictim ? 'Security Warning' : 'Transfer Complete'}
                        </h1>
                        <p className="text-theme-light font-medium">
                            {isVictim 
                                ? 'A security issue affects your new assets.' 
                                : isWitnessOnly
                                    ? 'Assets added. Your wallet also helped the network.'
                                    : 'Assets have been added to your wallet.'}
                        </p>
                    </div>
                </div>

                {/* Conflict Alert (if any) */}
                {isVictim && (
                    <div className="p-6 bg-rose-50 border-2 border-rose-100 rounded-[32px] flex items-start gap-4 shadow-sm animate-in shake duration-1000">
                        <AlertTriangle className="text-rose-500 shrink-0 mt-1" size={24} />
                        <div>
                            <h3 className="text-sm font-black text-rose-900 uppercase tracking-widest mb-1">Security Warning: Problem with your Balance</h3>
                            <p className="text-xs text-rose-800 font-medium leading-relaxed">
                                Our security system has detected that someone tried to spend the same money twice. One of the vouchers you just received is affected and has been safely 'quarantined' for your protection.
                            </p>
                        </div>
                    </div>
                )}

                {isWitnessOnly && (
                    <div className="p-6 bg-amber-50 border-2 border-amber-100 rounded-[32px] flex items-start gap-4 shadow-sm animate-in fade-in duration-700">
                        <Info className="text-amber-500 shrink-0 mt-1" size={24} />
                        <div>
                            <h3 className="text-sm font-black text-amber-900 uppercase tracking-widest mb-1">Security Insight: Fraud Detected Elsewhere</h3>
                            <p className="text-xs text-amber-800 font-medium leading-relaxed">
                                Your own balance is <strong>completely safe and unaffected</strong>. However, while processing this transfer, your wallet discovered a fraud attempt happening elsewhere in the network. This information helps keep the community safe.
                            </p>
                        </div>
                    </div>
                )}

                {/* Transfer Details Card */}
                <div className="bg-white border border-theme-subtle rounded-[40px] p-8 shadow-premium space-y-8">
                    {/* Amount Hero */}
                    <div className="text-center space-y-2">
                        <span className="text-[10px] font-black uppercase tracking-[0.2em] text-theme-light">Total Received</span>
                        <div className="text-4xl font-black text-theme-primary tracking-tighter">
                            {summaryString}
                        </div>
                    </div>

                    <div className="grid grid-cols-1 sm:grid-cols-2 gap-6">
                        {/* Sender Info */}
                        <div className="space-y-3">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5">
                                <User size={10} /> Sender
                            </span>
                            <div className="p-4 bg-theme-subtle/10 rounded-2xl border border-theme-subtle/20">
                                <p className="text-sm font-black text-theme-primary truncate">
                                    {payload.senderProfileName || 'Unknown Sender'}
                                </p>
                                <p className="text-[10px] font-mono font-bold text-theme-light truncate mt-1">
                                    {payload.senderId || 'No DID disclosed'}
                                </p>
                            </div>
                            
                            {/* Reputation Warning */}
                            {typeof trustStatus === 'object' && 'knownOffender' in trustStatus && (
                                <div className="flex items-center gap-2 px-3 py-2 bg-rose-50 rounded-xl border border-rose-100">
                                    <UserX size={14} className="text-rose-500" />
                                    <span className="text-[10px] font-bold text-rose-700">Flagged for past conflicts</span>
                                </div>
                            )}
                        </div>

                        {/* Notes Info */}
                        <div className="space-y-3">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5">
                                <MessageSquare size={10} /> Note
                            </span>
                            <div className="p-4 bg-theme-subtle/10 rounded-2xl border border-theme-subtle/20 h-[72px] overflow-y-auto no-scrollbar">
                                <p className="text-xs font-medium text-theme-secondary italic leading-relaxed">
                                    {payload.notes ? `"${payload.notes}"` : 'No memo attached.'}
                                </p>
                            </div>
                        </div>
                    </div>

                    {/* Breakdown */}
                    {payload.involvedVouchersDetails && payload.involvedVouchersDetails.length > 0 && (
                        <div className="space-y-3">
                            <span className="text-[9px] font-black uppercase tracking-widest text-theme-light flex items-center gap-1.5">
                                <Layers size={10} /> Included Vouchers ({payload.involvedVouchersDetails.length})
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
                        className="w-full max-w-sm py-4 rounded-3xl shadow-premium-lg text-lg gap-2"
                    >
                        Back to Dashboard
                        <ArrowRight size={20} />
                    </Button>
                    <p className="text-[10px] font-black uppercase tracking-widest text-theme-light flex items-center gap-2">
                        <ShieldCheck size={12} />
                        Wallet updated
                    </p>
                </div>
            </div>
        </div>
    );
}