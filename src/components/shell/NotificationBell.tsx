// src/components/NotificationBell.tsx
import React, { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
    Bell,
    RefreshCw,
    CheckCheck,
    Trash2,
    Wallet,
    MessageSquare,
    Store,
    Users,
    X,
} from "lucide-react";
import { FrontendAppNotice } from "../../types";
import { marketService } from "../../services/marketService";
import { postService } from "../../services/postService";
import { useNavigation } from "../../context/NavigationContext";
import { useSession } from "../../context/SessionContext";
import {
    APP_NOTICES_UPDATED_EVENT,
    notifyAppNoticesUpdated,
    showTransientSyncToast,
} from "../../utils/syncEvents";
import { describeAppNotice, formatLastSyncLabel } from "../../utils/appNotices";

export const NotificationBell: React.FC = () => {
    const { t } = useTranslation();
    const { navigate } = useNavigation();
    const { isSessionActive } = useSession();

    const [notices, setNotices] = useState<FrontendAppNotice[]>([]);
    const [lastSyncAt, setLastSyncAt] = useState<number | null>(null);
    const [isOpen, setIsOpen] = useState(false);
    const [isSyncing, setIsSyncing] = useState(false);

    const popoverRef = useRef<HTMLDivElement>(null);
    const triggerRef = useRef<HTMLButtonElement>(null);

    const loadNotices = useCallback(async () => {
        if (!isSessionActive || typeof marketService?.getAppNotices !== "function") return;
        try {
            const res = await marketService.getAppNotices();
            setNotices(res?.notices || []);
            setLastSyncAt(res?.lastSyncAt ?? null);
        } catch {
            // Silently ignore when locked or unavailable
        }
    }, [isSessionActive]);

    useEffect(() => {
        if (!isSessionActive) {
            setNotices([]);
            setLastSyncAt(null);
            return;
        }
        void loadNotices();

        const onUpdated = () => {
            void loadNotices();
        };
        window.addEventListener(APP_NOTICES_UPDATED_EVENT, onUpdated);
        return () => {
            window.removeEventListener(APP_NOTICES_UPDATED_EVENT, onUpdated);
        };
    }, [isSessionActive, loadNotices]);

    useEffect(() => {
        if (!isOpen) return;
        const onOutsideClick = (e: MouseEvent) => {
            if (
                popoverRef.current &&
                !popoverRef.current.contains(e.target as Node) &&
                triggerRef.current &&
                !triggerRef.current.contains(e.target as Node)
            ) {
                setIsOpen(false);
            }
        };
        const onEscape = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                setIsOpen(false);
                triggerRef.current?.focus();
            }
        };
        document.addEventListener("mousedown", onOutsideClick);
        document.addEventListener("keydown", onEscape);
        return () => {
            document.removeEventListener("mousedown", onOutsideClick);
            document.removeEventListener("keydown", onEscape);
        };
    }, [isOpen]);

    const handleSyncNow = async () => {
        if (isSyncing || !isSessionActive) return;
        setIsSyncing(true);
        try {
            const [mktRes, postRes] = await Promise.allSettled([
                marketService.syncMarketChannels(),
                postService.syncPostChannels(),
            ]);
            const mkt = mktRes.status === "fulfilled" ? mktRes.value : null;
            const post = postRes.status === "fulfilled" ? postRes.value : null;
            const hasUpdates =
                (mkt?.importedCards ?? 0) > 0 ||
                (mkt?.newAds ?? 0) > 0 ||
                (post?.importedPayments ?? 0) > 0 ||
                (post?.newInquiries ?? 0) > 0 ||
                (post?.newInquiryMessages ?? 0) > 0 ||
                (post?.newWotHandshakes ?? 0) > 0 ||
                (post?.newSignRequests ?? 0) > 0 ||
                (post?.importedSignatures ?? 0) > 0;

            if (!hasUpdates) {
                showTransientSyncToast({
                    message: t("common.syncUpToDateToast", "Alles aktuell – keine neuen Einträge."),
                    variant: "info",
                });
            }
            await loadNotices();
        } finally {
            setIsSyncing(false);
        }
    };

    const handleNoticeClick = async (notice: FrontendAppNotice) => {
        const desc = describeAppNotice(notice, t);
        if (!notice.read && typeof marketService?.markAppNoticesRead === "function") {
            try {
                const res = await marketService.markAppNoticesRead(notice.id);
                setNotices(res?.notices || []);
                notifyAppNoticesUpdated();
            } catch {
                // ignore
            }
        }
        setIsOpen(false);
        navigate(desc.target);
    };

    const handleMarkAllRead = async () => {
        if (typeof marketService?.markAppNoticesRead !== "function") return;
        try {
            const res = await marketService.markAppNoticesRead(null);
            setNotices(res?.notices || []);
            notifyAppNoticesUpdated();
        } catch {
            // ignore
        }
    };

    const handleClearAll = async () => {
        if (typeof marketService?.clearAppNotices !== "function") return;
        try {
            const res = await marketService.clearAppNotices();
            setNotices(res?.notices || []);
            notifyAppNoticesUpdated();
        } catch {
            // ignore
        }
    };

    const unreadCount = notices.filter((n) => !n.read).length;

    const renderCategoryIcon = (category: "payment" | "inquiry" | "market" | "trust") => {
        switch (category) {
            case "payment":
                return <Wallet size={15} className="text-emerald-600" />;
            case "inquiry":
                return <MessageSquare size={15} className="text-theme-primary" />;
            case "market":
                return <Store size={15} className="text-sky-600" />;
            case "trust":
                return <Users size={15} className="text-amber-600" />;
        }
    };

    return (
        <div className="relative inline-block">
            <button
                ref={triggerRef}
                type="button"
                onClick={() => setIsOpen((prev) => !prev)}
                aria-haspopup="dialog"
                aria-expanded={isOpen}
                aria-label={t("common.notificationsAria", "Neuigkeiten anzeigen")}
                data-testid="notification-bell-button"
                className="relative p-2 rounded-full border border-theme-subtle bg-white dark:bg-gray-800 text-theme-secondary hover:text-theme-primary hover:border-theme-primary/40 transition-all cursor-pointer shadow-xs focus:outline-none focus:ring-2 focus:ring-theme-accent"
            >
                <Bell size={16} />
                {unreadCount > 0 && (
                    <span
                        data-testid="notification-bell-badge"
                        className="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 rounded-full bg-theme-primary text-white text-[10px] font-black flex items-center justify-center shadow-xs"
                    >
                        {unreadCount > 9 ? "9+" : unreadCount}
                    </span>
                )}
            </button>

            {isOpen && (
                <div
                    ref={popoverRef}
                    role="dialog"
                    aria-label={t("common.notificationsTitle", "Neuigkeiten")}
                    data-testid="notification-bell-popover"
                    className="absolute right-0 mt-2 w-80 sm:w-96 rounded-2xl bg-white dark:bg-gray-900 border border-theme-subtle shadow-2xl z-50 overflow-hidden animate-in fade-in zoom-in-95 duration-150"
                >
                    {/* Header */}
                    <div className="p-3.5 border-b border-theme-subtle/60 flex items-center justify-between gap-2 bg-bg-app/40">
                        <div className="min-w-0">
                            <h3 className="text-sm font-bold text-theme-primary">
                                {t("common.notificationsTitle", "Neuigkeiten")}
                            </h3>
                            <p className="text-[11px] text-theme-light truncate" data-testid="notification-last-sync">
                                {t("common.lastUpdatedPrefix", "Zuletzt aktualisiert:")}{" "}
                                {formatLastSyncLabel(lastSyncAt, t)}
                            </p>
                        </div>
                        <div className="flex items-center gap-1 shrink-0">
                            <button
                                type="button"
                                onClick={handleSyncNow}
                                disabled={isSyncing}
                                data-testid="notification-refresh-btn"
                                title={t("common.refreshNow", "Jetzt aktualisieren")}
                                className="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-xl text-[11px] font-semibold bg-white border border-theme-subtle text-theme-secondary hover:text-theme-primary hover:border-theme-primary/40 transition-colors cursor-pointer disabled:opacity-50"
                            >
                                <RefreshCw size={12} className={isSyncing ? "animate-spin" : ""} />
                                <span>{t("common.refreshBtn", "Aktualisieren")}</span>
                            </button>
                            <button
                                type="button"
                                onClick={() => setIsOpen(false)}
                                aria-label={t("common.close", "Schließen")}
                                className="p-1.5 rounded-lg text-theme-light hover:text-theme-secondary hover:bg-bg-app transition-colors cursor-pointer"
                            >
                                <X size={15} />
                            </button>
                        </div>
                    </div>

                    {/* Notices list */}
                    <div className="max-h-80 overflow-y-auto divide-y divide-theme-subtle/40">
                        {notices.length === 0 ? (
                            <div className="p-6 text-center text-xs text-theme-light">
                                {t("common.noNotifications", "Keine neuen Ereignisse vorhanden.")}
                            </div>
                        ) : (
                            notices.map((notice) => {
                                const desc = describeAppNotice(notice, t);
                                return (
                                    <button
                                        key={notice.id}
                                        type="button"
                                        onClick={() => handleNoticeClick(notice)}
                                        className={`w-full text-left p-3.5 flex items-start gap-3 hover:bg-bg-app/60 transition-colors cursor-pointer ${
                                            !notice.read ? "bg-theme-primary/5" : ""
                                        }`}
                                    >
                                        <div className="p-2 rounded-xl bg-white border border-theme-subtle shrink-0 mt-0.5">
                                            {renderCategoryIcon(desc.category)}
                                        </div>
                                        <div className="min-w-0 flex-1">
                                            <div className="flex items-center justify-between gap-2">
                                                <p className="text-xs font-bold text-theme-secondary truncate">
                                                    {desc.title}
                                                </p>
                                                {!notice.read && (
                                                    <span className="w-2 h-2 rounded-full bg-theme-primary shrink-0" />
                                                )}
                                            </div>
                                            <p className="text-[11px] text-theme-light truncate mt-0.5">
                                                {desc.subtitle}
                                            </p>
                                            <p className="text-[10px] text-theme-light/70 mt-1">
                                                {formatLastSyncLabel(notice.createdAt, t)}
                                            </p>
                                        </div>
                                    </button>
                                );
                            })
                        )}
                    </div>

                    {/* Footer actions */}
                    {notices.length > 0 && (
                        <div className="p-2.5 border-t border-theme-subtle/60 bg-bg-app/30 flex items-center justify-between gap-2">
                            <button
                                type="button"
                                onClick={handleMarkAllRead}
                                disabled={unreadCount === 0}
                                className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-semibold text-theme-secondary hover:text-theme-primary hover:bg-white transition-colors cursor-pointer disabled:opacity-40"
                            >
                                <CheckCheck size={13} />
                                <span>{t("common.markAllRead", "Alle gelesen")}</span>
                            </button>
                            <button
                                type="button"
                                onClick={handleClearAll}
                                className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-semibold text-theme-light hover:text-rose-600 hover:bg-rose-50 transition-colors cursor-pointer"
                            >
                                <Trash2 size={13} />
                                <span>{t("common.clearAll", "Leeren")}</span>
                            </button>
                        </div>
                    )}
                </div>
            )}
        </div>
    );
};

export default NotificationBell;
