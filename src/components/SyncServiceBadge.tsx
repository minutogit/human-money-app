// src/components/SyncServiceBadge.tsx
import React, { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
    Cloud,
    CloudOff,
    AlertTriangle,
    RefreshCw,
    Settings,
    Server,
    Users,
    Globe,
    X,
} from "lucide-react";
import { marketService } from "../services/marketService";
import { postService } from "../services/postService";
import { useNavigation } from "../context/NavigationContext";
import { useSession } from "../context/SessionContext";
import { FrontendDropOverviewStatus, FrontendDropSyncResult } from "../types";
import { DROP_STATUS_UPDATED_EVENT, notifyDropStatusUpdated } from "../utils/dropEvents";
import { Button } from "./ui/Button";

export interface SyncServiceBadgeProps {
    className?: string;
    compact?: boolean;
}

export const SyncServiceBadge: React.FC<SyncServiceBadgeProps> = ({
    className = "",
    compact = false,
}) => {
    const { t } = useTranslation();
    const { navigateToSettingsSection } = useNavigation();
    const { isSessionActive } = useSession();

    const [overview, setOverview] = useState<FrontendDropOverviewStatus>({
        connectedCount: 0,
        friendDrops: 0,
        publicDrops: 0,
        hasOwnDrop: false,
        ownDropSoftWarning: false,
        lastSyncAt: null,
    });
    const [isChecking, setIsChecking] = useState(false);
    const [isOpen, setIsOpen] = useState(false);
    const [isSyncing, setIsSyncing] = useState(false);
    const [, setSyncFeedback] = useState<FrontendDropSyncResult | null>(null);
    const popoverRef = useRef<HTMLDivElement>(null);
    const hasTriggeredDailySyncRef = useRef(false);

    const refreshStatus = useCallback(async () => {
        if (!isSessionActive) {
            return;
        }
        setIsChecking(true);
        try {
            const current = await marketService.getDropOverviewStatus();
            setOverview(current);

            // Gentle Daily Sync on App Open (§3.2): 1x täglich beim Öffnen der App
            if (!hasTriggeredDailySyncRef.current && current.connectedCount > 0 && navigator.onLine) {
                const nowSecs = Math.floor(Date.now() / 1000);
                const isOutdated = current.lastSyncAt == null || (nowSecs - current.lastSyncAt >= 86400);
                hasTriggeredDailySyncRef.current = true;
                if (isOutdated) {
                    marketService.syncMarketDrops()
                        .then(async (res) => {
                            if (res) {
                                setSyncFeedback(res);
                                const updated = await marketService.getDropOverviewStatus();
                                setOverview(updated);
                                notifyDropStatusUpdated(updated);
                            }
                        })
                        .catch(() => {
                            // Background sync failure handled silently
                        });
                    postService.syncPostDrops().catch(() => {
                        // Background post sync failure handled silently
                    });
                }
            }
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
                const custom = e as CustomEvent<FrontendDropOverviewStatus | undefined>;
                if (custom.detail) {
                    setOverview(custom.detail);
                } else {
                    refreshStatus();
                }
            };
            window.addEventListener(DROP_STATUS_UPDATED_EVENT, onUpdated);
            return () => {
                window.clearInterval(interval);
                window.removeEventListener(DROP_STATUS_UPDATED_EVENT, onUpdated);
            };
        } else {
            hasTriggeredDailySyncRef.current = false;
            setSyncFeedback(null);
            setOverview({
                connectedCount: 0,
                friendDrops: 0,
                publicDrops: 0,
                hasOwnDrop: false,
                ownDropSoftWarning: false,
                lastSyncAt: null,
            });
        }
    }, [isSessionActive, refreshStatus]);

    // Close popover on outside click or Escape
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

    const handleSyncNow = async () => {
        setIsSyncing(true);
        try {
            const [res] = await Promise.all([
                marketService.syncMarketDrops({ forceUpload: true }),
                postService.syncPostDrops().catch(() => null),
            ]);
            setSyncFeedback(res);
            const updated = await marketService.getDropOverviewStatus();
            setOverview(updated);
            notifyDropStatusUpdated(updated);
        } catch {
            // Error handled by sync logic
        } finally {
            setIsSyncing(false);
        }
    };

    const handleOpenSettings = () => {
        setIsOpen(false);
        navigateToSettingsSection("sync");
    };

    const isConnected = overview.connectedCount > 0;
    const isWarning = overview.ownDropSoftWarning;

    // Styling configuration
    const badgeConfig = isConnected
        ? isWarning
            ? {
                label: overview.connectedCount === 1
                    ? t("settings.syncServiceBadgeConnectedOne", "1 Sync-Dienst verbunden")
                    : t("settings.syncServiceBadgeConnectedMany", {
                        count: overview.connectedCount,
                        defaultValue: "{{count}} Sync-Dienste verbunden",
                    }),
                title: t("settings.syncServiceBadgeWarningHint", "Speicherwarnung: Eigener Sync-Dienst zu mindestens 80% belegt!"),
                containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
                dotStyle: "bg-amber-500",
                Icon: AlertTriangle,
                iconColor: "text-amber-600 dark:text-amber-400",
            }
            : {
                label: overview.connectedCount === 1
                    ? t("settings.syncServiceBadgeConnectedOne", "1 Sync-Dienst verbunden")
                    : t("settings.syncServiceBadgeConnectedMany", {
                        count: overview.connectedCount,
                        defaultValue: "{{count}} Sync-Dienste verbunden",
                    }),
                title: t("settings.dropBadgeConnectedHint", {
                    count: overview.connectedCount,
                    defaultValue: "{{count}} Sync-Dienst(e) aktiv für Hintergrund-Synchronisation",
                }),
                containerStyle: "bg-emerald-50/90 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800/60 hover:bg-emerald-100/90",
                dotStyle: "bg-emerald-500",
                Icon: Cloud,
                iconColor: "text-emerald-600 dark:text-emerald-400",
            }
        : {
            label: t("settings.syncServiceBadgeNotConnected", "Kein Sync-Dienst"),
            title: t("settings.syncServiceBadgeNotConnectedHint", "Kein Sync-Dienst verbunden – Klick für Details und Optionen"),
            containerStyle: "bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-900/60 dark:text-slate-300 dark:border-slate-700 hover:bg-slate-200/80",
            dotStyle: "bg-slate-400",
            Icon: CloudOff,
            iconColor: "text-slate-500 dark:text-slate-400",
        };

    const IconComp = badgeConfig.Icon;

    const lastSyncDisplay = overview.lastSyncAt
        ? new Date(overview.lastSyncAt * 1000).toLocaleString()
        : t("settings.dropLastSyncNever", "Noch nicht synchronisiert");

    return (
        <div className={`relative inline-flex items-center ${className}`} ref={popoverRef}>
            <div
                className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border shadow-xs transition-all cursor-pointer select-none ${badgeConfig.containerStyle}`}
                title={badgeConfig.title}
                role="button"
                tabIndex={0}
                aria-label={`Sync-Dienst Status: ${badgeConfig.label}`}
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
                <span className={`w-2 h-2 rounded-full shrink-0 ${badgeConfig.dotStyle}`} />
                <IconComp size={14} className={`shrink-0 ${badgeConfig.iconColor}`} />
                {!compact && (
                    <span className="font-semibold truncate hidden sm:inline">
                        {badgeConfig.label}
                    </span>
                )}
                {isChecking && (
                    <RefreshCw size={10} className="animate-spin opacity-70 ml-0.5 shrink-0" />
                )}
            </div>

            {/* Lightweight Click Popover */}
            {isOpen && (
                <div
                    className="absolute top-full -right-20 sm:right-0 mt-2 z-50 w-80 sm:w-96 max-w-[calc(100vw-1.5rem)] bg-white dark:bg-gray-900 rounded-2xl p-4 shadow-2xl border border-theme-subtle text-theme-secondary space-y-3 animate-in fade-in slide-in-from-top-2 duration-150"
                    role="dialog"
                    aria-modal="false"
                    aria-label={t("settings.syncServicePopoverTitle", "Sync-Dienst")}
                >
                    <div className="flex items-start justify-between gap-2">
                        <div className="flex items-center gap-2">
                            <div className={`p-1.5 rounded-lg border ${badgeConfig.containerStyle}`}>
                                <IconComp size={16} className={badgeConfig.iconColor} />
                            </div>
                            <div>
                                <h3 className="text-sm font-bold text-theme-primary">
                                    {badgeConfig.label}
                                </h3>
                                <p className="text-[11px] text-theme-light">
                                    {t("settings.syncServicePopoverDesc", "Automatischer Hintergrund-Austausch für Zahlungen, Angebote und Backups.")}
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

                    <div className="p-2.5 bg-bg-app rounded-xl text-xs space-y-2 border border-theme-subtle">
                        <div className="flex justify-between items-center text-xs">
                            <span className="text-theme-light">{t("settings.dropLastSync", "Zuletzt synchronisiert")}:</span>
                            <span className="font-semibold text-theme-primary">{lastSyncDisplay}</span>
                        </div>

                        {isConnected && (
                            <div className="grid grid-cols-3 gap-1.5 pt-1 border-t border-theme-subtle/60 text-[11px]">
                                <div className="flex items-center gap-1 text-theme-secondary">
                                    <Users size={12} className="text-theme-primary shrink-0" />
                                    <span>{overview.friendDrops} Freunde</span>
                                </div>
                                <div className="flex items-center gap-1 text-theme-secondary">
                                    <Globe size={12} className="text-theme-primary shrink-0" />
                                    <span>{overview.publicDrops} Öffentlich</span>
                                </div>
                                <div className="flex items-center gap-1 text-theme-secondary">
                                    <Server size={12} className="text-theme-primary shrink-0" />
                                    <span>{overview.hasOwnDrop ? "1 Eigener" : "0 Eigene"}</span>
                                </div>
                            </div>
                        )}

                        {isWarning && (
                            <div className="p-2 rounded-lg bg-amber-50 dark:bg-amber-950/40 border border-amber-200 text-amber-800 dark:text-amber-200 text-[11px]">
                                {t("settings.dropWarningStorageDesc", "Dein eigener Sync-Dienst ist zu mindestens 80% belegt.")}
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
                            <span>{t("settings.manageSyncService", "Sync-Dienst verwalten")}</span>
                        </Button>

                        <Button
                            variant="primary"
                            size="sm"
                            onClick={handleSyncNow}
                            disabled={isSyncing || !isConnected}
                            className="flex items-center gap-1.5 text-xs"
                            title={t("settings.syncNow", "Jetzt synchronisieren")}
                        >
                            <RefreshCw size={13} className={isSyncing ? "animate-spin" : ""} />
                            <span>{isSyncing ? t("settings.syncing", "Synchronisiere...") : t("settings.syncNow", "Jetzt synchronisieren")}</span>
                        </Button>
                    </div>
                </div>
            )}
        </div>
    );
};

export default SyncServiceBadge;
