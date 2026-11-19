// src/components/Header.tsx
import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { ClipboardPaste } from "lucide-react";
import logo from "../../assets/logo.png";
import { useNavigation } from "../../context/NavigationContext";
import { useSession } from "../../context/SessionContext";
import { StatusChip } from "./StatusChip";
import { NotificationBell } from "./NotificationBell";
import { INTERNAL_VIEWS } from "./Sidebar";
import { UiModeToggle } from "../ui/UiModeToggle";
import { ModalShell } from "../ui/ModalShell";
import { Button } from "../ui/Button";
import { Textarea } from "../ui/Textarea";
import { AlertBanner } from "../ui/AlertBanner";
import { readFromClipboard } from "../../utils/clipboard";
import { extractHumocoDeepLink, classifyHumocoFile, formatCardImportStatusFeedback } from "../../utils/humocoFileClassifier";
import { parseContactCard } from "../../utils/humocoUri";
import { marketService } from "../../services/marketService";
import { translateError } from "../../utils/errorHelper";

const VIEW_TITLE_KEYS: Record<string, string> = {
    logged_in: "dashboard.title",
    wallet: "wallet.title",
    marketplace: "marketplace.title",
    transaction_history: "history.transactionHistory",
    address_book: "contacts.addressBook",
    conflict_list: "conflict.fraudReports",
    conflict_details: "conflict.fraudReports",
    settings: "settings.title",
    create_voucher: "voucher.create",
    send_vouchers: "transfer.title",
    receive_bundle: "transfer.receive",
    voucher_details: "wallet.title",
    receive_success: "transfer.receiveSuccess.transferComplete",
    transfer_success: "transfer.success.title",
    sign_request: "voucher.signRequest.title",
    support: "common.supportMenu",
    bug_report: "common.bugReportTitle",
    help_center: "helpCenter.menuTitle",
};

export const Header: React.FC = () => {
    const { t } = useTranslation();
    const { appState, navigate, setSidebarOpen } = useNavigation();
    const { isAuthenticated, protectAction } = useSession();
    const isLoggedIn = isAuthenticated;

    const [isPasteModalOpen, setIsPasteModalOpen] = useState(false);
    const [pasteInput, setPasteInput] = useState("");
    const [pasteError, setPasteError] = useState<string | null>(null);
    const [isProcessingPaste, setIsProcessingPaste] = useState(false);

    if (!INTERNAL_VIEWS.includes(appState.view) || !isLoggedIn) {
        return null;
    }

    const titleKey = VIEW_TITLE_KEYS[appState.view] ?? "common.appName";

    const handleReadClipboard = async () => {
        setPasteError(null);
        const res = await readFromClipboard();
        if (res.error || !res.text.trim()) {
            setPasteError(t("common.pasteClipboardEmpty", "Zwischenablage ist leer oder nicht lesbar."));
            return;
        }
        setPasteInput(res.text);
    };

    const handleProcessPastedContent = async () => {
        const raw = pasteInput.trim();
        if (!raw) {
            setPasteError(t("common.pasteEmptyInput", "Bitte zuerst einen Link, Code oder Inhalt einfügen."));
            return;
        }
        setPasteError(null);
        setIsProcessingPaste(true);
        try {
            const deepLink = extractHumocoDeepLink(raw);
            if (deepLink) {
                setIsPasteModalOpen(false);
                setPasteInput("");
                window.dispatchEvent(new CustomEvent("humoco:deeplink", { detail: deepLink }));
                return;
            }

            const classification = classifyHumocoFile({ text: raw });
            if (classification.kind === "card") {
                const res = await protectAction(async (pwd) => {
                    return await marketService.importContactCard(raw, pwd || undefined);
                });
                if (res && res.status === "accepted" && res.did) {
                    setIsPasteModalOpen(false);
                    setPasteInput("");
                    navigate({ view: "marketplace", selectedDid: res.did });
                    return;
                }
                if (res) {
                    setPasteError(formatCardImportStatusFeedback(res.status, t));
                    return;
                }
            }
            if (classification.kind !== "unknown" && classification.kind !== "backup") {
                const bytes = Array.from(new TextEncoder().encode(raw));
                setIsPasteModalOpen(false);
                setPasteInput("");
                navigate({
                    view: "receive_bundle",
                    droppedFile: { name: "eingefuegt.humoco", bytes },
                });
                return;
            }

            const contact = parseContactCard(raw);
            if (contact) {
                setIsPasteModalOpen(false);
                setPasteInput("");
                navigate({ view: "address_book", initialContactPayload: raw });
                return;
            }

            setPasteError(
                t(
                    "common.pasteUnrecognized",
                    "Kein gültiger Link (humoco://…), Kontakt-Code oder Zahlungs-Inhalt erkannt."
                )
            );
        } catch (err) {
            setPasteError(translateError(err, t));
        } finally {
            setIsProcessingPaste(false);
        }
    };

    return (
        <header className="relative shrink-0 z-30 flex h-16 items-center justify-between border-b border-theme-subtle bg-white dark:bg-gray-900 px-4 md:px-6 shadow-xs">
            <div className="flex items-center gap-3 min-w-0">
                <button
                    onClick={() => setSidebarOpen(true)}
                    className="rounded-xl p-2 text-theme-secondary hover:bg-bg-app focus:outline-none focus:ring-2 focus:ring-theme-accent md:hidden shrink-0"
                    aria-label={t('sidebar.openNavigation', 'Navigation öffnen')}
                >
                    <svg className="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M4 6h16M4 12h16M4 18h16"></path>
                    </svg>
                </button>
                <div className="flex items-center gap-2.5 min-w-0">
                    <img src={logo} alt={t('common.logo', 'Logo')} className="w-7 h-7 object-contain md:hidden shrink-0" />
                    <h1 className="text-base md:text-lg font-bold text-theme-primary truncate">
                        {t(titleKey)}
                    </h1>
                </div>
            </div>

            <div className="flex items-center gap-2 sm:gap-3 shrink-0">
                <button
                    type="button"
                    onClick={() => {
                        setPasteError(null);
                        setIsPasteModalOpen(true);
                    }}
                    data-testid="header-paste-link-btn"
                    title={t("common.pasteLinkTooltip", "Link, Kontakt-Code oder Zahlungsdaten einfügen")}
                    className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-full border border-theme-subtle bg-white dark:bg-gray-800 text-xs font-semibold text-theme-secondary hover:text-theme-primary hover:border-theme-primary/40 transition-colors cursor-pointer shadow-xs"
                >
                    <ClipboardPaste size={14} />
                    <span className="hidden lg:inline">{t("common.pasteLinkBtn", "Link / Code einfügen")}</span>
                </button>
                <UiModeToggle variant="compact" />
                <NotificationBell />
                <StatusChip />
            </div>

            <ModalShell
                isOpen={isPasteModalOpen}
                onClose={() => setIsPasteModalOpen(false)}
                maxWidth="md"
                title={t("common.pasteModalTitle", "Link oder Code einfügen")}
                description={t(
                    "common.pasteModalDesc",
                    "Füge eine erhaltene Nachricht mit einem humoco://-Link, eine Kontakt-ID oder einen kopierten Inhalt direkt hier ein."
                )}
                footer={
                    <div className="flex justify-end gap-2">
                        <Button
                            variant="outline"
                            size="sm"
                            onClick={() => setIsPasteModalOpen(false)}
                        >
                            {t("common.cancel", "Abbrechen")}
                        </Button>
                        <Button
                            size="sm"
                            onClick={() => void handleProcessPastedContent()}
                            disabled={isProcessingPaste || !pasteInput.trim()}
                            data-testid="header-paste-submit-btn"
                        >
                            {t("common.pasteOpenBtn", "Öffnen")}
                        </Button>
                    </div>
                }
            >
                <div className="space-y-3">
                    <div className="flex justify-end">
                        <Button
                            variant="outline"
                            size="xs"
                            onClick={() => void handleReadClipboard()}
                            data-testid="header-paste-from-clipboard-btn"
                        >
                            <ClipboardPaste size={13} className="mr-1" />
                            {t("common.pasteFromClipboard", "Aus Zwischenablage einfügen")}
                        </Button>
                    </div>
                    <Textarea
                        rows={5}
                        value={pasteInput}
                        onChange={(e) => {
                            setPasteInput(e.target.value);
                            if (pasteError) setPasteError(null);
                        }}
                        placeholder={t(
                            "common.pastePlaceholder",
                            "z. B. humoco://pay?… oder ganze Chat-/E-Mail-Nachricht"
                        )}
                        data-testid="header-paste-textarea"
                    />
                    {pasteError && (
                        <AlertBanner variant="error">{pasteError}</AlertBanner>
                    )}
                </div>
            </ModalShell>
        </header>
    );
};

export default Header;
