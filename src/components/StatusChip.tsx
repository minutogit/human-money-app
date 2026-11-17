// src/components/StatusChip.tsx
import React, { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
    HardDrive,
    Cloud,
    Shield,
    ShieldAlert,
    ChevronDown,
    X,
    Tag,
} from "lucide-react";
import { backupService } from "../services/backupService";
import { marketService } from "../services/marketService";
import { postService } from "../services/postService";
import { settingsService } from "../services/settingsService";
import { useNavigation } from "../context/NavigationContext";
import { useSession } from "../context/SessionContext";
import { FrontendAutoBackupStatusDto, FrontendSyncOverviewStatus, L2LockStatusDto } from "../types";
import { BACKUP_UPDATED_EVENT } from "../utils/autoBackup";
import { notifySyncStatusUpdated, SYNC_STATUS_UPDATED_EVENT } from "../utils/syncEvents";
import { useRateLimitCooldown } from "../hooks/useRateLimitCooldown";
import { rateLimitManager } from "../utils/rateLimitManager";
import { Button } from "./ui/Button";

const DEFAULT_L2_LOCK_STATUS: L2LockStatusDto = {
    status: "NoServers",
    unresolvedIntentsCount: 0,
    pendingRestoreReconciliation: false,
    templatesCount: 0,
    hasOwnNominatedServer: false,
    staleServersCount: 0,
    maxProbeAgeSecs: null,
};

export interface StatusChipProps {
    className?: string;
}

export type StatusLevel = "healthy" | "warning" | "critical";

export const StatusChip: React.FC<StatusChipProps> = ({ className = "" }) => {
    const { t } = useTranslation();
    const { navigateToSettingsSection, navigate } = useNavigation();
    const { isSessionActive } = useSession();

    // 1. Backup state
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

    // 2. Sync overview state
    const [syncOverview, setSyncOverview] = useState<FrontendSyncOverviewStatus>({
        connectedCount: 0,
        pendingCount: 0,
        friendSyncs: 0,
        publicSyncs: 0,
        hasOwnSync: false,
        ownSyncSoftWarning: false,
        lastSyncAt: null,
    });

    // 3. Realtime protection state
    const [protectionStatus, setProtectionStatus] = useState<L2LockStatusDto>(DEFAULT_L2_LOCK_STATUS);

    // 4. Expiring ads state
    const [expiringAdsCount, setExpiringAdsCount] = useState<number>(0);
    const [minExpiringDays, setMinExpiringDays] = useState<number>(0);

    // Popover state
    const [isOpen, setIsOpen] = useState(false);
    const popoverRef = useRef<HTMLDivElement>(null);
    const triggerRef = useRef<HTMLButtonElement>(null);
    const hasTriggeredDailySyncRef = useRef(false);
    const { checkAndTriggerCooldown } = useRateLimitCooldown();

    // Fetch backup status
    const refreshBackup = useCallback(async () => {
        if (!isSessionActive) return;
        try {
            const current = await backupService.getBackupStatus();
            setBackupStatus(current);
        } catch {
            // Silently retain last known status
        }
    }, [isSessionActive]);

    // Fetch sync status & daily sync throttle
    const refreshSync = useCallback(async () => {
        if (!isSessionActive) return;
        try {
            const current = await marketService.getSyncOverviewStatus();
            setSyncOverview(current);

            if (current.connectedCount === 0) {
                marketService.probeContactSyncs(false)
                    .then(() => marketService.getSyncOverviewStatus())
                    .then((fresh) => {
                        setSyncOverview(fresh);
                        if (
                            fresh.connectedCount !== current.connectedCount ||
                            fresh.pendingAuthCount !== current.pendingAuthCount ||
                            fresh.failingCount !== current.failingCount
                        ) {
                            notifySyncStatusUpdated(fresh);
                        }
                        if (!hasTriggeredDailySyncRef.current && fresh.connectedCount > 0 && navigator.onLine) {
                            const nowSecs = Math.floor(Date.now() / 1000);
                            const isOutdated = fresh.lastSyncAt == null || (nowSecs - fresh.lastSyncAt >= 86400);
                            hasTriggeredDailySyncRef.current = true;
                            if (isOutdated && !rateLimitManager.isCoolingDown()) {
                                marketService.syncMarketChannels()
                                    .then((res) => {
                                        if (res) checkAndTriggerCooldown(res);
                                    })
                                    .catch((err) => {
                                        checkAndTriggerCooldown(err);
                                    });
                                postService.syncPostChannels().catch((err) => {
                                    checkAndTriggerCooldown(err);
                                });
                            }
                        }
                    })
                    .catch(() => {});
            }

            // Gentle Daily Sync on App Open (§3.2)
            if (!hasTriggeredDailySyncRef.current && current.connectedCount > 0 && navigator.onLine) {
                const nowSecs = Math.floor(Date.now() / 1000);
                const isOutdated = current.lastSyncAt == null || (nowSecs - current.lastSyncAt >= 86400);
                hasTriggeredDailySyncRef.current = true;
                if (isOutdated && !rateLimitManager.isCoolingDown()) {
                    marketService.syncMarketChannels()
                        .then((res) => {
                            if (res) checkAndTriggerCooldown(res);
                        })
                        .catch((err) => {
                            checkAndTriggerCooldown(err);
                        });
                    postService.syncPostChannels().catch((err) => {
                        checkAndTriggerCooldown(err);
                    });
                }
            }
        } catch {
            // Silently retain last known status
        }
    }, [isSessionActive, checkAndTriggerCooldown]);

    // Fetch realtime protection status
    const refreshProtection = useCallback(async () => {
        if (!isSessionActive) {
            setProtectionStatus(DEFAULT_L2_LOCK_STATUS);
            return;
        }
        try {
            const nextStatus = await settingsService.checkL2LockStatus();
            setProtectionStatus(nextStatus);
        } catch {
            setProtectionStatus(DEFAULT_L2_LOCK_STATUS);
        }
    }, [isSessionActive]);

    // Fetch expiring ads
    const refreshAds = useCallback(async () => {
        if (!isSessionActive) return;
        try {
            const profile = await marketService.getMyMarketProfile();
            const expiring = profile.ads.filter(
                (a) => a.status === "active" && a.daysLeft <= 14
            );
            setExpiringAdsCount(expiring.length);
            setMinExpiringDays(
                expiring.length > 0 ? Math.min(...expiring.map((a) => a.daysLeft)) : 0
            );
        } catch {
            // Silently retain last known status
        }
    }, [isSessionActive]);

    useEffect(() => {
        if (!isSessionActive) {
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
            setSyncOverview({
                connectedCount: 0,
                pendingCount: 0,
                friendSyncs: 0,
                publicSyncs: 0,
                hasOwnSync: false,
                ownSyncSoftWarning: false,
                lastSyncAt: null,
            });
            setProtectionStatus(DEFAULT_L2_LOCK_STATUS);
            setExpiringAdsCount(0);
            return;
        }

        refreshBackup();
        refreshSync();
        refreshProtection();
        refreshAds();

        const backupInterval = window.setInterval(refreshBackup, 30000);
        const syncInterval = window.setInterval(refreshSync, 30000);
        const protectionInterval = window.setInterval(refreshProtection, 25000);
        const adsInterval = window.setInterval(refreshAds, 30000);

        const onBackupUpdated = (e: Event) => {
            const custom = e as CustomEvent<FrontendAutoBackupStatusDto | undefined>;
            if (custom.detail) {
                setBackupStatus(custom.detail);
            } else {
                refreshBackup();
            }
        };

        const onSyncUpdated = (e: Event) => {
            const custom = e as CustomEvent<FrontendSyncOverviewStatus | undefined>;
            if (custom.detail) {
                setSyncOverview(custom.detail);
            } else {
                refreshSync();
            }
        };

        window.addEventListener(BACKUP_UPDATED_EVENT, onBackupUpdated);
        window.addEventListener(SYNC_STATUS_UPDATED_EVENT, onSyncUpdated);

        return () => {
            window.clearInterval(backupInterval);
            window.clearInterval(syncInterval);
            window.clearInterval(protectionInterval);
            window.clearInterval(adsInterval);
            window.removeEventListener(BACKUP_UPDATED_EVENT, onBackupUpdated);
            window.removeEventListener(SYNC_STATUS_UPDATED_EVENT, onSyncUpdated);
        };
    }, [isSessionActive, refreshBackup, refreshSync, refreshProtection, refreshAds]);

    // Close on outside click or Escape
    useEffect(() => {
        if (!isOpen) return;

        const handleOutsideClick = (e: MouseEvent) => {
            if (
                popoverRef.current &&
                !popoverRef.current.contains(e.target as Node) &&
                triggerRef.current &&
                !triggerRef.current.contains(e.target as Node)
            ) {
                setIsOpen(false);
            }
        };

        const handleEscape = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                setIsOpen(false);
                triggerRef.current?.focus();
            }
        };

        document.addEventListener("mousedown", handleOutsideClick);
        document.addEventListener("keydown", handleEscape);
        return () => {
            document.removeEventListener("mousedown", handleOutsideClick);
            document.removeEventListener("keydown", handleEscape);
        };
    }, [isOpen]);

    // Determine individual row levels
    // Row 1: Backup
    const hasAnyBackup = !!backupStatus.lastBackupAt || !!backupStatus.lastSyncBackupAt;

    const backupIsCritical =
        backupStatus.status === "disconnected" ||
        backupStatus.status === "folderDisconnected" ||
        (!hasAnyBackup && backupStatus.status === "notConfigured");

    const backupIsWarning =
        !backupIsCritical &&
        (backupStatus.backupDirty ||
            backupStatus.status === "outdated" ||
            backupStatus.status === "warning" ||
            (backupStatus.changedBlobs != null && backupStatus.changedBlobs > 0) ||
            backupStatus.status === "pending");

    const backupLevel: StatusLevel = backupIsCritical
        ? "critical"
        : backupIsWarning
        ? "warning"
        : "healthy";

    const getBackupSummary = (): string => {
        if (backupStatus.status === "folderDisconnected") {
            return t("statusChip.backupFolderDisconnected", "Backup-Ordner ist nicht erreichbar.");
        }
        if (backupStatus.status === "disconnected") {
            return t("statusChip.backupFailed", "Backup-Erstellung fehlgeschlagen.");
        }
        if (!hasAnyBackup && backupStatus.status === "notConfigured") {
            return t("statusChip.backupNone", "Noch kein Backup vorhanden.");
        }
        if (backupIsWarning) {
            return t("statusChip.backupDue", "Backup fällig oder neue Änderungen vorhanden.");
        }
        return t("statusChip.backupOk", "Profil ist gesichert.");
    };

    // Row 2: Sync-Dienst
    const syncIsWarning =
        syncOverview.connectedCount === 0 ||
        syncOverview.ownSyncSoftWarning ||
        (syncOverview.failingCount ?? 0) > 0;
    const syncLevel: StatusLevel = syncIsWarning ? "warning" : "healthy";

    const getSyncSummary = (): string => {
        if (syncOverview.connectedCount === 0) {
            if ((syncOverview.pendingAuthCount ?? 0) > 0) {
                return t("statusChip.syncPendingAuth", "Sync wartet auf Freigabe.");
            }
            return t("statusChip.syncNotConfigured", "Kein Sync-Dienst verbunden.");
        }
        if ((syncOverview.failingCount ?? 0) > 0) {
            return t("statusChip.syncFailing", "Mindestens ein Sync-Dienst ist derzeit nicht erreichbar.");
        }
        if (syncOverview.ownSyncSoftWarning) {
            return t("statusChip.syncWarning", "Sync-Dienst benötigt Aufmerksamkeit.");
        }
        return t("statusChip.syncOk", "Verbunden mit {{count}} Sync-Dienst(en).", {
            count: syncOverview.connectedCount,
        });
    };

    // Row 3: Schutz (L2 Realtime Protection)
    const protectionIsCritical = false;
    const protectionIsWarning =
        protectionStatus.status === "NoServers" ||
        protectionStatus.status === "Degraded" ||
        protectionStatus.status === "Syncing";

    const protectionLevel: StatusLevel = protectionIsCritical
        ? "critical"
        : protectionIsWarning
        ? "warning"
        : "healthy";

    const getProtectionSummary = (): string => {
        if (protectionStatus.status === "NoServers" || protectionStatus.status === "Degraded") {
            return t("statusChip.protectionOffline", "Echtzeitschutz ist offline.");
        }
        if (protectionStatus.status === "Syncing") {
            return t("statusChip.protectionSyncing", "Echtzeitschutz gleicht ab...");
        }
        return t("statusChip.protectionOk", "Echtzeitschutz ist aktiv.");
    };

    // Overall count & level
    const criticalCount =
        (backupLevel === "critical" ? 1 : 0) +
        (protectionLevel === "critical" ? 1 : 0);

    const adsIsWarning = expiringAdsCount > 0;

    const warningCount =
        (backupLevel === "warning" ? 1 : 0) +
        (syncLevel === "warning" ? 1 : 0) +
        (protectionLevel === "warning" ? 1 : 0) +
        (adsIsWarning ? 1 : 0);

    const overallLevel: StatusLevel =
        criticalCount > 0 ? "critical" : warningCount > 0 ? "warning" : "healthy";

    const getPillText = (): string => {
        if (overallLevel === "critical") {
            return t("statusChip.actionRequired", "Handlung nötig");
        }
        if (overallLevel === "warning") {
            return warningCount === 1
                ? t("statusChip.recommendation", "1 Empfehlung")
                : t("statusChip.recommendations", "{{count}} Empfehlungen", { count: warningCount });
        }
        return t("statusChip.allGood", "Alles in Ordnung");
    };

    const pillStyle =
        overallLevel === "critical"
            ? "bg-rose-50 hover:bg-rose-100 text-rose-700 border-rose-200 dark:bg-rose-950/40 dark:text-rose-300 dark:border-rose-800"
            : overallLevel === "warning"
            ? "bg-amber-50 hover:bg-amber-100 text-amber-700 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800"
            : "bg-emerald-50 hover:bg-emerald-100 text-emerald-700 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800";

    const dotStyle =
        overallLevel === "critical"
            ? "bg-rose-500"
            : overallLevel === "warning"
            ? "bg-amber-500"
            : "bg-emerald-500";

    const getRowDot = (level: StatusLevel) => {
        if (level === "critical") return "bg-rose-500";
        if (level === "warning") return "bg-amber-500";
        return "bg-emerald-500";
    };

    return (
        <div className={`relative inline-block ${className}`}>
            <button
                ref={triggerRef}
                type="button"
                onClick={() => setIsOpen((prev) => !prev)}
                aria-haspopup="dialog"
                aria-expanded={isOpen}
                aria-label={t("statusChip.ariaLabel", "Systemstatus anzeigen")}
                className={`flex items-center gap-2 px-3 py-1.5 rounded-full border text-xs font-semibold transition-all duration-150 cursor-pointer shadow-xs focus:outline-none focus:ring-2 focus:ring-theme-accent ${pillStyle}`}
                data-testid="status-chip-button"
            >
                <span className={`w-2 h-2 rounded-full shrink-0 ${dotStyle}`} />
                <span>{getPillText()}</span>
                <ChevronDown
                    size={14}
                    className={`transition-transform duration-200 opacity-60 ${isOpen ? "rotate-180" : ""}`}
                />
            </button>

            {isOpen && (
                <div
                    ref={popoverRef}
                    role="dialog"
                    aria-label={t("statusChip.dialogTitle", "Systemstatus")}
                    className="absolute right-0 mt-2 w-80 sm:w-96 p-4 rounded-2xl bg-white dark:bg-gray-900 border border-theme-subtle shadow-2xl z-50 animate-in fade-in zoom-in-95 duration-150 text-theme-secondary"
                    data-testid="status-chip-popover"
                >
                    <div className="flex items-center justify-between pb-3 border-b border-theme-subtle/50 mb-3">
                        <div className="flex items-center gap-2">
                            <span className={`w-2.5 h-2.5 rounded-full shrink-0 ${dotStyle}`} />
                            <h3 className="text-sm font-bold text-theme-primary">
                                {t("statusChip.dialogTitle", "Systemstatus")}
                            </h3>
                        </div>
                        <button
                            type="button"
                            onClick={() => setIsOpen(false)}
                            className="p-1 rounded-lg text-theme-light hover:text-theme-secondary hover:bg-bg-app transition-colors"
                            aria-label={t("common.close", "Schließen")}
                        >
                            <X size={16} />
                        </button>
                    </div>

                    <div className="space-y-3">
                        {/* Row 1: Backup */}
                        <div
                            className="p-3 rounded-xl bg-bg-app/40 border border-theme-subtle/30 flex items-start justify-between gap-3"
                            data-testid="status-chip-row-backup"
                        >
                            <div className="flex items-start gap-2.5 min-w-0">
                                <div className="p-1.5 rounded-lg bg-theme-subtle/30 text-theme-primary shrink-0 mt-0.5">
                                    <HardDrive size={16} />
                                </div>
                                <div className="min-w-0">
                                    <div className="flex items-center gap-1.5">
                                        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${getRowDot(backupLevel)}`} />
                                        <p className="text-xs font-bold text-theme-primary">
                                            {t("statusChip.backupTitle", "Backup")}
                                        </p>
                                    </div>
                                    <p className="text-xs text-theme-light mt-0.5 leading-tight">
                                        {getBackupSummary()}
                                    </p>
                                </div>
                            </div>
                            <Button
                                size="xs"
                                variant={backupLevel === "critical" ? "primary" : "outline"}
                                onClick={() => {
                                    setIsOpen(false);
                                    navigateToSettingsSection("backup");
                                }}
                                className="shrink-0"
                            >
                                {backupStatus.status === "notConfigured" && !hasAnyBackup
                                    ? t("statusChip.setup", "Einrichten")
                                    : t("statusChip.open", "Öffnen")}
                            </Button>
                        </div>

                        {/* Row 2: Sync-Dienst */}
                        <div
                            className="p-3 rounded-xl bg-bg-app/40 border border-theme-subtle/30 flex items-start justify-between gap-3"
                            data-testid="status-chip-row-sync"
                        >
                            <div className="flex items-start gap-2.5 min-w-0">
                                <div className="p-1.5 rounded-lg bg-theme-subtle/30 text-theme-primary shrink-0 mt-0.5">
                                    <Cloud size={16} />
                                </div>
                                <div className="min-w-0">
                                    <div className="flex items-center gap-1.5">
                                        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${getRowDot(syncLevel)}`} />
                                        <p className="text-xs font-bold text-theme-primary">
                                            {t("statusChip.syncTitle", "Sync-Dienst")}
                                        </p>
                                    </div>
                                    <p className="text-xs text-theme-light mt-0.5 leading-tight">
                                        {getSyncSummary()}
                                    </p>
                                </div>
                            </div>
                            <Button
                                size="xs"
                                variant={syncLevel === "warning" && syncOverview.connectedCount === 0 ? "secondary" : "outline"}
                                onClick={() => {
                                    setIsOpen(false);
                                    navigateToSettingsSection("sync");
                                }}
                                className="shrink-0"
                            >
                                {syncOverview.connectedCount === 0
                                    ? t("statusChip.setup", "Einrichten")
                                    : t("statusChip.open", "Öffnen")}
                            </Button>
                        </div>

                        {/* Row 3: Schutz */}
                        <div
                            className="p-3 rounded-xl bg-bg-app/40 border border-theme-subtle/30 flex items-start justify-between gap-3"
                            data-testid="status-chip-row-protection"
                        >
                            <div className="flex items-start gap-2.5 min-w-0">
                                <div className="p-1.5 rounded-lg bg-theme-subtle/30 text-theme-primary shrink-0 mt-0.5">
                                    {protectionLevel === "critical" ? (
                                        <ShieldAlert size={16} className="text-rose-500" />
                                    ) : (
                                        <Shield size={16} />
                                    )}
                                </div>
                                <div className="min-w-0">
                                    <div className="flex items-center gap-1.5">
                                        <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${getRowDot(protectionLevel)}`} />
                                        <p className="text-xs font-bold text-theme-primary">
                                            {t("statusChip.protectionTitle", "Schutz")}
                                        </p>
                                    </div>
                                    <p className="text-xs text-theme-light mt-0.5 leading-tight">
                                        {getProtectionSummary()}
                                    </p>
                                </div>
                            </div>
                            <Button
                                size="xs"
                                variant={protectionLevel === "critical" ? "primary" : "outline"}
                                onClick={() => {
                                    setIsOpen(false);
                                    navigateToSettingsSection("network");
                                }}
                                className="shrink-0"
                            >
                                {protectionLevel === "critical"
                                    ? t("statusChip.fixNow", "Jetzt beheben")
                                    : t("statusChip.open", "Öffnen")}
                            </Button>
                        </div>

                        {/* Row 4: Ablaufende Inserate */}
                        {expiringAdsCount > 0 && (
                            <div
                                className="p-3 rounded-xl bg-amber-50/50 dark:bg-amber-950/20 border border-amber-200/60 dark:border-amber-800/40 flex items-start justify-between gap-3"
                                data-testid="status-chip-row-ads"
                            >
                                <div className="flex items-start gap-2.5 min-w-0">
                                    <div className="p-1.5 rounded-lg bg-amber-100 text-amber-700 dark:bg-amber-900/60 dark:text-amber-300 shrink-0 mt-0.5">
                                        <Tag size={16} />
                                    </div>
                                    <div className="min-w-0">
                                        <div className="flex items-center gap-1.5">
                                            <span className="w-1.5 h-1.5 rounded-full shrink-0 bg-amber-500" />
                                            <p className="text-xs font-bold text-theme-primary">
                                                {t("statusChip.adsTitle", "Inserate")}
                                            </p>
                                        </div>
                                        <p className="text-xs text-theme-light mt-0.5 leading-tight">
                                            {expiringAdsCount === 1
                                                ? t("statusChip.oneAdExpiring", { days: minExpiringDays })
                                                : t("statusChip.adsExpiring", { count: expiringAdsCount, days: minExpiringDays })}
                                        </p>
                                    </div>
                                </div>
                                <Button
                                    size="xs"
                                    variant="outline"
                                    onClick={() => {
                                        setIsOpen(false);
                                        navigate({ view: 'marketplace', initialTab: 'mine' });
                                    }}
                                    className="shrink-0"
                                >
                                    {t("statusChip.open", "Öffnen")}
                                </Button>
                            </div>
                        )}
                    </div>
                </div>
            )}
        </div>
    );
};

export default StatusChip;
