// src/components/BackupStatusBadge.tsx
import React, { useState, useEffect, useCallback } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { 
    HardDrive, 
    HardDriveDownload,
    CheckCircle2, 
    AlertTriangle, 
    RefreshCw, 
    Settings, 
    X
} from "lucide-react";
import { backupService } from "../services/backupService";
import { useNavigation } from "../context/NavigationContext";
import { useSession } from "../context/SessionContext";
import { FrontendAutoBackupStatusDto } from "../types";
import { BACKUP_UPDATED_EVENT, runAutoBackupPass } from "../utils/autoBackup";
import { Button } from "./ui/Button";

export interface BackupStatusBadgeProps {
    className?: string;
    compact?: boolean;
}

export const BackupStatusBadge: React.FC<BackupStatusBadgeProps> = ({
    className = "",
    compact = false,
}) => {
    const { t } = useTranslation();
    const { navigate } = useNavigation();
    const { isSessionActive } = useSession();

    const [backupStatus, setBackupStatus] = useState<FrontendAutoBackupStatusDto>({
        status: "notConfigured",
        lastBackupAt: null,
        backupDirty: false,
        syncFolder: null,
        changedBlobs: null,
    });
    const [isChecking, setIsChecking] = useState(false);
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [isManualSyncing, setIsManualSyncing] = useState(false);
    const [isDropSyncing, setIsDropSyncing] = useState(false);

    const handleSyncDrop = async () => {
        setIsDropSyncing(true);
        try {
            await backupService.syncDropBackup();
            await refreshStatus();
        } finally {
            setIsDropSyncing(false);
        }
    };

    const refreshStatus = useCallback(async () => {
        if (!isSessionActive) {
            return;
        }
        setIsChecking(true);
        try {
            const current = await backupService.getBackupStatus();
            setBackupStatus(current);
        } catch {
            // Silently retain last known status
        } finally {
            setIsChecking(false);
        }
    }, [isSessionActive]);

    useEffect(() => {
        if (isSessionActive) {
            refreshStatus();
            const interval = window.setInterval(refreshStatus, 30000);
            const onUpdated = (e: Event) => {
                const custom = e as CustomEvent<FrontendAutoBackupStatusDto | undefined>;
                if (custom.detail) {
                    setBackupStatus(custom.detail);
                } else {
                    refreshStatus();
                }
            };
            window.addEventListener(BACKUP_UPDATED_EVENT, onUpdated);
            return () => {
                window.clearInterval(interval);
                window.removeEventListener(BACKUP_UPDATED_EVENT, onUpdated);
            };
        } else {
            setBackupStatus({
                status: "notConfigured",
                lastBackupAt: null,
                backupDirty: false,
                syncFolder: null,
                changedBlobs: null,
            });
        }
    }, [isSessionActive, refreshStatus]);

    const handleSyncNow = async () => {
        setIsManualSyncing(true);
        try {
            const res = await runAutoBackupPass();
            if (res) {
                setBackupStatus(res);
            }
        } finally {
            setIsManualSyncing(false);
        }
    };

    const handleOpenSettings = () => {
        setIsModalOpen(false);
        navigate({ view: "settings" });
    };

    const statusConfig = {
        synced: {
            label: t("settings.backupBadgeSynced", "Gesichert"),
            title: t("settings.backupBadgeSyncedHint", "Auto-Backup ist aktuell"),
            containerStyle: "bg-emerald-50/90 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800/60 hover:bg-emerald-100/90",
            dotStyle: "bg-emerald-500",
            Icon: CheckCircle2,
            iconColor: "text-emerald-600 dark:text-emerald-400",
        },
        folderDisconnected: {
            label: t("settings.backupBadgeDisconnected", "Ordner getrennt"),
            title: t("settings.backupBadgeDisconnectedHint", "Backup-Ordner (z. B. USB-Stick) nicht verbunden – sichert automatisch beim Anstecken"),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: AlertTriangle,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        outdated: {
            label: t("settings.backupBadgeOutdated", "Backup ausstehend"),
            title: t("settings.backupBadgeOutdatedHint", "Letzte Sicherung liegt über 14 Tage zurück oder es gibt ungesicherte Änderungen"),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: AlertTriangle,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        manualOnly: {
            label: t("settings.backupBadgeManual", "Manuell"),
            title: t("settings.backupBadgeManualHint", "Manuelles Backup konfiguriert"),
            containerStyle: "bg-blue-50/90 text-blue-800 border-blue-200 dark:bg-blue-950/40 dark:text-blue-300 dark:border-blue-800/60 hover:bg-blue-100/90",
            dotStyle: "bg-blue-500",
            Icon: HardDriveDownload,
            iconColor: "text-blue-600 dark:text-blue-400",
        },
        notConfigured: {
            label: t("settings.backupBadgeNotConfigured", "Kein Backup"),
            title: t("settings.backupBadgeNotConfiguredHint", "Kein automatisches Backup eingerichtet"),
            containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700 hover:bg-slate-200/80",
            dotStyle: "bg-slate-400",
            Icon: HardDrive,
            iconColor: "text-slate-500 dark:text-slate-400",
        },
        locked: {
            label: t("settings.backupBadgeLocked", "Wallet gesperrt"),
            title: t("settings.backupBadgeLockedHint", "Wallet-Sitzung ist gesperrt"),
            containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700",
            dotStyle: "bg-slate-400",
            Icon: HardDrive,
            iconColor: "text-slate-500",
        },
    }[backupStatus.status] || {
        label: t("settings.backupBadgeNotConfigured", "Kein Backup"),
        title: t("settings.backupBadgeNotConfiguredHint", "Kein automatisches Backup eingerichtet"),
        containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700",
        dotStyle: "bg-slate-400",
        Icon: HardDrive,
        iconColor: "text-slate-500",
    };

    const IconComp = statusConfig.Icon;

    const lastBackupDisplay = backupStatus.lastBackupAt
        ? new Date(backupStatus.lastBackupAt).toLocaleString()
        : t("settings.backupNever", "Noch kein Backup erstellt");

    useEffect(() => {
        if (!isModalOpen) return;
        const handleKeyDown = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                setIsModalOpen(false);
            }
        };
        window.addEventListener("keydown", handleKeyDown);
        return () => window.removeEventListener("keydown", handleKeyDown);
    }, [isModalOpen]);

    const handleClick = () => {
        if (backupStatus.status === "notConfigured") {
            navigate({ view: "settings" });
        } else {
            setIsModalOpen(true);
        }
    };

    return (
        <>
            <div className={`inline-flex items-center gap-2 ${className}`}>
                <div
                    className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border shadow-xs transition-all cursor-pointer select-none ${statusConfig.containerStyle}`}
                    title={statusConfig.title}
                    role="button"
                    tabIndex={0}
                    aria-label={`Backup Status: ${statusConfig.label}`}
                    onClick={handleClick}
                    onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            handleClick();
                        }
                    }}
                >
                    <span className={`w-2 h-2 rounded-full shrink-0 ${statusConfig.dotStyle}`} />
                    <IconComp size={14} className={`shrink-0 ${statusConfig.iconColor}`} />
                    {!compact && (
                        <span className="font-semibold truncate hidden sm:inline">
                            {statusConfig.label}
                        </span>
                    )}
                    {isChecking && (
                        <RefreshCw size={10} className="animate-spin opacity-70 ml-0.5 shrink-0" />
                    )}
                </div>
            </div>

            {/* Backup Details Modal */}
            {isModalOpen && typeof document !== 'undefined' && document.body && createPortal(
                <div
                    className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 backdrop-blur-xs p-4 animate-in fade-in duration-150"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="backup-status-title"
                    onClick={(e) => {
                        if (e.target === e.currentTarget) {
                            setIsModalOpen(false);
                        }
                    }}
                >
                    <div className="max-w-md w-full bg-card rounded-2xl p-6 shadow-2xl border border-theme-subtle text-theme-secondary space-y-4">
                        <div className="flex items-start justify-between gap-3">
                            <div className="flex items-center gap-2.5">
                                <div className={`p-2 rounded-xl border ${statusConfig.containerStyle}`}>
                                    <IconComp size={20} className={statusConfig.iconColor} />
                                </div>
                                <div>
                                    <h3 id="backup-status-title" className="text-base font-bold text-theme-primary">
                                        {statusConfig.label}
                                    </h3>
                                    <p className="text-xs text-theme-light">
                                        {statusConfig.title}
                                    </p>
                                </div>
                            </div>
                            <button
                                onClick={() => setIsModalOpen(false)}
                                className="text-theme-placeholder hover:text-theme-primary p-1 rounded-lg hover:bg-bg-app transition-colors"
                                aria-label={t("common.close", "Schließen")}
                            >
                                <X size={18} />
                            </button>
                        </div>

                        <div className="p-3 bg-bg-app rounded-xl text-xs space-y-2 border border-theme-subtle">
                            <div className="flex justify-between items-center text-xs">
                                <span className="text-theme-light">{t("settings.lastBackup", "Zuletzt gesichert")}:</span>
                                <span className="font-semibold text-theme-primary">{lastBackupDisplay}</span>
                            </div>
                            {backupStatus.syncFolder && (
                                <div className="space-y-0.5 pt-1 border-t border-theme-subtle/60">
                                    <span className="text-[11px] text-theme-light">{t("settings.syncFolderTitle", "Zielordner")}:</span>
                                    <p className="font-mono text-[10px] text-theme-secondary truncate p-1.5 bg-card rounded-lg border border-theme-subtle/50">
                                        {backupStatus.syncFolder}
                                    </p>
                                </div>
                            )}
                            {backupStatus.status === "folderDisconnected" && (
                                <div className="p-2 rounded-lg bg-amber-50 dark:bg-amber-950/40 border border-amber-200 text-amber-800 dark:text-amber-200 text-[11px]">
                                    {t("settings.backupFolderDisconnectedDesc", "Der Zielordner ist derzeit nicht eingehängt (z. B. USB-Stick abgezogen). Sobald das Medium verbunden ist, wird automatisch im Hintergrund synchronisiert.")}
                                </div>
                            )}
                            {backupStatus.status === "outdated" && (
                                <div className="p-2 rounded-lg bg-amber-50 dark:bg-amber-950/40 border border-amber-200 text-amber-800 dark:text-amber-200 text-[11px]">
                                    {t("settings.backupOutdatedDesc", "Es liegen ungesicherte Änderungen vor oder das letzte Backup ist über 14 Tage alt.")}
                                </div>
                            )}
                        </div>

                        <div className="flex items-center justify-between gap-2 pt-2">
                            <Button
                                variant="outline"
                                size="sm"
                                onClick={handleOpenSettings}
                                className="flex items-center gap-1.5"
                            >
                                <Settings size={14} />
                                <span>{t("settings.openBackupSettings", "Backup-Einstellungen")}</span>
                            </Button>

                            <div className="flex items-center gap-2">
                                <Button
                                    variant="outline"
                                    size="sm"
                                    onClick={handleSyncDrop}
                                    disabled={isDropSyncing}
                                    className="flex items-center gap-1.5"
                                    title={t("settings.syncDropTooltip", "Sicherung in verbundenen Ablagen aktualisieren")}
                                >
                                    <RefreshCw size={14} className={isDropSyncing ? "animate-spin" : ""} />
                                    <span>{isDropSyncing ? t("settings.syncing", "Sichere...") : t("settings.syncDropNow", "In Ablage sichern")}</span>
                                </Button>

                                {backupStatus.status !== "notConfigured" && (
                                    <Button
                                        variant="primary"
                                        size="sm"
                                        onClick={handleSyncNow}
                                        disabled={isManualSyncing}
                                        className="flex items-center gap-1.5"
                                    >
                                        <RefreshCw size={14} className={isManualSyncing ? "animate-spin" : ""} />
                                        <span>{isManualSyncing ? t("settings.syncing", "Sichere...") : t("settings.syncNow", "Jetzt sichern")}</span>
                                    </Button>
                                )}
                            </div>
                        </div>
                    </div>
                </div>,
                document.body
            )}
        </>
    );
};

export default BackupStatusBadge;
