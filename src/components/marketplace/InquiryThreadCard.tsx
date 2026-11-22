// src/components/marketplace/InquiryThreadCard.tsx
import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import {
    CheckCircle2,
    XCircle,
    ArrowRight,
    Trash2,
    Send,
    Clock,
    AlertCircle,
} from "lucide-react";
import { Contact, FrontendMarketInquiryItem } from "../../types";
import { translateError } from "../../utils/errorHelper";
import { Button } from "../ui/Button";
import { ContactBadge } from "../ui/ContactBadge";

export interface InquiryThreadCardProps {
    item: FrontendMarketInquiryItem;
    contacts?: Contact[];
    onSendMessage?: (
        inquiryId: string,
        options: { text?: string; decision?: "confirm" | "decline" }
    ) => Promise<void>;
    onMarkRead?: (inquiryId: string) => Promise<void> | void;
    onPay?: (counterpartyDid: string, paymentUri?: string | null) => void;
    onDismiss?: () => Promise<void>;
    onAddContact?: (did: string, suggestedName?: string | null) => void;
    onViewCard?: (did: string) => void;
}

const MAX_INQUIRY_CHARS = 280;
const MAX_MESSAGES_PER_THREAD = 20;

export const InquiryThreadCard: React.FC<InquiryThreadCardProps> = ({
    item,
    contacts = [],
    onSendMessage,
    onMarkRead,
    onPay,
    onDismiss,
    onAddContact,
    onViewCard,
}) => {
    const { t } = useTranslation();
    const [draftText, setDraftText] = useState("");
    const [isSubmitting, setIsSubmitting] = useState(false);
    const [isDismissing, setIsDismissing] = useState(false);
    const [sendError, setSendError] = useState<string | null>(null);

    const isSeller = item.role === "seller";
    const targetLabel =
        item.target.kind === "ad"
            ? `${t("marketplace.adPrefix", "Inserat")}: ${item.target.adTitle}`
            : item.target.skillTitle
            ? `${t("marketplace.skillPrefix", "Fähigkeit")}: ${item.target.skillTitle}`
            : t("marketplace.generalInquiry", "Allgemeine Anfrage");

    const hasReplyChannel = item.replySyncs.length > 0 || item.isContact;
    const isThreadFull = item.messages.length >= MAX_MESSAGES_PER_THREAD;

    const markReadIfNeeded = () => {
        if (item.unread && onMarkRead) {
            void onMarkRead(item.inquiryId);
        }
    };

    const handleSend = async (decision?: "confirm" | "decline") => {
        if (!onSendMessage || isSubmitting || !hasReplyChannel) return;
        const trimmed = draftText.trim();
        if (!decision && !trimmed) return;
        setIsSubmitting(true);
        setSendError(null);
        try {
            await onSendMessage(item.inquiryId, {
                text: trimmed || undefined,
                decision,
            });
            setDraftText("");
            markReadIfNeeded();
        } catch (err) {
            setSendError(translateError(err, t));
        } finally {
            setIsSubmitting(false);
        }
    };

    const statusBadge = (() => {
        if (item.status === "confirmed") {
            return (
                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-50 text-emerald-700 border border-emerald-200">
                    <CheckCircle2 size={11} />
                    <span>{t("marketplace.inquiryConfirmed", "Bestätigt")}</span>
                </span>
            );
        }
        if (item.status === "declined") {
            return (
                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-rose-50 text-rose-700 border border-rose-200">
                    <XCircle size={11} />
                    <span>{t("marketplace.inquiryDeclined", "Abgelehnt")}</span>
                </span>
            );
        }
        return (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-amber-50 text-amber-700 border border-amber-200">
                <Clock size={11} />
                <span>{t("marketplace.inquiryOpen", "Offen")}</span>
            </span>
        );
    })();

    return (
        <div
            data-testid={`inquiry-thread-${item.inquiryId}`}
            onClick={markReadIfNeeded}
            className={`p-4 rounded-2xl bg-white border space-y-3 shadow-xs transition-colors ${
                item.unread ? "border-theme-primary/50 ring-1 ring-theme-primary/15" : "border-theme-subtle"
            }`}
        >
            {/* Header tags & actions */}
            <div className="flex items-start justify-between gap-2 flex-wrap">
                <div className="flex items-center gap-1.5 flex-wrap">
                    {item.unread && (
                        <span
                            className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-black bg-theme-primary text-white"
                            data-testid={`inquiry-unread-${item.inquiryId}`}
                        >
                            {t("common.newBadge", "Neu")}
                        </span>
                    )}
                    <span
                        className={`text-[10px] font-bold px-2 py-0.5 rounded-full border ${
                            isSeller
                                ? "bg-indigo-50 text-indigo-700 border-indigo-200"
                                : "bg-sky-50 text-sky-700 border-sky-200"
                        }`}
                    >
                        {isSeller
                            ? t("marketplace.inquiryRoleSeller", "An dein Inserat")
                            : t("marketplace.inquiryRoleBuyer", "Deine Anfrage")}
                    </span>
                    <span className="text-[10px] font-bold text-theme-primary px-2 py-0.5 rounded-full bg-theme-primary/10">
                        {targetLabel}
                    </span>
                    {statusBadge}
                </div>

                <div className="flex items-center gap-2 shrink-0">
                    <span className="text-[10px] text-theme-light">
                        {new Date(item.updatedAt * 1000).toLocaleString()}
                    </span>
                    {onDismiss && (
                        <button
                            type="button"
                            disabled={isDismissing}
                            onClick={async (e) => {
                                e.stopPropagation();
                                setIsDismissing(true);
                                try {
                                    await onDismiss();
                                } finally {
                                    setIsDismissing(false);
                                }
                            }}
                            title={t("common.remove", "Entfernen")}
                            aria-label={t("common.remove", "Entfernen")}
                            className="p-1 rounded-lg text-theme-light hover:text-rose-600 hover:bg-rose-50 transition-colors cursor-pointer disabled:opacity-50"
                        >
                            <Trash2 size={13} />
                        </button>
                    )}
                </div>
            </div>

            {/* Counterparty Identity with Add Contact / View Card */}
            <ContactBadge
                did={item.counterpartyDid}
                contacts={contacts}
                size="sm"
                fallbackName={item.counterpartyName}
                fallbackPictureUrl={item.counterpartyPictureUrl}
                onAddContact={!item.isContact ? onAddContact : undefined}
                onViewCard={onViewCard}
            />

            {/* Chat history bubbles */}
            <div className="space-y-2 max-h-64 overflow-y-auto pr-1">
                {item.messages.map((msg) => {
                    const isMe = msg.sender === "me";
                    return (
                        <div
                            key={msg.msgId}
                            className={`flex flex-col ${isMe ? "items-end" : "items-start"}`}
                        >
                            <div
                                className={`max-w-[85%] rounded-2xl px-3 py-2 text-xs space-y-1 ${
                                    isMe
                                        ? "bg-theme-primary/10 text-theme-secondary rounded-br-sm"
                                        : "bg-bg-app text-theme-secondary border border-theme-subtle/60 rounded-bl-sm"
                                }`}
                            >
                                {msg.decision && (
                                    <div
                                        className={`inline-flex items-center gap-1 text-[10px] font-bold ${
                                            msg.decision === "confirm" ? "text-emerald-700" : "text-rose-700"
                                        }`}
                                    >
                                        {msg.decision === "confirm" ? (
                                            <CheckCircle2 size={12} />
                                        ) : (
                                            <XCircle size={12} />
                                        )}
                                        <span>
                                            {msg.decision === "confirm"
                                                ? t("marketplace.inquiryConfirmed", "Bestätigt")
                                                : t("marketplace.inquiryDeclined", "Abgelehnt")}
                                        </span>
                                    </div>
                                )}
                                {msg.text && <p className="whitespace-pre-wrap leading-relaxed">{msg.text}</p>}
                                <div className="text-[9px] text-theme-light text-right">
                                    {new Date(msg.createdAt * 1000).toLocaleTimeString([], {
                                        hour: "2-digit",
                                        minute: "2-digit",
                                    })}
                                </div>
                            </div>
                        </div>
                    );
                })}
            </div>

            {/* Pay action for confirmed buyer inquiry */}
            {!isSeller && item.status === "confirmed" && onPay && (
                <div className="pt-1 flex items-center justify-between gap-3 p-2.5 rounded-xl bg-emerald-50/70 border border-emerald-200">
                    <span className="text-xs font-semibold text-emerald-900">
                        {t("marketplace.inquiryReadyToPay", "Anfrage bestätigt – du kannst jetzt bezahlen.")}
                    </span>
                    <Button
                        size="xs"
                        variant="primary"
                        onClick={(e) => {
                            e.stopPropagation();
                            onPay(item.counterpartyDid, item.paymentUri ?? undefined);
                        }}
                        data-testid={`inquiry-pay-btn-${item.inquiryId}`}
                        className="flex items-center gap-1 shrink-0"
                    >
                        <span>{t("marketplace.payNowBtn", "Jetzt bezahlen")}</span>
                        <ArrowRight size={13} />
                    </Button>
                </div>
            )}

            {/* Chat composer */}
            {onSendMessage && !isThreadFull && (
                <div className="pt-2 border-t border-theme-subtle/50 space-y-2" onClick={(e) => e.stopPropagation()}>
                    {!hasReplyChannel ? (
                        <div className="flex items-center gap-2 text-[11px] text-amber-700 bg-amber-50/70 border border-amber-200 rounded-xl px-3 py-2">
                            <AlertCircle size={14} className="shrink-0" />
                            <span>
                                {t(
                                    "marketplace.inquiryNoReplyChannel",
                                    "Kein Rückkanal: Diese Person hat keinen Sync-Dienst für Antworten hinterlegt."
                                )}
                            </span>
                        </div>
                    ) : (
                        <>
                            <div className="flex items-center gap-2">
                                <input
                                    type="text"
                                    value={draftText}
                                    onFocus={markReadIfNeeded}
                                    onChange={(e) => setDraftText(e.target.value)}
                                    onKeyDown={(e) => {
                                        if (e.key === "Enter" && !e.shiftKey && draftText.trim()) {
                                            e.preventDefault();
                                            void handleSend();
                                        }
                                    }}
                                    placeholder={t(
                                        "marketplace.inquiryChatPlaceholder",
                                        "Nachricht schreiben..."
                                    )}
                                    maxLength={MAX_INQUIRY_CHARS}
                                    disabled={isSubmitting}
                                    data-testid={`inquiry-input-${item.inquiryId}`}
                                    className="flex-1 text-xs px-3 py-2 rounded-xl border border-theme-subtle bg-white focus:outline-none focus:ring-2 focus:ring-theme-accent"
                                />
                                <Button
                                    size="xs"
                                    variant="primary"
                                    disabled={isSubmitting || !draftText.trim()}
                                    onClick={() => void handleSend()}
                                    data-testid={`inquiry-send-btn-${item.inquiryId}`}
                                    aria-label={t("marketplace.sendMessageBtn", "Senden")}
                                    className="flex items-center gap-1 shrink-0"
                                >
                                    <Send size={12} />
                                    <span>{t("marketplace.sendMessageBtn", "Senden")}</span>
                                </Button>
                            </div>

                            {sendError && (
                                <p className="text-[11px] text-rose-600 font-medium">{sendError}</p>
                            )}

                            {isSeller && item.status === "open" && (
                                <div className="flex items-center justify-end gap-2">
                                    <Button
                                        size="xs"
                                        variant="outline"
                                        disabled={isSubmitting}
                                        onClick={() => void handleSend("decline")}
                                        data-testid={`inquiry-decline-btn-${item.inquiryId}`}
                                        className="text-rose-600 hover:bg-rose-50"
                                    >
                                        {t("marketplace.declineInquiryBtn", "Ablehnen")}
                                    </Button>
                                    <Button
                                        size="xs"
                                        variant="primary"
                                        disabled={isSubmitting}
                                        onClick={() => void handleSend("confirm")}
                                        data-testid={`inquiry-confirm-btn-${item.inquiryId}`}
                                    >
                                        {t("marketplace.confirmInquiryBtn", "Bestätigen")}
                                    </Button>
                                </div>
                            )}
                        </>
                    )}
                </div>
            )}
        </div>
    );
};

export default InquiryThreadCard;
