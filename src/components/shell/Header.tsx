// src/components/Header.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import logo from "../../assets/logo.png";
import { useNavigation } from "../../context/NavigationContext";
import { useSession } from "../../context/SessionContext";
import { StatusChip } from "./StatusChip";
import { NotificationBell } from "./NotificationBell";
import { INTERNAL_VIEWS } from "./Sidebar";
import { UiModeToggle } from "../ui/UiModeToggle";

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
    const { appState, setSidebarOpen } = useNavigation();
    const { isSessionActive, isAuthenticated } = useSession();
    const isLoggedIn = isAuthenticated ?? isSessionActive;

    if (!INTERNAL_VIEWS.includes(appState.view) || !isLoggedIn) {
        return null;
    }

    const titleKey = VIEW_TITLE_KEYS[appState.view] ?? "common.appName";

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
                <UiModeToggle variant="compact" />
                <NotificationBell />
                <StatusChip />
            </div>
        </header>
    );
};

export default Header;
