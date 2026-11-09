// src/components/RealtimeProtectionBadge.tsx
import React, { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { 
    CheckCircle2, 
    Shield, 
    ShieldAlert, 
    RefreshCw, 
    AlertTriangle, 
    Check, 
    X,
    Network
} from "lucide-react";
import { settingsService, AnchorSyncStatus, RollbackReport } from "../services/settingsService";
import { useSession } from "../context/SessionContext";
import { useNavigation } from "../context/NavigationContext";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";

export interface RealtimeProtectionBadgeProps {
    className?: string;
    compact?: boolean;
}

export const RealtimeProtectionBadge: React.FC<RealtimeProtectionBadgeProps> = ({
    className = "",
    compact = false,
}) => {
    const { t } = useTranslation();
    const { navigateToSettingsSection } = useNavigation();
    const { isSessionActive, protectAction, triggerSecurityLockdown } = useSession();

    const [status, setStatus] = useState<AnchorSyncStatus>("Offline");
    const [isChecking, setIsChecking] = useState(false);
    const [isOpen, setIsOpen] = useState(false);
    const [isReconciling, setIsReconciling] = useState(false);
    const [reconcileReport, setReconcileReport] = useState<RollbackReport | null>(null);
    const [reconcileError, setReconcileError] = useState<string | null>(null);
    const [activeNetwork, setActiveNetwork] = useState<string | undefined>(undefined);
    const popoverRef = useRef<HTMLDivElement>(null);

    const refreshSyncStatus = useCallback(async () => {
        if (!isSessionActive) {
            setStatus("Offline");
            return;
        }

        setIsChecking(true);
        try {
            const nextStatus = await settingsService.checkAnchorSync();
            setStatus(nextStatus);
            if (nextStatus === "SupersededByHandover") {
                logger.warn("Live L2 anchor check returned SupersededByHandover");
                triggerSecurityLockdown("handover");
            } else if (nextStatus === "ConcurrentCloneDetected") {
                logger.warn("Live L2 anchor check returned ConcurrentCloneDetected");
                triggerSecurityLockdown("clone");
            } else if (nextStatus === "ForkConflict") {
                logger.warn("Live L2 anchor check returned ForkConflict");
                triggerSecurityLockdown("fork");
            } else if (nextStatus === "RollbackDetected" || nextStatus === "SameDeviceRollback") {
                logger.warn(`L2 Anchor Sync alert detected: ${nextStatus}`);
            }
        } catch (e) {
            console.debug(`Anchor sync check returned error: ${e}`);
            setStatus("Offline");
        } finally {
            setIsChecking(false);
        }
    }, [isSessionActive, triggerSecurityLockdown]);

    const refreshNetwork = useCallback(async () => {
        try {
            const s = await settingsService.getSettings();
            setActiveNetwork(s.activeNetwork || "mainnet");
        } catch {
            setActiveNetwork("mainnet");
        }
    }, []);

    useEffect(() => {
        refreshNetwork();
        const netInterval = window.setInterval(refreshNetwork, 30000);
        return () => window.clearInterval(netInterval);
    }, [refreshNetwork]);

    useEffect(() => {
        if (isSessionActive) {
            refreshSyncStatus();
            const interval = window.setInterval(refreshSyncStatus, 25000);
            return () => window.clearInterval(interval);
        } else {
            setStatus("Offline");
        }
    }, [isSessionActive, refreshSyncStatus]);

    // Close on outside click or Escape
    useEffect(() => {
        if (!isOpen) return;

        const handleOutsideClick = (e: MouseEvent) => {
            if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
                setIsOpen(false);
            }
        };

        const handleKeyDown = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                setIsOpen(false);
            }
        };

        document.addEventListener("mousedown", handleOutsideClick);
        document.addEventListener("keydown", handleKeyDown);
        return () => {
            document.removeEventListener("mousedown", handleOutsideClick);
            document.removeEventListener("keydown", handleKeyDown);
        };
    }, [isOpen]);

    const handleReconcile = async () => {
        setIsReconciling(true);
        setReconcileError(null);
        try {
            await protectAction(async (password) => {
                const report = await settingsService.reconcileRollback(password ?? undefined);
                setReconcileReport(report);
                setStatus("Synchronized");
                logger.info("Rollback successfully reconciled with L2 anchor.");
            });
        } catch (e: unknown) {
            logger.error(`Reconciliation failed: ${e}`);
            setReconcileError(e instanceof Error ? e.message : String(e));
        } finally {
            setIsReconciling(false);
        }
    };

    const handleOpenSettings = () => {
        setIsOpen(false);
        navigateToSettingsSection("network");
    };

    const isConflict = status === "RollbackDetected" || status === "ForkConflict" || status === "SameDeviceRollback" || status === "ConcurrentCloneDetected" || status === "SupersededByHandover";
    const isSameDevice = status === "SameDeviceRollback";
    const isTestnet = activeNetwork?.toLowerCase() === "testnet";

    const statusConfigs: Record<AnchorSyncStatus, {
        label: string;
        description: string;
        tooltip: string;
        containerStyle: string;
        dotStyle: string;
        Icon: React.ComponentType<{ size?: number; className?: string }>;
        iconColor: string;
    }> = {
        Synchronized: {
            label: t("wallet.protectionActive", "Echtzeitschutz aktiv"),
            description: t("wallet.protectionActiveDesc", "Verbunden mit dem HuMoCo-Node-Netzwerk (L2-Quorum). Sofortiger Schutz vor Kollisionen."),
            tooltip: t("wallet.anchor.syncTooltip", "Deine Brieftasche ist mit dem Online-Schutz abgeglichen."),
            containerStyle: "bg-emerald-50/90 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800/60 hover:bg-emerald-100/90",
            dotStyle: "bg-emerald-500",
            Icon: CheckCircle2,
            iconColor: "text-emerald-600 dark:text-emerald-400",
        },
        NoServers: {
            label: t("wallet.protectionOffline", "Offline-Schutz"),
            description: t("wallet.protectionOfflineDesc", "Keine L2-Nodes verbunden. Zahlungen sind durch mathematische Shared-Signature Traps (SST v4) sicher geschützt."),
            tooltip: t("wallet.syncNoServersHint", "Kein Online-Abgleich eingerichtet. Du kannst Gutscheine trotzdem jederzeit sicher ohne Internet senden und empfangen."),
            containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700 hover:bg-slate-200/80",
            dotStyle: "bg-slate-400",
            Icon: Shield,
            iconColor: "text-slate-500 dark:text-slate-400",
        },
        Offline: {
            label: t("wallet.protectionOffline", "Offline-Schutz"),
            description: t("wallet.protectionOfflineDesc", "Keine L2-Nodes verbunden. Zahlungen sind durch mathematische Shared-Signature Traps (SST v4) sicher geschützt."),
            tooltip: t("wallet.anchor.offlineTooltip", "Der Online-Abgleich ist gerade nicht erreichbar. Zahlungen funktionieren weiterhin ganz normal offline."),
            containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700 hover:bg-slate-200/80",
            dotStyle: "bg-slate-400",
            Icon: Shield,
            iconColor: "text-slate-500 dark:text-slate-400",
        },
        PendingLeapLock: {
            label: t("wallet.protectionChecking", "Prüfe Echtzeitschutz..."),
            description: t("wallet.protectionCheckingDesc", "Verbindung zum L2-Netzwerk wird geprüft."),
            tooltip: t("wallet.anchor.pendingTooltip", "Ausstehender Abgleich – wird automatisch nachgeholt, sobald eine Verbindung besteht."),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: Shield,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        Inconclusive: {
            label: t("wallet.protectionChecking", "Prüfe Echtzeitschutz..."),
            description: t("wallet.protectionCheckingDesc", "Verbindung zum L2-Netzwerk wird geprüft."),
            tooltip: t("wallet.anchor.offlineTooltip", "Der Online-Abgleich ist gerade nicht erreichbar. Zahlungen funktionieren weiterhin ganz normal offline."),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: Shield,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        SameDeviceRollback: {
            label: t("wallet.protectionWarning", "Sicherheitswarnung"),
            description: t("wallet.anchor.sameDeviceRollbackHint", "Ein älteres Backup ist auf diesem Gerät geladen. Klicken Sie zum nahtlosen Synchronisieren."),
            tooltip: t("wallet.anchor.sameDeviceRollbackHint", "Ein älteres Backup ist auf diesem Gerät geladen. Klicken Sie zum nahtlosen Synchronisieren."),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: RefreshCw,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        SupersededByHandover: {
            label: t("wallet.protectionWarning", "Sicherheitswarnung"),
            description: t("wallet.anchor.supersededHint", "Dieses Wallet wurde auf einem anderen Gerät aktiviert."),
            tooltip: t("wallet.anchor.supersededHint", "Dieses Wallet wurde auf einem anderen Gerät aktiviert."),
            containerStyle: "bg-rose-50 text-rose-800 border-rose-300 dark:bg-rose-950/50 dark:text-rose-200 dark:border-rose-700 hover:bg-rose-100 animate-pulse",
            dotStyle: "bg-rose-600",
            Icon: ShieldAlert,
            iconColor: "text-rose-600 dark:text-rose-400",
        },
        ConcurrentCloneDetected: {
            label: t("wallet.protectionWarning", "Sicherheitswarnung"),
            description: t("wallet.anchor.cloneHint", "Mehrere Geräte gleichzeitig aktiv. Vollständige Wiederherstellung erforderlich."),
            tooltip: t("wallet.anchor.cloneHint", "Mehrere Geräte gleichzeitig aktiv. Vollständige Wiederherstellung erforderlich."),
            containerStyle: "bg-rose-50 text-rose-800 border-rose-300 dark:bg-rose-950/50 dark:text-rose-200 dark:border-rose-700 hover:bg-rose-100 animate-pulse",
            dotStyle: "bg-rose-600",
            Icon: ShieldAlert,
            iconColor: "text-rose-600 dark:text-rose-400",
        },
        RollbackDetected: {
            label: t("wallet.protectionWarning", "Sicherheitswarnung"),
            description: t("wallet.anchor.conflictTooltip", "Sicherheitswarnung: Backup-Konflikt erkannt. Klicken zur Bereinigung."),
            tooltip: t("wallet.anchor.conflictTooltip", "Sicherheitswarnung: Backup-Konflikt erkannt. Klicken zur Bereinigung."),
            containerStyle: "bg-rose-50 text-rose-800 border-rose-300 dark:bg-rose-950/50 dark:text-rose-200 dark:border-rose-700 hover:bg-rose-100 animate-pulse",
            dotStyle: "bg-rose-600",
            Icon: ShieldAlert,
            iconColor: "text-rose-600 dark:text-rose-400",
        },
        ForkConflict: {
            label: t("wallet.protectionWarning", "Sicherheitswarnung"),
            description: t("wallet.anchor.conflictTooltip", "Sicherheitswarnung: Fork-Konflikt erkannt. Klicken zur Bereinigung."),
            tooltip: t("wallet.anchor.conflictTooltip", "Sicherheitswarnung: Fork-Konflikt erkannt. Klicken zur Bereinigung."),
            containerStyle: "bg-rose-50 text-rose-800 border-rose-300 dark:bg-rose-950/50 dark:text-rose-200 dark:border-rose-700 hover:bg-rose-100 animate-pulse",
            dotStyle: "bg-rose-600",
            Icon: ShieldAlert,
            iconColor: "text-rose-600 dark:text-rose-400",
        },
    };

    const currentConfig = statusConfigs[status] || statusConfigs.Offline;
    const IconComp = currentConfig.Icon;

    return (
        <div className={`relative inline-flex items-center ${className}`} ref={popoverRef}>
            <div 
                className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border shadow-xs transition-all cursor-pointer select-none ${currentConfig.containerStyle}`}
                title={currentConfig.tooltip}
                role="button"
                tabIndex={0}
                aria-label={`Echtzeitschutz Status: ${currentConfig.label}`}
                aria-haspopup="dialog"
                aria-expanded={isOpen}
                onClick={() => setIsOpen((prev) => !prev)}
                onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        setIsOpen((prev) => !prev);
                    }
                }}
            >
                <span className={`w-2 h-2 rounded-full shrink-0 ${currentConfig.dotStyle} ${isConflict ? "animate-ping" : ""}`} />
                <IconComp size={14} className={`shrink-0 ${currentConfig.iconColor}`} />
                {!compact && (
                    <span className="font-semibold truncate hidden sm:inline">
                        {currentConfig.label}
                    </span>
                )}
                {isTestnet && (
                    <span className="text-[10px] uppercase font-black px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20 shrink-0">
                        Testnet
                    </span>
                )}
                {isChecking && (
                    <RefreshCw size={10} className="animate-spin opacity-70 ml-0.5" />
                )}
            </div>

            {/* Lightweight Click Popover */}
            {isOpen && (
                <div
                    className="absolute top-full right-0 mt-2 z-50 w-80 sm:w-96 bg-card rounded-2xl p-4 shadow-xl border border-theme-subtle text-theme-secondary space-y-3 animate-in fade-in slide-in-from-top-2 duration-150"
                    role="dialog"
                    aria-modal="false"
                    aria-label={currentConfig.label}
                >
                    <div className="flex items-start justify-between gap-2">
                        <div className="flex items-center gap-2">
                            <div className={`p-1.5 rounded-lg border ${currentConfig.containerStyle}`}>
                                <IconComp size={16} className={currentConfig.iconColor} />
                            </div>
                            <div>
                                <div className="flex items-center gap-1.5">
                                    <h3 className="text-sm font-bold text-theme-primary">
                                        {currentConfig.label}
                                    </h3>
                                    {isTestnet && (
                                        <span className="text-[9px] uppercase font-black px-1 py-0.2 rounded bg-amber-500/10 text-amber-600 border border-amber-500/20">
                                            Testnet
                                        </span>
                                    )}
                                </div>
                                <p className="text-[11px] text-theme-light">
                                    {currentConfig.description}
                                </p>
                            </div>
                        </div>
                        <button
                            type="button"
                            onClick={() => setIsOpen(false)}
                            className="text-theme-placeholder hover:text-theme-primary p-1 rounded-lg hover:bg-bg-app transition-colors"
                            aria-label={t("common.close", "Schließen")}
                        >
                            <X size={16} />
                        </button>
                    </div>

                    {/* Conflict resolution box */}
                    {isConflict && (
                        <div className="p-3 bg-rose-50/70 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-800/50 rounded-xl space-y-2 text-xs">
                            <div className="flex items-center gap-1.5 text-rose-800 dark:text-rose-300 font-bold">
                                <AlertTriangle size={14} className="shrink-0" />
                                <span>{isSameDevice ? t("wallet.anchor.sameDeviceRollbackModalTitle", "Lokales Backup aktualisieren") : t("wallet.anchor.modalTitle", "Sicherheitswarnung: Backup-Konflikt")}</span>
                            </div>
                            <p className="text-[11px] text-rose-700 dark:text-rose-400 leading-relaxed">
                                {isSameDevice ? t("wallet.anchor.sameDeviceRollbackModalDesc", "Dieses Gerät hat ein früheres Backup geladen. Neuere Transaktionen wurden bereits online geschützt.") : t("wallet.anchor.modalDescription", "Ein veraltetes Backup wurde erkannt oder ein Gerätekonflikt liegt vor.")}
                            </p>

                            {(status === "RollbackDetected" || status === "SameDeviceRollback") && (
                                <div className="pt-1">
                                    <Button
                                        variant="primary"
                                        size="sm"
                                        onClick={handleReconcile}
                                        disabled={isReconciling}
                                        className="w-full text-xs flex items-center justify-center gap-1.5"
                                    >
                                        <RefreshCw size={12} className={isReconciling ? "animate-spin" : ""} />
                                        <span>{isReconciling ? t("wallet.anchor.reconciling", "Synchronisiere...") : t("wallet.anchor.reconcileBtn", "Gutscheine prüfen & synchronisieren")}</span>
                                    </Button>
                                </div>
                            )}

                            {reconcileReport && (
                                <div className="p-2 bg-white dark:bg-card border border-emerald-200 rounded-lg text-emerald-800 dark:text-emerald-300 text-[11px] space-y-0.5">
                                    <div className="flex items-center gap-1 font-bold">
                                        <Check size={12} />
                                        <span>{t("wallet.anchor.reconcileSuccess", "Erfolgreich synchronisiert")}</span>
                                    </div>
                                    <p>{t("wallet.anchor.reconciledRetainedLabel", "Erhaltene Gutscheine:")} {(reconcileReport.retainedVoucherIds?.length ?? reconcileReport.retained_voucher_ids?.length ?? 0)}</p>
                                    <p>{t("wallet.anchor.reconciledArchivedLabel", "Archivierte Gutscheine:")} {(reconcileReport.archivedVoucherIds?.length ?? reconcileReport.archived_voucher_ids?.length ?? 0)}</p>
                                </div>
                            )}

                            {reconcileError && (
                                <p className="text-[11px] text-rose-600 font-semibold">{reconcileError}</p>
                            )}
                        </div>
                    )}

                    <div className="flex items-center justify-between gap-2 pt-1">
                        <Button
                            variant="outline"
                            size="sm"
                            onClick={handleOpenSettings}
                            className="flex items-center gap-1.5 text-xs"
                        >
                            <Network size={14} />
                            <span>{t("wallet.openNetworkSettings", "Netzwerk-Einstellungen öffnen")}</span>
                        </Button>

                        <Button
                            variant="outline"
                            size="sm"
                            onClick={() => refreshSyncStatus()}
                            disabled={isChecking}
                            className="flex items-center gap-1.5 text-xs"
                            title={t("wallet.checkProtectionNow", "Jetzt prüfen")}
                        >
                            <RefreshCw size={13} className={isChecking ? "animate-spin" : ""} />
                            <span>{t("wallet.checkProtectionNow", "Jetzt prüfen")}</span>
                        </Button>
                    </div>
                </div>
            )}
        </div>
    );
};

export default RealtimeProtectionBadge;
