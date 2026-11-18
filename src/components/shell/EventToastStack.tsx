// src/components/EventToastStack.tsx
import React, { useState, useEffect, useRef, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { X, Wallet, MessageSquare, Store, Users, Info, ArrowRight } from "lucide-react";
import { AppState, FrontendAppNotice } from "../../types";
import { marketService } from "../../services/marketService";
import { useSession } from "../../context/SessionContext";
import { useNavigation } from "../../context/NavigationContext";
import {
    APP_NOTICES_UPDATED_EVENT,
    TRANSIENT_SYNC_TOAST_EVENT,
    TransientSyncToastDetail,
    notifyAppNoticesUpdated,
} from "../../utils/syncEvents";
import { describeAppNotice } from "../../utils/appNotices";

interface ActiveToast {
    id: string;
    noticeId?: string;
    category: "payment" | "inquiry" | "market" | "trust" | "info";
    title: string;
    subtitle?: string;
    target?: AppState;
}

export const EventToastStack: React.FC = () => {
    const { t } = useTranslation();
    const { isSessionActive } = useSession();
    const { navigate } = useNavigation();

    const [toasts, setToasts] = useState<ActiveToast[]>([]);
    const seenNoticeIdsRef = useRef<Set<string> | null>(null);
    const timersRef = useRef<Map<string, number>>(new Map());

    const dismissToast = useCallback((toastId: string) => {
        const timer = timersRef.current.get(toastId);
        if (timer !== undefined) {
            window.clearTimeout(timer);
            timersRef.current.delete(toastId);
        }
        setToasts((prev) => prev.filter((item) => item.id !== toastId));
    }, []);

    const enqueueToast = useCallback(
        (toast: ActiveToast) => {
            setToasts((prev) => {
                if (prev.some((tItem) => tItem.id === toast.id)) return prev;
                return [toast, ...prev].slice(0, 3);
            });
            const existingTimer = timersRef.current.get(toast.id);
            if (existingTimer !== undefined) {
                window.clearTimeout(existingTimer);
            }
            const timer = window.setTimeout(() => {
                dismissToast(toast.id);
            }, 8000);
            timersRef.current.set(toast.id, timer);
        },
        [dismissToast]
    );

    // Clear on logout & initialize seen IDs on login so old persisted notices don't re-toast
    useEffect(() => {
        if (!isSessionActive) {
            seenNoticeIdsRef.current = null;
            setToasts([]);
            timersRef.current.forEach((timer) => window.clearTimeout(timer));
            timersRef.current.clear();
            return;
        }

        if (typeof marketService?.getAppNotices !== "function") return;
        let active = true;

        marketService
            .getAppNotices()
            .then((res) => {
                if (!active) return;
                seenNoticeIdsRef.current = new Set((res?.notices || []).map((n) => n.id));
            })
            .catch(() => {
                if (active && seenNoticeIdsRef.current === null) {
                    seenNoticeIdsRef.current = new Set();
                }
            });

        return () => {
            active = false;
        };
    }, [isSessionActive]);

    useEffect(() => {
        const onNoticesUpdated = async () => {
            if (!isSessionActive || typeof marketService?.getAppNotices !== "function") return;
            try {
                const res = await marketService.getAppNotices();
                const list: FrontendAppNotice[] = res?.notices || [];
                if (seenNoticeIdsRef.current === null) {
                    seenNoticeIdsRef.current = new Set(list.map((n) => n.id));
                    return;
                }
                const newlyArrived = list.filter((n) => !seenNoticeIdsRef.current!.has(n.id));
                for (const n of newlyArrived) {
                    seenNoticeIdsRef.current.add(n.id);
                    if (!n.read) {
                        const desc = describeAppNotice(n, t);
                        enqueueToast({
                            id: `notice-${n.id}`,
                            noticeId: n.id,
                            category: desc.category,
                            title: desc.title,
                            subtitle: desc.subtitle,
                            target: desc.target,
                        });
                    }
                }
            } catch {
                // ignore
            }
        };

        const onTransient = (e: Event) => {
            const detail = (e as CustomEvent<TransientSyncToastDetail | undefined>).detail;
            if (!detail?.message) return;
            enqueueToast({
                id: `transient-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
                category: "info",
                title: detail.message,
            });
        };

        window.addEventListener(APP_NOTICES_UPDATED_EVENT, onNoticesUpdated);
        window.addEventListener(TRANSIENT_SYNC_TOAST_EVENT, onTransient);
        return () => {
            window.removeEventListener(APP_NOTICES_UPDATED_EVENT, onNoticesUpdated);
            window.removeEventListener(TRANSIENT_SYNC_TOAST_EVENT, onTransient);
        };
    }, [isSessionActive, enqueueToast, t]);

    if (!isSessionActive || toasts.length === 0) {
        return null;
    }

    const renderIcon = (category: ActiveToast["category"]) => {
        switch (category) {
            case "payment":
                return <Wallet size={16} className="text-emerald-600" />;
            case "inquiry":
                return <MessageSquare size={16} className="text-theme-primary" />;
            case "market":
                return <Store size={16} className="text-sky-600" />;
            case "trust":
                return <Users size={16} className="text-amber-600" />;
            default:
                return <Info size={16} className="text-theme-primary" />;
        }
    };

    const handleOpen = async (toast: ActiveToast) => {
        dismissToast(toast.id);
        if (toast.noticeId && typeof marketService?.markAppNoticesRead === "function") {
            try {
                await marketService.markAppNoticesRead(toast.noticeId);
                notifyAppNoticesUpdated();
            } catch {
                // ignore
            }
        }
        if (toast.target) {
            navigate(toast.target);
        }
    };

    return (
        <div
            className="fixed bottom-4 right-4 z-50 flex flex-col gap-2.5 w-80 sm:w-96 max-w-[calc(100vw-2rem)] pointer-events-none"
            data-testid="event-toast-stack"
        >
            {toasts.map((toast) => (
                <div
                    key={toast.id}
                    className="pointer-events-auto p-3.5 rounded-2xl bg-white dark:bg-gray-900 border border-theme-subtle shadow-2xl flex items-start gap-3 animate-in fade-in slide-in-from-bottom-4 duration-200"
                    role="status"
                >
                    <div className="p-2 rounded-xl bg-bg-app border border-theme-subtle shrink-0">
                        {renderIcon(toast.category)}
                    </div>
                    <div className="min-w-0 flex-1">
                        <p className="text-xs font-bold text-theme-secondary">{toast.title}</p>
                        {toast.subtitle && (
                            <p className="text-[11px] text-theme-light mt-0.5 truncate">{toast.subtitle}</p>
                        )}
                        {toast.target && (
                            <button
                                type="button"
                                onClick={() => void handleOpen(toast)}
                                className="mt-1.5 inline-flex items-center gap-1 text-[11px] font-bold text-theme-primary hover:underline cursor-pointer"
                            >
                                <span>{t("common.viewNow", "Ansehen")}</span>
                                <ArrowRight size={12} />
                            </button>
                        )}
                    </div>
                    <button
                        type="button"
                        onClick={() => dismissToast(toast.id)}
                        aria-label={t("common.dismiss", "Ausblenden")}
                        className="p-1 rounded-lg text-theme-light hover:text-theme-secondary hover:bg-bg-app transition-colors cursor-pointer shrink-0"
                    >
                        <X size={14} />
                    </button>
                </div>
            ))}
        </div>
    );
};

export default EventToastStack;
