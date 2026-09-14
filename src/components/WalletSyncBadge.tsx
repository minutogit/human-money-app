// src/components/WalletSyncBadge.tsx
import React, { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { 
    CheckCircle2, 
    Shield, 
    ShieldAlert, 
    RefreshCw, 
    AlertTriangle, 
    Check, 
    X,
    Lock
} from "lucide-react";
import { settingsService, AnchorSyncStatus, RollbackReport } from "../services/settingsService";
import { useSession } from "../context/SessionContext";
import { logger } from "../utils/log";
import { Button } from "./ui/Button";

export interface WalletSyncBadgeProps {
    className?: string;
    compact?: boolean;
}

export const WalletSyncBadge: React.FC<WalletSyncBadgeProps> = ({
    className = "",
    compact = false,
}) => {
    const { t } = useTranslation();
    const { isSessionActive, protectAction } = useSession();

    const [status, setStatus] = useState<AnchorSyncStatus>("Offline");
    const [isChecking, setIsChecking] = useState(false);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [isReconciling, setIsReconciling] = useState(false);
    const [reconcileReport, setReconcileReport] = useState<RollbackReport | null>(null);
    const [reconcileError, setReconcileError] = useState<string | null>(null);
    const [activeNetwork, setActiveNetwork] = useState<string | undefined>(undefined);

    const refreshSyncStatus = useCallback(async () => {
        if (!isSessionActive) {
            setStatus("Offline");
            return;
        }

        setIsChecking(true);
        try {
            const nextStatus = await settingsService.checkAnchorSync();
            setStatus(nextStatus);
            if (nextStatus === "RollbackDetected" || nextStatus === "ForkConflict") {
                logger.warn(`L2 Anchor Sync alert detected: ${nextStatus}`);
            }
        } catch (e) {
            console.debug(`Anchor sync check returned error: ${e}`);
            setStatus("Offline");
        } finally {
            setIsChecking(false);
        }
    }, [isSessionActive]);

    const refreshNetwork = useCallback(async () => {
        try {
            const s = await settingsService.getSettings();
            setActiveNetwork(s.activeNetwork || "mainnet");
        } catch {
            setActiveNetwork("mainnet");
        }
    }, []);

    useEffect(() => {
        // Always keep network mode up-to-date (even offline) for Testnet badge
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
        } catch (e: any) {
            logger.error(`Reconciliation failed: ${e}`);
            setReconcileError(e?.message || String(e));
        } finally {
            setIsReconciling(false);
        }
    };

    const isConflict = status === "RollbackDetected" || status === "ForkConflict";
    const isTestnet = activeNetwork?.toLowerCase() === "testnet";

    const badgeConfig = {
        Synchronized: {
            label: t("wallet.anchor.syncLabel", "L2 synchron"),
            tooltip: t("wallet.anchor.syncTooltip", "Zustand auf Layer 2 verifiziert"),
            containerStyle: "bg-emerald-50/90 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800/60 hover:bg-emerald-100/90",
            dotStyle: "bg-emerald-500",
            Icon: CheckCircle2,
            iconColor: "text-emerald-600 dark:text-emerald-400",
        },
        Offline: {
            label: t("wallet.anchor.offlineLabel", "Offline"),
            tooltip: t("wallet.anchor.offlineTooltip", "Zahlungen offline möglich"),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: Shield,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        PendingLeapLock: {
            label: t("wallet.anchor.pendingLabel", "Nicht synchronisiert"),
            tooltip: t("wallet.anchor.pendingTooltip", "Zahlungen offline möglich"),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: Shield,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        RollbackDetected: {
            label: t("wallet.anchor.conflictLabel", "Backup-Konflikt"),
            tooltip: t("wallet.anchor.conflictTooltip", "Sicherheitswarnung: Backup-Konflikt erkannt. Klicken zur Bereinigung."),
            containerStyle: "bg-rose-50 text-rose-800 border-rose-300 dark:bg-rose-950/50 dark:text-rose-200 dark:border-rose-700 hover:bg-rose-100 animate-pulse",
            dotStyle: "bg-rose-600",
            Icon: ShieldAlert,
            iconColor: "text-rose-600 dark:text-rose-400",
        },
        ForkConflict: {
            label: t("wallet.anchor.conflictLabel", "Sicherheitswarnung"),
            tooltip: t("wallet.anchor.conflictTooltip", "Sicherheitswarnung: Fork-Konflikt erkannt. Klicken zur Bereinigung."),
            containerStyle: "bg-rose-50 text-rose-800 border-rose-300 dark:bg-rose-950/50 dark:text-rose-200 dark:border-rose-700 hover:bg-rose-100 animate-pulse",
            dotStyle: "bg-rose-600",
            Icon: ShieldAlert,
            iconColor: "text-rose-600 dark:text-rose-400",
        },
    }[status] || {
        label: t("wallet.anchor.offlineLabel", "Offline"),
        tooltip: t("wallet.anchor.offlineTooltip", "Zahlungen offline möglich"),
        containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200",
        dotStyle: "bg-amber-500",
        Icon: Shield,
        iconColor: "text-amber-600",
    };

    const IconComp = badgeConfig.Icon;

    const testnetTooltip = isTestnet
        ? t("wallet.anchor.testnetTooltip", "Testnet-Modus: Transaktionen laufen im Testnetzwerk – kein echtes Mainnet-Geld. Netzwerk: testnet")
        : t("wallet.anchor.mainnetTooltip", "Mainnet-Modus: Produktiv-Netzwerk");

    return (
        <>
            <div className={`inline-flex items-center gap-2 ${className}`}>
                <div 
                    className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border shadow-xs transition-all cursor-pointer select-none ${badgeConfig.containerStyle}`}
                    title={badgeConfig.tooltip}
                    role="button"
                    tabIndex={0}
                    aria-label={`L2 Status: ${badgeConfig.label} - ${badgeConfig.tooltip}`}
                    onClick={() => {
                        if (isConflict) {
                            setIsModalOpen(true);
                        } else {
                            refreshSyncStatus();
                        }
                    }}
                    onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            if (isConflict) {
                                setIsModalOpen(true);
                            } else {
                                refreshSyncStatus();
                            }
                        }
                    }}
                >
                    <span className={`w-2 h-2 rounded-full ${badgeConfig.dotStyle} ${isConflict ? "animate-ping" : ""}`} />
                    <IconComp size={14} className={badgeConfig.iconColor} />
                    {!compact && (
                        <span className="font-semibold truncate">
                            {badgeConfig.label}
                        </span>
                    )}
                    {isChecking && (
                        <RefreshCw size={10} className="animate-spin opacity-70 ml-0.5" />
                    )}
                </div>

                {isTestnet && (
                    <span
                        className="inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-bold border shadow-xs bg-orange-100 text-orange-800 border-orange-300 dark:bg-orange-950/40 dark:text-orange-300 dark:border-orange-800/60"
                        title={testnetTooltip}
                        role="status"
                        aria-label={`Testnet: ${testnetTooltip}`}
                    >
                        🧪 Testnet
                    </span>
                )}
                {!isTestnet && activeNetwork && !compact && (
                    <span className="sr-only" title={testnetTooltip}>{testnetTooltip}</span>
                )}
            </div>

            {/* Conflict & Reconciliation Modal */}
            {isModalOpen && (
                <div 
                    className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4 animate-in fade-in duration-150"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="anchor-reconcile-title"
                >
                    <div className="max-w-md w-full bg-card rounded-2xl p-6 shadow-2xl border border-theme-subtle text-theme-secondary space-y-4">
                        <div className="flex items-start justify-between gap-3">
                            <div className="flex items-center gap-2.5 text-theme-error">
                                <div className="p-2 rounded-xl bg-rose-100 dark:bg-rose-950/60 border border-rose-300">
                                    <ShieldAlert size={22} className="text-rose-600 dark:text-rose-400" />
                                </div>
                                <h3 id="anchor-reconcile-title" className="text-base font-bold text-theme-primary">
                                    {t("wallet.anchor.modalTitle", "Sicherheitswarnung: Backup-Konflikt")}
                                </h3>
                            </div>
                            <button
                                onClick={() => setIsModalOpen(false)}
                                className="text-theme-placeholder hover:text-theme-primary p-1 rounded-lg hover:bg-bg-app transition-colors"
                                aria-label="Schließen"
                            >
                                <X size={18} />
                            </button>
                        </div>

                        {!reconcileReport ? (
                            <>
                                <p className="text-xs leading-relaxed text-theme-secondary">
                                    {t(
                                        "wallet.anchor.modalDescription",
                                        "Ein veraltetes Backup wurde erkannt oder ein Gerätekonflikt liegt vor. Auf Layer 2 existieren bereits neuere Transaktionen für diese Wallet."
                                    )}
                                </p>
                                <div className="p-3 bg-bg-app rounded-xl text-xs space-y-1.5 border border-theme-subtle">
                                    <div className="flex items-center gap-2 font-semibold text-theme-primary">
                                        <Lock size={14} className="text-theme-primary" />
                                        <span>{t("wallet.anchor.safetyGuaranteeTitle", "Automatische Bereinigung:")}</span>
                                    </div>
                                    <ul className="list-disc list-inside space-y-1 text-theme-light text-[11px]">
                                        <li>Bereits auf anderen Geräten ausgegebene Gutscheine werden archiviert.</li>
                                        <li>Alle noch unberührten, gültigen Gutscheine bleiben vollständig erhalten.</li>
                                        <li>Der Wallet-Zustand wird mit dem L2-Sperrregister harmonisiert.</li>
                                    </ul>
                                </div>

                                {reconcileError && (
                                    <div className="p-2.5 rounded-xl bg-rose-50 dark:bg-rose-950/40 border border-rose-200 text-rose-700 dark:text-rose-300 text-xs flex items-center gap-2">
                                        <AlertTriangle size={14} className="shrink-0" />
                                        <span>{reconcileError}</span>
                                    </div>
                                )}

                                <div className="flex items-center justify-end gap-2 pt-2">
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        onClick={() => setIsModalOpen(false)}
                                        disabled={isReconciling}
                                    >
                                        {t("common.cancel", "Abbrechen")}
                                    </Button>
                                    <Button
                                        variant="primary"
                                        size="sm"
                                        onClick={handleReconcile}
                                        disabled={isReconciling}
                                        className="bg-rose-600 hover:bg-rose-700 text-white"
                                    >
                                        {isReconciling ? (
                                            <span className="flex items-center gap-1.5">
                                                <RefreshCw size={14} className="animate-spin" />
                                                {t("wallet.anchor.reconciling", "Synchronisiere...")}
                                            </span>
                                        ) : (
                                            t("wallet.anchor.reconcileBtn", "Gutscheine prüfen & synchronisieren")
                                        )}
                                    </Button>
                                </div>
                            </>
                        ) : (
                            <div className="space-y-4 pt-1">
                                <div className="p-3 rounded-xl bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 text-emerald-800 dark:text-emerald-200 text-xs space-y-2">
                                    <div className="flex items-center gap-2 font-bold text-sm text-emerald-900 dark:text-emerald-100">
                                        <Check size={16} />
                                        <span>{t("wallet.anchor.reconcileSuccess", "Erfolgreich synchronisiert")}</span>
                                    </div>
                                    <p className="text-[11px] leading-relaxed">
                                        {reconcileReport.message}
                                    </p>
                                    <div className="pt-2 border-t border-emerald-200/60 dark:border-emerald-800/60 flex justify-between text-[11px]">
                                        <span>Archivierte Gutscheine:</span>
                                        <span className="font-bold">
                                            {reconcileReport.archivedVoucherIds?.length ?? reconcileReport.archived_voucher_ids?.length ?? 0}
                                        </span>
                                    </div>
                                    <div className="flex justify-between text-[11px]">
                                        <span>Gesicherte Gutscheine:</span>
                                        <span className="font-bold">
                                            {reconcileReport.retainedVoucherIds?.length ?? reconcileReport.retained_voucher_ids?.length ?? 0}
                                        </span>
                                    </div>
                                    <div className="flex justify-between text-[11px]">
                                        <span>Neuer Nonce-Zähler:</span>
                                        <span className="font-bold font-mono">
                                            {reconcileReport.newTxNonce ?? reconcileReport.new_tx_nonce ?? 0}
                                        </span>
                                    </div>
                                </div>

                                <div className="flex justify-end">
                                    <Button
                                        variant="primary"
                                        size="sm"
                                        onClick={() => {
                                            setIsModalOpen(false);
                                            setReconcileReport(null);
                                        }}
                                    >
                                        {t("common.done", "Fertig")}
                                    </Button>
                                </div>
                            </div>
                        )}
                    </div>
                </div>
            )}
        </>
    );
};

export default WalletSyncBadge;
