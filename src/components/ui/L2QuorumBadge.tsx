// src/components/ui/L2QuorumBadge.tsx
import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { 
    CheckCircle2, 
    AlertTriangle, 
    ShieldAlert, 
    WifiOff, 
    HelpCircle, 
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
    Icon: React.ComponentType<{ size?: number; className?: string }>;
}

const STATUS_CONFIGS: Record<L2VerdictState, StatusConfig> = {
    final: {
        labelKey: "transfer.l2.finalTitle",
        defaultLabel: "Final gesichert",
        sublabelKey: "transfer.l2.finalSub",
        defaultSublabel: "Shard-Quorum erreicht",
        merchantHintKey: "transfer.l2.finalHint",
        defaultMerchantHint: "Vollständiges Shard-Quorum erreicht. Zahlung ist kryptografisch unwiderruflich und final abgesichert. Ware kann sicher ausgehändigt werden.",
        badgeStyle: "bg-emerald-50 border-emerald-200 text-emerald-900 shadow-emerald-500/10",
        pillStyle: "bg-emerald-600 text-white",
        borderStyle: "border-emerald-300",
        textStyle: "text-emerald-800",
        iconStyle: "text-emerald-600",
        dotColor: "bg-emerald-500",
        Icon: CheckCircle2,
    },
    provisional: {
        labelKey: "transfer.l2.provisionalTitle",
        defaultLabel: "Vorläufig gesichert",
        sublabelKey: "transfer.l2.provisionalSub",
        defaultSublabel: "Vorläufiges Quorum registriert",
        merchantHintKey: "transfer.l2.provisionalHint",
        defaultMerchantHint: "Vorläufiges Quorum registriert. Zahlung ist im Layer-2-Netzwerk vermerkt, wartet aber noch auf finale Shard-Bestätigung.",
        badgeStyle: "bg-amber-50 border-amber-200 text-amber-900 shadow-amber-500/10",
        pillStyle: "bg-amber-500 text-white",
        borderStyle: "border-amber-300",
        textStyle: "text-amber-800",
        iconStyle: "text-amber-600",
        dotColor: "bg-amber-500",
        Icon: AlertTriangle,
    },
    conflict: {
        labelKey: "transfer.l2.conflictTitle",
        defaultLabel: "Double-Spend Warnung",
        sublabelKey: "transfer.l2.conflictSub",
        defaultSublabel: "Kollision im Layer 2 erkannt",
        merchantHintKey: "transfer.l2.conflictHint",
        defaultMerchantHint: "ACHTUNG: Double-Spend erkannt! Dieser Gutschein wurde bereits anderweitig ausgegeben. Ware keinesfalls herausgeben!",
        badgeStyle: "bg-rose-50 border-rose-200 text-rose-900 shadow-rose-500/10 animate-pulse-subtle",
        pillStyle: "bg-rose-600 text-white",
        borderStyle: "border-rose-300",
        textStyle: "text-rose-800",
        iconStyle: "text-rose-600",
        dotColor: "bg-rose-500",
        Icon: ShieldAlert,
    },
    offline: {
        labelKey: "transfer.l2.offlineTitle",
        defaultLabel: "Offline-Prüfung",
        sublabelKey: "transfer.l2.offlineSub",
        defaultSublabel: "Peer-to-Peer Nachweis",
        merchantHintKey: "transfer.l2.offlineHint",
        defaultMerchantHint: "Layer-2 Sperrregister offline oder nicht erreichbar. Die Prüfung erfolgte rein lokal auf Basis der Kausalitätskette.",
        badgeStyle: "bg-slate-50 border-slate-200 text-slate-800 shadow-slate-500/10",
        pillStyle: "bg-slate-600 text-white",
        borderStyle: "border-slate-300",
        textStyle: "text-slate-700",
        iconStyle: "text-slate-500",
        dotColor: "bg-slate-400",
        Icon: WifiOff,
    },
    unverified: {
        labelKey: "transfer.l2.unverifiedTitle",
        defaultLabel: "Offline-Prüfung",
        sublabelKey: "transfer.l2.unverifiedSub",
        defaultSublabel: "Nicht geprüft",
        merchantHintKey: "transfer.l2.unverifiedHint",
        defaultMerchantHint: "Keine Layer-2 Prüfung erfolgt.",
        badgeStyle: "bg-slate-50 border-slate-200 text-slate-800 shadow-slate-500/10",
        pillStyle: "bg-slate-600 text-white",
        borderStyle: "border-slate-300",
        textStyle: "text-slate-700",
        iconStyle: "text-slate-500",
        dotColor: "bg-slate-400",
        Icon: HelpCircle,
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
}) => {
    const { t } = useTranslation();
    const [isExpanded, setIsExpanded] = useState<boolean>(showDetails);

    const effectiveStatus: L2VerdictState = quorum?.status ?? status ?? "unverified";
    const effectiveSignerCount = quorum?.signerCount ?? signerCount;
    const effectiveActiveNodes = quorum?.activeNodesCount ?? activeNodesCount;
    const effectiveShardId = quorum?.shardId ?? shardId;
    const effectiveDetails = quorum?.details ?? details;

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
                        <div className="flex items-center gap-2 flex-wrap">
                            <span className="font-black tracking-tight text-theme-primary truncate">
                                {label}
                            </span>
                            <span 
                                className={`inline-flex items-center gap-1 rounded-full font-bold uppercase tracking-wider ${sizeClasses.badge} ${config.pillStyle}`}
                                data-testid="l2-status-pill"
                            >
                                <span className={`${sizeClasses.dot} rounded-full bg-white animate-pulse`} />
                                {effectiveStatus}
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
                        className="p-1.5 rounded-xl hover:bg-white/80 active:bg-white text-theme-secondary transition-colors cursor-pointer"
                    >
                        {isExpanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
                    </button>
                </div>
            </div>

            {/* Collapsible / Expandable Merchant Infobox */}
            {isExpanded && (
                <div 
                    data-testid="l2-merchant-infobox"
                    className="mt-3 pt-3 border-t border-current/15 text-xs space-y-2.5 animate-in fade-in slide-in-from-top-2 duration-200"
                >
                    {/* Node Quorum Progress (if counts available) */}
                    {percentage !== null && (
                        <div className="space-y-1">
                            <div className="flex justify-between text-[10px] font-bold text-theme-secondary">
                                <span>Shard-Bestätigung</span>
                                <span className="font-mono">{effectiveSignerCount}/{effectiveActiveNodes} ({percentage}%)</span>
                            </div>
                            <div className="w-full bg-black/10 rounded-full h-1.5 overflow-hidden">
                                <div 
                                    className={`h-full rounded-full transition-all duration-500 ${
                                        effectiveStatus === 'final' 
                                            ? 'bg-emerald-500' 
                                            : effectiveStatus === 'conflict' 
                                                ? 'bg-rose-500' 
                                                : 'bg-amber-500'
                                    }`}
                                    style={{ width: `${Math.min(100, Math.max(0, percentage))}%` }}
                                />
                            </div>
                        </div>
                    )}

                    {/* Shard & Node Info metadata */}
                    <div className="flex items-center gap-3 text-[10px] font-medium opacity-85">
                        {effectiveShardId !== undefined && (
                            <span className="bg-white/70 px-2 py-0.5 rounded-md border border-current/10 font-mono">
                                Shard #{effectiveShardId}
                            </span>
                        )}
                        <span className="flex items-center gap-1">
                            <ShieldCheck size={12} />
                            Layer-2 Sperrregister
                        </span>
                    </div>

                    {/* Merchant Action Advice */}
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

                    {effectiveDetails && (
                        <p className="text-[10px] font-mono opacity-75 italic px-1">
                            {effectiveDetails}
                        </p>
                    )}
                </div>
            )}
        </div>
    );
};

export default L2QuorumBadge;
