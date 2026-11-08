// src/components/Header.tsx
import React from "react";
import logo from "../assets/logo.png";
import { useNavigation } from "../context/NavigationContext";
import { useSession } from "../context/SessionContext";
import { WalletSyncBadge } from "./WalletSyncBadge";
import { BackupStatusBadge } from "./BackupStatusBadge";
import { DropStatusBadge } from "./DropStatusBadge";
import { INTERNAL_VIEWS } from "./Sidebar";

export const Header: React.FC = () => {
    const { appState, setSidebarOpen } = useNavigation();
    const { isSessionActive, isAuthenticated } = useSession();
    const isLoggedIn = isAuthenticated ?? isSessionActive;

    if (!INTERNAL_VIEWS.includes(appState.view) || !isLoggedIn) {
        return null;
    }

    return (
        <header className="shrink-0 z-20 flex h-16 items-center justify-between border-b border-theme-subtle bg-card px-4 md:px-6 shadow-xs">
            <div className="flex items-center gap-3 min-w-0">
                <button
                    onClick={() => setSidebarOpen(true)}
                    className="rounded-xl p-2 text-theme-secondary hover:bg-bg-app focus:outline-none focus:ring-2 focus:ring-theme-accent md:hidden shrink-0"
                    aria-label="Navigation öffnen"
                >
                    <svg className="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M4 6h16M4 12h16M4 18h16"></path>
                    </svg>
                </button>
                <div className="flex items-center gap-2.5 min-w-0">
                    <img src={logo} alt="Logo" className="w-7 h-7 object-contain md:hidden shrink-0" />
                    <h1 className="text-base md:text-lg font-bold text-theme-primary truncate">
                        Human Money App
                    </h1>
                </div>
            </div>

            <div className="flex items-center gap-2 sm:gap-3 shrink-0">
                <DropStatusBadge />
                <BackupStatusBadge />
                <WalletSyncBadge />
            </div>
        </header>
    );
};

export default Header;
