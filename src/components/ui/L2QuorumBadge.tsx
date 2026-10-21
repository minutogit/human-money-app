// src/components/ui/L2QuorumBadge.tsx
import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { 
    Check,
    CheckCheck,
    ShieldAlert, 
    WifiOff, 
    Info, 
    ChevronDown, 
    ChevronUp,
    ShieldCheck
} from "lucide-react";
import { L2QuorumInfo, L2VerdictState } from "../../types";

export interface L2QuorumBadgeProps {
    quorum?: L2QuorumInfo;
    status?: L2VerdictState;
    signerCount?: number;
    activeNodesCount?: number;
    shardId?: number;
    details?: string;
    size?: 'sm' | 'md' | 'lg';
    showDetails?: boolean;
    className?: string;
    isVerifiedCheckout?: boolean;
    pillarLabel?: string;
}

interface StatusConfig {
    labelKey: string;
    defaultLabel: string;
    sublabelKey: string;
    defaultSublabel: string;
    merchantHintKey: string;
    defaultMerchantHint: string;
    badgeStyle: string;
    pillStyle: string;
    borderStyle: string;
    textStyle: string;
    iconStyle: string;
    dotColor: string;
    levelDots: string;
    Icon: React.ComponentType<{ size?: number; className?: string }>;
}

const STATUS_CONFIGS: Record<L2VerdictState, StatusConfig> = {
    final: {
        labelKey: "transfer.l2.finalTitle",
        defaultLabel: "Maximale Sicherheit",
        sublabelKey: "transfer.l2.finalSub",
        defaultSublabel: "Großes Server-Netz",
        merchantHintKey: "transfer.l2.finalHint",
        defaultMerchantHint: "Ein großes, weltweites Server-Netzwerk hat die Zahlung final abgesichert. Höchste Sicherheitsstufe – ein Betrug ist praktisch unmöglich.",
        badgeStyle: "bg-emerald-50 border-emerald-200 text-emerald-950 shadow-emerald-500/10",
        pillStyle: "bg-emerald-600 text-white",
        borderStyle: "border-emerald-300",
        textStyle: "text-emerald-800",
        iconStyle: "text-emerald-600",
        dotColor: "bg-emerald-500",
        levelDots: "●●",
        Icon: CheckCheck,
    },
    provisional: {
        labelKey: "transfer.l2.provisionalTitle",
        defaultLabel: "Hohe Sicherheit",
        sublabelKey: "transfer.l2.provisionalSub",
        defaultSublabel: "Kleines Server-Netz",
        merchantHintKey: "transfer.l2.provisionalHint",
        defaultMerchantHint: "Ein kleineres Server-Netzwerk hat die Zahlung geprüft und abgesichert. Hohe Sicherheit – ein Betrug ist extrem unwahrscheinlich. Ware kann problemlos herausgegeben werden.",
        badgeStyle: "bg-emerald-50/70 border-emerald-200/90 text-emerald-900 shadow-emerald-500/5",
        pillStyle: "bg-emerald-500 text-white",
        borderStyle: "border-emerald-200",
        textStyle: "text-emerald-700",
        iconStyle: "text-emerald-500",
        dotColor: "bg-emerald-400",
        levelDots: "●○",
        Icon: Check,
    },
    conflict: {
        labelKey: "transfer.l2.conflictTitle",
        defaultLabel: "Doppelausgabe erkannt!",
        sublabelKey: "transfer.l2.conflictSub",
        defaultSublabel: "Zahlung abgelehnt",
        merchantHintKey: "transfer.l2.conflictHint",
        defaultMerchantHint: "Alarm: Doppelausgabe! Dieser Gutschein wurde bereits früher oder an einem anderen Ort ausgegeben. Ware keinesfalls herausgeben!",
        badgeStyle: "bg-rose-50 border-rose-200 text-rose-900 shadow-rose-500/10 animate-pulse-subtle",
        pillStyle: "bg-rose-600 text-white",
        borderStyle: "border-rose-300",
        textStyle: "text-rose-800",
        iconStyle: "text-rose-600",
        dotColor: "bg-rose-500",
        levelDots: "●",
        Icon: ShieldAlert,
    },
    offline: {
        labelKey: "transfer.l2.offlineTitle",
        defaultLabel: "Offline-Zahlung",
        sublabelKey: "transfer.l2.offlineSub",
        defaultSublabel: "Direktübertragung (ohne Netzabgleich)",
        merchantHintKey: "transfer.l2.offlineHint",
        defaultMerchantHint: "Keine Server-Verbindung. Die Zahlung wurde direkt von Gerät zu Gerät übertragen. Empfohlen bei persönlichem Vertrauen (z. B. Stammkunden, Kleinstbeträge).",
        badgeStyle: "bg-slate-50 border-slate-200 text-slate-800 shadow-slate-500/10",
        pillStyle: "bg-slate-500 text-white",
        borderStyle: "border-slate-300",
        textStyle: "text-slate-700",
        iconStyle: "text-slate-500",
        dotColor: "bg-slate-400",
        levelDots: "○○",
        Icon: WifiOff,
    },
    unverified: {
        labelKey: "transfer.l2.unverifiedTitle",
        defaultLabel: "Offline-Zahlung",
        sublabelKey: "transfer.l2.unverifiedSub",
        defaultSublabel: "Nicht im Netzwerk geprüft",
        merchantHintKey: "transfer.l2.unverifiedHint",
        defaultMerchantHint: "Keine Server-Verbindung. Die Zahlung wurde direkt von Gerät zu Gerät übertragen. Empfohlen bei persönlichem Vertrauen (z. B. Stammkunden, Kleinstbeträge).",
        badgeStyle: "bg-slate-50 border-slate-200 text-slate-700 shadow-slate-500/10",
        pillStyle: "bg-slate-500 text-white",
        borderStyle: "border-slate-300",
        textStyle: "text-slate-700",
        iconStyle: "text-slate-500",
        dotColor: "bg-slate-400",
        levelDots: "○○",
        Icon: WifiOff,
    },
};

export const L2QuorumBadge: React.FC<L2QuorumBadgeProps> = ({
    quorum,
    status,
    signerCount,
    activeNodesCount,
    shardId,
    details,
    size = "md",
    showDetails = false,
    className = "",
    isVerifiedCheckout = false,
    pillarLabel,
}) => {
    const { t } = useTranslation();
    const [isExpanded, setIsExpanded] = useState<boolean>(showDetails);
    const [showTechDetails, setShowTechDetails] = useState<boolean>(false);

    // These public props are presentation data, not a verifier result. Only when
    // verified checkout integration supplies a trusted boundary (isVerifiedCheckout),
    // confirmation claims may produce final/provisional badges.
    const claimedStatus = quorum?.status ?? status ?? "unverified";
    const unsupportedConfirmation = !isVerifiedCheckout && (claimedStatus === 'final' || claimedStatus === 'provisional');
    const effectiveStatus: L2VerdictState = unsupportedConfirmation ? 'unverified' : claimedStatus;
    const effectiveSignerCount = unsupportedConfirmation ? undefined : quorum?.signerCount ?? signerCount;
    const effectiveActiveNodes = unsupportedConfirmation ? undefined : quorum?.activeNodesCount ?? activeNodesCount;
    const effectiveShardId = unsupportedConfirmation ? undefined : quorum?.shardId ?? shardId;
    const effectiveDetails = unsupportedConfirmation ? undefined : quorum?.details ?? details;

    const config = STATUS_CONFIGS[effectiveStatus] || STATUS_CONFIGS.unverified;
    const IconComponent = config.Icon;

    const label = t(config.labelKey, config.defaultLabel);
    const sublabel = t(config.sublabelKey, config.defaultSublabel);
    const merchantHint = t(config.merchantHintKey, config.defaultMerchantHint);

    const sizeClasses = {
        sm: {
            container: "p-2.5 rounded-2xl text-xs",
            icon: 16,
            badge: "text-[10px] px-2 py-0.5",
            dot: "w-2 h-2",
        },
        md: {
            container: "p-3.5 rounded-3xl text-sm",
            icon: 20,
            badge: "text-xs px-2.5 py-1",
            dot: "w-2.5 h-2.5",
        },
        lg: {
            container: "p-4.5 rounded-[32px] text-base",
            icon: 26,
            badge: "text-sm px-3 py-1.5",
            dot: "w-3 h-3",
        },
    }[size];

    const quorumRatioText = effectiveSignerCount !== undefined
        ? `${effectiveSignerCount}${effectiveActiveNodes !== undefined ? `/${effectiveActiveNodes}` : ''} Knoten`
        : null;

    const percentage = (effectiveSignerCount !== undefined && effectiveActiveNodes !== undefined && effectiveActiveNodes > 0)
        ? Math.round((effectiveSignerCount / effectiveActiveNodes) * 100)
        : null;

    return (
        <div 
            data-testid="l2-quorum-badge"
            className={`border shadow-sm transition-all duration-300 ${config.badgeStyle} ${sizeClasses.container} ${className}`}
        >
            {/* Main Header / Bar */}
            <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-2.5 min-w-0">
                    <div className={`p-2 rounded-xl bg-white shadow-xs flex items-center justify-center shrink-0 ${config.borderStyle} border`}>
                        <IconComponent size={sizeClasses.icon} className={config.iconStyle} />
                    </div>

                    <div className="min-w-0">
                        {pillarLabel && (
                            <span className="text-[10px] font-bold uppercase tracking-wider text-theme-secondary/80 block mb-0.5">
                                {pillarLabel}
                            </span>
                        )}
                        <div className="flex items-center gap-2 flex-wrap">
                            <span className="font-black tracking-tight text-theme-primary truncate">
                                {label}
                            </span>
                            <span 
                                className={`inline-flex items-center gap-1.5 rounded-full font-bold uppercase tracking-wider ${sizeClasses.badge} ${config.pillStyle}`}
                                data-testid="l2-status-pill"
                            >
                                <span className="font-mono text-[11px] tracking-tight" aria-hidden="true">
                                    {config.levelDots}
                                </span>
                                <span>{effectiveStatus}</span>
                            </span>
                        </div>
                        <p className={`text-[11px] font-medium truncate ${config.textStyle}`}>
                            {sublabel}
                        </p>
                    </div>
                </div>

                <div className="flex items-center gap-2 shrink-0">
                    {quorumRatioText && (
                        <div 
                            className="bg-white/80 backdrop-blur-xs border border-theme-subtle/40 px-2.5 py-1 rounded-xl shadow-2xs text-center"
                            data-testid="l2-quorum-count"
                        >
                            <span className="text-[11px] font-mono font-bold text-theme-primary">
                                {quorumRatioText}
                            </span>
                        </div>
                    )}

                    <button
                        type="button"
                        onClick={() => setIsExpanded(!isExpanded)}
                        aria-expanded={isExpanded}
                        aria-label="Layer-2 Details anzeigen"
                        data-testid="l2-toggle-details"
                        className="p-1.5 rounded-xl hover:bg-white/80 active:bg-white text-theme-secondary transition-colors cursor-pointer flex items-center gap-1"
                        title={t("transfer.l2.infoTooltip", "Informationen zum Netzwerkschutz")}
                    >
                        <Info size={16} />
                        {isExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                    </button>
                </div>
            </div>

            {/* Collapsible / Expandable Merchant Infobox */}
            {isExpanded && (
                <div 
                    data-testid="l2-merchant-infobox"
                    className="mt-3 pt-3 border-t border-current/15 text-xs space-y-2.5 animate-in fade-in slide-in-from-top-2 duration-200"
                >
                    {/* Merchant Action Advice (Layman Explanation) */}
                    <div className="p-2.5 rounded-xl bg-white/75 border border-current/15 flex items-start gap-2">
                        <Info size={14} className="shrink-0 mt-0.5 opacity-75" />
                        <div className="space-y-0.5">
                            <p className="font-bold text-[10px] uppercase tracking-wider opacity-90">
                                {t("transfer.l2.merchantGuidance", "Kassierer-Hinweis")}
                            </p>
                            <p className="text-[11px] leading-relaxed font-medium">
                                {merchantHint}
                            </p>
                        </div>
                    </div>

                    {/* Collapsible Technical Details: "Technische Diagnose" */}
                    {(effectiveSignerCount !== undefined || effectiveShardId !== undefined || effectiveDetails) && (
                        <div className="pt-1">
                            <button
                                type="button"
                                onClick={() => setShowTechDetails(!showTechDetails)}
                                className="flex items-center gap-1.5 text-[10px] font-bold text-theme-secondary hover:text-theme-primary transition-colors cursor-pointer"
                                data-testid="l2-toggle-tech-diagnostics"
                            >
                                {showTechDetails ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                                <span>{t("transfer.l2.technicalDiagnostics", "Technische Diagnose")}</span>
                            </button>

                            {showTechDetails && (
                                <div className="mt-2 space-y-2 p-2.5 rounded-xl bg-black/5 border border-current/10 text-[10px] animate-in fade-in duration-200" data-testid="l2-tech-details">
                                    {percentage !== null && (
                                        <div className="space-y-1">
                                            <div className="flex justify-between font-bold text-theme-secondary">
                                                <span>Shard-Bestätigung</span>
                                                <span className="font-mono">{effectiveSignerCount}/{effectiveActiveNodes} ({percentage}%)</span>
                                            </div>
                                            <div className="w-full bg-black/10 rounded-full h-1.5 overflow-hidden">
                                                <div 
                                                    className={`h-full rounded-full transition-all duration-500 ${
                                                        effectiveStatus === 'conflict' ? 'bg-rose-500' : 'bg-emerald-500'
                                                    }`}
                                                    style={{ width: `${Math.min(100, Math.max(0, percentage))}%` }}
                                                />
                                            </div>
                                        </div>
                                    )}

                                    <div className="flex items-center gap-3 font-medium opacity-85">
                                        {effectiveShardId !== undefined && (
                                            <span className="bg-white/70 px-2 py-0.5 rounded-md border border-current/10 font-mono">
                                                Shard #{effectiveShardId}
                                            </span>
                                        )}
                                        <span className="flex items-center gap-1">
                                            <ShieldCheck size={12} />
                                            14/20 BFT Shard-Quorum
                                        </span>
                                    </div>

                                    {effectiveDetails && (
                                        <p className="font-mono opacity-75 italic">
                                            {effectiveDetails}
                                        </p>
                                    )}
                                </div>
                            )}
                        </div>
                    )}
                </div>
            )}
        </div>
    );
};

export default L2QuorumBadge;
