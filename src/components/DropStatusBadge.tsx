// src/components/DropStatusBadge.tsx
import React, { useState, useEffect, useCallback, useRef } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import {
    Cloud,
    CloudOff,
    AlertTriangle,
    RefreshCw,
    Settings,
    X,
    Share2,
    Check,
    Server,
    Users,
    Globe,
} from "lucide-react";
import { marketService } from "../services/marketService";
import { postService } from "../services/postService";
import { useNavigation } from "../context/NavigationContext";
import { useSession } from "../context/SessionContext";
import { FrontendDropOverviewStatus, FrontendDropSyncResult } from "../types";
import { DROP_STATUS_UPDATED_EVENT, notifyDropStatusUpdated } from "../utils/dropEvents";
import { copyToClipboard } from "../utils/clipboard";
import { Button } from "./ui/Button";

export interface DropStatusBadgeProps {
    className?: string;
    compact?: boolean;
}

export const DropStatusBadge: React.FC<DropStatusBadgeProps> = ({
    className = "",
    compact = false,
}) => {
    const { t } = useTranslation();
    const { navigate } = useNavigation();
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
    const [isModalOpen, setIsModalOpen] = useState(false);
    const [isSyncing, setIsSyncing] = useState(false);
    const [copiedInvite, setCopiedInvite] = useState(false);
    const [syncFeedback, setSyncFeedback] = useState<FrontendDropSyncResult | null>(null);
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

    const handleOpenSettings = (targetTab?: 'friend_drops' | 'public_drops' | 'own_drop') => {
        setIsModalOpen(false);
        navigate({ view: "settings", openDropsModal: true, openDropsTab: targetTab });
    };

    const handleCopyInvite = async () => {
        const text = t("settings.dropInviteTemplate", "Hallo! Ich nutze die Human Money App. Damit wir Angebote, Sicherungen und Zahlungen noch bequemer im Hintergrund austauschen können, können wir eine private Ablage auf einem Webspace nutzen. Hast du Lust, eine für unseren Kreis einzurichten? Hier findest du die Infos: https://humanmoney.org");
        const ok = await copyToClipboard(text);
        if (ok) {
            setCopiedInvite(true);
            setTimeout(() => setCopiedInvite(false), 2500);
        }
    };

    const isConnected = overview.connectedCount > 0;
    const isWarning = overview.ownDropSoftWarning;

    // Styling configuration
    const badgeConfig = isConnected
        ? isWarning
            ? {
                label: overview.connectedCount === 1
                    ? t("settings.dropBadgeConnectedOne", "1 Ablage verbunden")
                    : t("settings.dropBadgeConnectedMany", {
                        count: overview.connectedCount,
                        defaultValue: "{{count}} Ablagen verbunden",
                    }),
                title: t("settings.dropBadgeWarningHint", "Speicherwarnung: Eigene Ablage zu mindestens 80% belegt!"),
                containerStyle: "bg-amber-50/90 text-amber-800 border-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:border-amber-800/60 hover:bg-amber-100/90",
                dotStyle: "bg-amber-500",
                Icon: AlertTriangle,
                iconColor: "text-amber-600 dark:text-amber-400",
            }
            : {
                label: overview.connectedCount === 1
                    ? t("settings.dropBadgeConnectedOne", "1 Ablage verbunden")
                    : t("settings.dropBadgeConnectedMany", {
                        count: overview.connectedCount,
                        defaultValue: "{{count}} Ablagen verbunden",
                    }),
                title: t("settings.dropBadgeConnectedHint", {
                    count: overview.connectedCount,
                    defaultValue: "{{count}} Ablage(n) aktiv für Hintergrund-Synchronisation",
                }),
                containerStyle: "bg-emerald-50/90 text-emerald-800 border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800/60 hover:bg-emerald-100/90",
                dotStyle: "bg-emerald-500",
                Icon: Cloud,
                iconColor: "text-emerald-600 dark:text-emerald-400",
            }
        : {
            label: t("settings.dropBadgeNotConnected", "Keine Ablage"),
            title: t("settings.dropBadgeNotConnectedHint", "Noch keine Ablage verbunden – Klick für Details und Optionen"),
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
        <>
            <div className={`inline-flex items-center gap-2 ${className}`}>
                <div
                    className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border shadow-xs transition-all cursor-pointer select-none ${badgeConfig.containerStyle}`}
                    title={badgeConfig.title}
                    role="button"
                    tabIndex={0}
                    aria-label={`Ablage Status: ${badgeConfig.label}`}
                    onClick={() => setIsModalOpen(true)}
                    onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                            e.preventDefault();
                            setIsModalOpen(true);
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
            </div>

            {/* Drop Status Popover Modal */}
            {isModalOpen && typeof document !== "undefined" && document.body && createPortal(
                <div
                    className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 backdrop-blur-xs p-4 animate-in fade-in duration-150"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="drop-status-title"
                    onClick={(e) => {
                        if (e.target === e.currentTarget) {
                            setIsModalOpen(false);
                        }
                    }}
                >
                    <div className="max-w-md w-full bg-card rounded-2xl p-6 shadow-2xl border border-theme-subtle text-theme-secondary space-y-4">
                        <div className="flex items-start justify-between gap-3">
                            <div className="flex items-center gap-2.5">
                                <div className={`p-2 rounded-xl border ${badgeConfig.containerStyle}`}>
                                    <IconComp size={20} className={badgeConfig.iconColor} />
                                </div>
                                <div>
                                    <h3 id="drop-status-title" className="text-base font-bold text-theme-primary">
                                        {isConnected
                                            ? t("settings.dropModalConnectedTitle", "Ablagen-Status")
                                            : t("settings.dropModalNotConnectedTitle", "Keine Ablage verbunden")}
                                    </h3>
                                    <p className="text-xs text-theme-light">
                                        {isConnected
                                            ? t("settings.dropModalConnectedSubtitle", "Automatische Synchronisation im Hintergrund")
                                            : t("settings.dropModalNotConnectedSubtitle", "Offline-Betrieb von Gerät zu Gerät aktiv")}
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

                        {isConnected ? (
                            /* Connected View */
                            <div className="space-y-3">
                                {isWarning && (
                                    <div className="p-3 rounded-xl bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-800/60 text-amber-800 dark:text-amber-200 text-xs flex items-start gap-2">
                                        <AlertTriangle size={16} className="shrink-0 mt-0.5 text-amber-600 dark:text-amber-400" />
                                        <div>
                                            <span className="font-semibold block">{t("settings.dropWarningStorageTitle", "Speichergrenze fast erreicht")}</span>
                                            <span>{t("settings.dropWarningStorageDesc", "Deine eigene Ablage ist zu 80% oder mehr belegt. Prüfe den Speicherplatz in den Ablage-Einstellungen.")}</span>
                                        </div>
                                    </div>
                                )}

                                <div className="p-3 bg-bg-app rounded-xl text-xs space-y-2.5 border border-theme-subtle">
                                    <div className="flex justify-between items-center text-xs">
                                        <span className="text-theme-light">{t("settings.dropLastSync", "Letzte Synchronisation")}:</span>
                                        <span className="font-semibold text-theme-primary">{lastSyncDisplay}</span>
                                    </div>

                                    <div className="pt-2 border-t border-theme-subtle/60 grid grid-cols-3 gap-2 text-center">
                                        <div className="p-2 bg-card rounded-lg border border-theme-subtle/50">
                                            <div className="flex items-center justify-center gap-1 text-theme-light text-[11px] mb-0.5">
                                                <Users size={12} />
                                                <span>{t("settings.dropCategoryFriends", "Freunde")}</span>
                                            </div>
                                            <span className="text-sm font-bold text-theme-primary">{overview.friendDrops}</span>
                                        </div>
                                        <div className="p-2 bg-card rounded-lg border border-theme-subtle/50">
                                            <div className="flex items-center justify-center gap-1 text-theme-light text-[11px] mb-0.5">
                                                <Globe size={12} />
                                                <span>{t("settings.dropCategoryPublic", "Öffentlich")}</span>
                                            </div>
                                            <span className="text-sm font-bold text-theme-primary">{overview.publicDrops}</span>
                                        </div>
                                        <div className="p-2 bg-card rounded-lg border border-theme-subtle/50">
                                            <div className="flex items-center justify-center gap-1 text-theme-light text-[11px] mb-0.5">
                                                <Server size={12} />
                                                <span>{t("settings.dropCategoryOwn", "Eigene")}</span>
                                            </div>
                                            <span className="text-sm font-bold text-theme-primary">
                                                {overview.hasOwnDrop ? "1" : "0"}
                                            </span>
                                        </div>
                                    </div>
                                </div>

                                {syncFeedback && (
                                    <div className="p-2.5 rounded-xl bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800/60 text-emerald-800 dark:text-emerald-200 text-xs">
                                        <span className="font-semibold block mb-0.5">{t("marketplace.syncSuccessTitle", "Synchronisation abgeschlossen")}</span>
                                        <span className="opacity-90">
                                            {t("marketplace.syncResultFeedback", "{{cards}} Angebot(e), {{wot}} Vertrauensliste(n) synchronisiert", {
                                                cards: syncFeedback.importedCards,
                                                wot: syncFeedback.importedWotLists,
                                            })}
                                        </span>
                                    </div>
                                )}

                                <div className="flex items-center justify-between gap-2 pt-2">
                                    <Button
                                        variant="outline"
                                        size="sm"
                                        onClick={() => handleOpenSettings()}
                                        className="flex items-center gap-1.5"
                                    >
                                        <Settings size={14} />
                                        <span>{t("settings.openDropSettings", "Ablage-Einstellungen")}</span>
                                    </Button>

                                    <Button
                                        variant="primary"
                                        size="sm"
                                        onClick={handleSyncNow}
                                        disabled={isSyncing}
                                        className="flex items-center gap-1.5"
                                    >
                                        <RefreshCw size={14} className={isSyncing ? "animate-spin" : ""} />
                                        <span>{isSyncing ? t("settings.syncing", "Synchronisiere...") : t("settings.syncNow", "Jetzt synchronisieren")}</span>
                                    </Button>
                                </div>
                            </div>
                        ) : (
                            /* Not Connected View (§7.6) */
                            <div className="space-y-4">
                                <div className="p-3 bg-bg-app rounded-xl text-xs leading-relaxed text-theme-secondary border border-theme-subtle">
                                    <p>
                                        {t("settings.dropNotConnectedExplanation", "Die Human Money App funktioniert auch komplett ohne Internet von Gerät zu Gerät. Wenn ein technikaffiner Freund mit eigenem Webspace eine Ablage für seinen Freundeskreis einrichtet – oder du eine öffentliche Ablage nutzt –, macht das die App wesentlich komfortabler: Angebote, Sicherungen, Bürgschaften und Zahlungen synchronisieren sich dann automatisch im Hintergrund.")}
                                    </p>
                                </div>

                                <div className="flex flex-col gap-2 pt-1">
                                    <Button
                                        variant="primary"
                                        size="sm"
                                        onClick={handleCopyInvite}
                                        className="flex items-center justify-center gap-2 w-full"
                                    >
                                        {copiedInvite ? (
                                            <>
                                                <Check size={15} className="text-emerald-400" />
                                                <span>{t("common.copied", "In Zwischenablage kopiert!")}</span>
                                            </>
                                        ) : (
                                            <>
                                                <Share2 size={15} />
                                                <span>{t("settings.dropAskFriendBtn", "Freund fragen (Einladungstext teilen)")}</span>
                                            </>
                                        )}
                                    </Button>

                                    <Button
                                        variant="outline"
                                        size="sm"
                                        onClick={() => handleOpenSettings()}
                                        className="flex items-center justify-center gap-1.5 w-full"
                                    >
                                        <Settings size={14} />
                                        <span>{t("settings.openDropSettings", "Ablage-Einstellungen öffnen")}</span>
                                    </Button>
                                </div>
                            </div>
                        )}
                    </div>
                </div>,
                document.body
            )}
        </>
    );
};

export default DropStatusBadge;
