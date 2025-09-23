// src/App.tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { error } from "@tauri-apps/plugin-log";
import { CreateProfile } from "./components/CreateProfile"; 
import { Dashboard } from "./components/Dashboard";
import { WalletRecovery } from "./components/WalletRecovery";
import { Login } from "./components/Login";
import "./App.css";

type AppState = "loading" | "needs_profile" | "needs_login" | "logged_in" | "needs_recovery";

function App() {
    const [appState, setAppState] = useState<AppState>("loading");
    const [isSidebarOpen, setSidebarOpen] = useState(false);

    useEffect(() => {
        async function checkProfile() {
            try {
                const exists = await invoke<boolean>("profile_exists");
                setAppState(exists ? "needs_login" : "needs_profile");
            } catch (e) {
                error(`Failed to check if profile exists: ${e}`);
                setAppState("needs_profile");
            }
        }
        checkProfile();
    }, []);

    function handleLogout() {
        invoke("logout").catch(e => error(`Logout failed: ${e}`));
        setSidebarOpen(false);
        setAppState("needs_login");
    }

    function renderContent() {
        switch (appState) {
            case "loading":
                return (
                    <div className="flex h-full w-full items-center justify-center">
                        <p className="text-theme-light">Loading application...</p>
                    </div>
                );
            case "needs_profile":
                return <CreateProfile onProfileCreated={() => setAppState("logged_in")} />;
            case "needs_login":
                return <Login onLoginSuccess={() => setAppState("logged_in")} onSwitchToCreate={() => setAppState("needs_profile")} onSwitchToReset={() => setAppState("needs_recovery")} />;
            case "logged_in":
                return <Dashboard />;
            case "needs_recovery":
                return <WalletRecovery onRecoverySuccess={() => setAppState("logged_in")} onSwitchToLogin={() => setAppState("needs_login")} />;
            default:
                return (
                    <div className="flex h-full w-full items-center justify-center">
                        <p className="text-theme-error">Error: Invalid application state.</p>
                    </div>
                );
        }
    }

    return (
        <div className="flex h-screen w-full bg-bg-app font-sans text-theme-secondary overflow-hidden">
            {appState === "logged_in" && (
                <>
                    {/* Adding 'will-change-transform' fixes the rendering bug with animations */}
                    <aside
                        className={`fixed inset-y-0 left-0 z-40 w-64 transform transition-transform duration-300 ease-in-out will-change-transform md:relative md:translate-x-0 ${
                            isSidebarOpen ? "translate-x-0" : "-translate-x-full"
                        }`}
                    >
                        <div className="flex h-full flex-col bg-white dark:bg-gray-800 p-4 shadow-lg">
                            <div className="mb-8 text-center">
                                <h1 className="text-xl font-bold text-theme-primary">Voucher Wallet</h1>
                                <p className="text-sm text-theme-light">Prototype v0.1</p>
                            </div>
                            <nav className="flex flex-grow flex-col space-y-2">
                                <a href="#" className="rounded-md px-4 py-2 text-theme-secondary hover:bg-bg-app">Dashboard</a>
                                <a href="#" className="rounded-md px-4 py-2 text-theme-secondary hover:bg-bg-app">Send</a>
                                <a href="#" className="rounded-md px-4 py-2 text-theme-secondary hover:bg-bg-app">Receive</a>
                                <a href="#" className="rounded-md px-4 py-2 text-theme-secondary hover:bg-bg-app">Settings</a>
                            </nav>
                            <div className="mt-auto border-t border-theme-subtle pt-4">
                                <button onClick={handleLogout} className="w-full rounded-md px-4 py-2 text-left text-theme-secondary hover:bg-bg-app focus:outline-none focus:ring-2 focus:ring-theme-accent">
                                    Logout
                                </button>
                            </div>
                        </div>
                    </aside>

                    {/* Overlay for mobile view */}
                    {isSidebarOpen && (
                        <div
                            className="fixed inset-0 z-30 bg-black/50 md:hidden"
                            onClick={() => setSidebarOpen(false)}
                        ></div>
                    )}
                </>
            )}

            {/* Main content area */}
            <div className="flex flex-1 flex-col overflow-y-auto">
                {appState === "logged_in" && (
                    <header className="sticky top-0 z-20 flex h-16 items-center justify-between border-b border-theme-subtle bg-card px-4 shadow-sm md:hidden">
                        <button
                            onClick={() => setSidebarOpen(true)}
                            className="rounded p-2 text-theme-secondary hover:bg-bg-app focus:outline-none focus:ring-2 focus:ring-theme-accent"
                            aria-label="Open navigation"
                        >
                            <svg className="h-6 w-6" fill="none" stroke="currentColor" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M4 6h16M4 12h16M4 18h16"></path>
                            </svg>
                        </button>
                        <h1 className="text-lg font-bold text-theme-primary">Voucher Wallet</h1>
                    </header>
                )}

                <main className="w-full flex-grow p-4 md:p-6 lg:p-8">
                    {renderContent()}
                </main>
            </div>
        </div>
    );
}

export default App;