// src/components/BackupStatusBadge.tsx
import React, { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { 
    HardDrive, 
    CheckCircle2, 
    AlertTriangle, 
    RefreshCw, 
    Settings, 
    X,
    Cloud,
    Folder
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
    const { navigateToSettingsSection } = useNavigation();
    const { isSessionActive } = useSession();

    const [backupStatus, setBackupStatus] = useState<FrontendAutoBackupStatusDto>({
        status: "notConfigured",
        lastBackupAt: null,
        backupDirty: false,
        syncFolder: null,
        changedBlobs: null,
        syncBackupEnabled: false,
        lastSyncBackupAt: null,
        hasAuthorizedSyncBackupTarget: false,
    });
    const [isChecking, setIsChecking] = useState(false);
    const [isOpen, setIsOpen] = useState(false);
    const [isManualSyncing, setIsManualSyncing] = useState(false);
    const popoverRef = useRef<HTMLDivElement>(null);

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
                syncBackupEnabled: false,
                lastSyncBackupAt: null,
                hasAuthorizedSyncBackupTarget: false,
            });
        }
    }, [isSessionActive, refreshStatus]);

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

    const handleSyncAll = async () => {
        setIsManualSyncing(true);
        try {
            // Trigger both local folder backup pass and sync service backup pass
            await Promise.all([
                runAutoBackupPass().catch(() => null),
                backupService.syncBackup().catch(() => null),
            ]);
            await refreshStatus();
        } finally {
            setIsManualSyncing(false);
        }
    };

    const handleOpenSettings = () => {
        setIsOpen(false);
        navigateToSettingsSection("backup");
    };

    // Determine badge styling based on combined status
    const statusConfig = {
        ok: {
            label: t("settings.backupBadgeSynced", "Gesichert"),
            title: t("settings.backupBadgeSyncedHint", "Datensicherung ist aktuell"),
            containerStyle: "bg-emerald-50/90 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800/60 hover:bg-emerald-100/90",
            dotStyle: "bg-emerald-500",
            Icon: CheckCircle2,
            iconColor: "text-emerald-600 dark:text-emerald-400",
        },
        synced: {
            label: t("settings.backupBadgeSynced", "Gesichert"),
            title: t("settings.backupBadgeSyncedHint", "Auto-Backup ist aktuell"),
            containerStyle: "bg-emerald-50/90 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800/60 hover:bg-emerald-100/90",
            dotStyle: "bg-emerald-500",
            Icon: CheckCircle2,
            iconColor: "text-emerald-600 dark:text-emerald-400",
        },
        disconnected: {
            label: t("settings.backupBadgeDisconnected", "Ordner getrennt"),
            title: t("settings.backupBadgeDisconnectedHint", "Backup-Ordner (z. B. USB-Stick) nicht verbunden – sichert automatisch beim Anstecken"),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: AlertTriangle,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        folderDisconnected: {
            label: t("settings.backupBadgeDisconnected", "Ordner getrennt"),
            title: t("settings.backupBadgeDisconnectedHint", "Backup-Ordner (z. B. USB-Stick) nicht verbunden – sichert automatisch beim Anstecken"),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: AlertTriangle,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        pending: {
            label: t("settings.backupBadgeOutdated", "Backup ausstehend"),
            title: t("settings.backupBadgeOutdatedHint", "Es gibt ungesicherte Änderungen"),
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
        warning: {
            label: t("settings.backupCombinedWarning", "Backup überfällig"),
            title: t("settings.backupBadgeOutdatedHint", "Letzte Sicherung liegt über 14 Tage zurück"),
            containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
            dotStyle: "bg-amber-500",
            Icon: AlertTriangle,
            iconColor: "text-amber-600 dark:text-amber-400",
        },
        notConfigured: {
            label: t("settings.backupBadgeNotConfigured", "Kein Backup"),
            title: t("settings.backupBadgeNotConfiguredHint", "Kein automatisches Backup eingerichtet"),
            containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700 hover:bg-slate-200/80",
            dotStyle: "bg-slate-400",
            Icon: HardDrive,
            iconColor: "text-slate-500 dark:text-slate-400",
        },
        manualOnly: {
            label: t("settings.backupBadgeManual", "Manuelle Sicherung"),
            title: t("settings.backupBadgeManualHint", "Automatische Sicherung ist deaktiviert"),
            containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700 hover:bg-slate-200/80",
            dotStyle: "bg-slate-400",
            Icon: HardDrive,
            iconColor: "text-slate-500 dark:text-slate-400",
        },
        locked: {
            label: t("settings.backupBadgeLocked", "Gesperrt"),
            title: t("settings.backupBadgeLockedHint", "Sicherung ist gesperrt"),
            containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700 hover:bg-slate-200/80",
            dotStyle: "bg-slate-400",
            Icon: HardDrive,
            iconColor: "text-slate-500 dark:text-slate-400",
        },
    }[backupStatus.status] || {
        label: t("settings.backupBadgeNotConfigured", "Kein Backup"),
        title: t("settings.backupBadgeNotConfiguredHint", "Kein automatisches Backup eingerichtet"),
        containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700 hover:bg-slate-200/80",
        dotStyle: "bg-slate-400",
        Icon: HardDrive,
        iconColor: "text-slate-500",
    };

    const IconComp = statusConfig.Icon;

    // Helper descriptions for cloud sync-service backup
    const renderSyncBackupInfo = () => {
        if (!backupStatus.syncBackupEnabled) {
            return t("settings.backupCloudDisabled", "Deaktiviert");
        }
        if (!backupStatus.hasAuthorizedSyncBackupTarget) {
            return t("settings.backupCloudNoTarget", "Kein Backup-Ziel freigeschaltet");
        }
        if (backupStatus.lastSyncBackupAt) {
            return `${t("settings.backupBadgeSynced", "Aktiv")} (${new Date(backupStatus.lastSyncBackupAt).toLocaleString()})`;
        }
        return t("settings.backupCloudActiveNever", "Aktiv (noch keine Sicherung)");
    };

    // Helper descriptions for local folder backup
    const renderLocalBackupInfo = () => {
        if (!backupStatus.syncFolder) {
            return t("settings.backupLocalNotConfigured", "Nicht eingerichtet");
        }
        if (backupStatus.status === "disconnected" || backupStatus.status === "folderDisconnected") {
            return t("settings.backupBadgeDisconnected", "Ordner getrennt");
        }
        if (backupStatus.lastBackupAt) {
            return `${t("settings.backupBadgeSynced", "Gesichert")} (${new Date(backupStatus.lastBackupAt).toLocaleString()})`;
        }
        return t("settings.backupLocalSynced", "Gesichert (Ordner verbunden)");
    };

    return (
        <div className={`relative inline-flex items-center ${className}`} ref={popoverRef}>
            <div
                className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border shadow-xs transition-all cursor-pointer select-none ${statusConfig.containerStyle}`}
                title={statusConfig.title}
                role="button"
                tabIndex={0}
                aria-label={`Backup Status: ${statusConfig.label}`}
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

            {/* Lightweight Click Popover */}
            {isOpen && (
                <div
                    className="absolute top-full -right-10 sm:right-0 mt-2 z-50 w-80 sm:w-96 max-w-[calc(100vw-1.5rem)] bg-white dark:bg-gray-900 rounded-2xl p-4 shadow-2xl border border-theme-subtle text-theme-secondary space-y-3 animate-in fade-in slide-in-from-top-2 duration-150"
                    role="dialog"
                    aria-modal="false"
                    aria-label={statusConfig.label}
                >
                    <div className="flex items-start justify-between gap-2">
                        <div className="flex items-center gap-2">
                            <div className={`p-1.5 rounded-lg border ${statusConfig.containerStyle}`}>
                                <IconComp size={16} className={statusConfig.iconColor} />
                            </div>
                            <div>
                                <h3 className="text-sm font-bold text-theme-primary">
                                    {statusConfig.label}
                                </h3>
                                <p className="text-[11px] text-theme-light">
                                    {statusConfig.title}
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

                    <div className="p-3 bg-bg-app rounded-xl text-xs space-y-2.5 border border-theme-subtle">
                        {/* 1. Sync Service Backup */}
                        <div className="space-y-1">
                            <div className="flex items-center justify-between text-xs">
                                <span className="font-semibold text-theme-primary flex items-center gap-1.5">
                                    <Cloud size={13} className="text-theme-primary" />
                                    {t("settings.backupCloudTitle", "Sync-Dienst-Backup")}
                                </span>
                                <span className="text-[11px] font-medium text-theme-secondary">
                                    {renderSyncBackupInfo()}
                                </span>
                            </div>
                        </div>

                        {/* 2. Local Folder / USB Backup */}
                        <div className="space-y-1 pt-2 border-t border-theme-subtle/60">
                            <div className="flex items-center justify-between text-xs">
                                <span className="font-semibold text-theme-primary flex items-center gap-1.5">
                                    <Folder size={13} className="text-theme-primary" />
                                    {t("settings.backupLocalTitle", "Lokaler Ordner / USB")}
                                </span>
                                <span className="text-[11px] font-medium text-theme-secondary">
                                    {renderLocalBackupInfo()}
                                </span>
                            </div>
                            {backupStatus.syncFolder && (
                                <p className="font-mono text-[10px] text-theme-light truncate p-1 bg-white dark:bg-gray-800 rounded border border-theme-subtle/50">
                                    {backupStatus.syncFolder}
                                </p>
                            )}
                            {(backupStatus.status === "disconnected" || backupStatus.status === "folderDisconnected") && (
                                <div className="p-2 rounded-lg bg-amber-50 dark:bg-amber-950/40 border border-amber-200 text-amber-800 dark:text-amber-200 text-[11px]">
                                    {t("settings.backupFolderDisconnectedDesc", "Der Zielordner ist derzeit nicht eingehängt (z. B. USB-Stick abgezogen). Sobald das Medium verbunden ist, wird automatisch im Hintergrund synchronisiert.")}
                                </div>
                            )}
                        </div>

                        {(backupStatus.status === "pending" || backupStatus.status === "outdated") && (
                            <div className="p-2 rounded-lg bg-amber-50 dark:bg-amber-950/40 border border-amber-200 text-amber-800 dark:text-amber-200 text-[11px]">
                                {t("settings.backupOutdatedDesc", "Es liegen ungesicherte Änderungen vor oder das letzte Backup ist über 14 Tage alt.")}
                            </div>
                        )}
                    </div>

                    <div className="flex items-center justify-between gap-2 pt-1">
                        <Button
                            variant="outline"
                            size="sm"
                            onClick={handleOpenSettings}
                            className="flex items-center gap-1.5 text-xs"
                        >
                            <Settings size={14} />
                            <span>{t("settings.backupActionSettings", "Backup-Einstellungen")}</span>
                        </Button>

                        <Button
                            variant="primary"
                            size="sm"
                            onClick={handleSyncAll}
                            disabled={isManualSyncing}
                            className="flex items-center gap-1.5 text-xs"
                            title={t("settings.backupActionSyncNow", "Jetzt sichern")}
                        >
                            <RefreshCw size={13} className={isManualSyncing ? "animate-spin" : ""} />
                            <span>{isManualSyncing ? t("settings.syncing", "Sichere...") : t("settings.backupActionSyncNow", "Jetzt sichern")}</span>
                        </Button>
                    </div>
                </div>
            )}
        </div>
    );
};

export default BackupStatusBadge;
