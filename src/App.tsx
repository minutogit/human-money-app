// src/App.tsx
import { useState, useEffect } from "react";
import { profileService } from "./services/profileService";
import { authService } from "./services/authService";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { error } from "@tauri-apps/plugin-log";
import { logger } from "./utils/log";
import logo from './assets/logo.png';
import { CreateNewProfile } from './components/CreateNewProfile';
import { Login } from "./components/Login";
import { CreateVoucher } from "./components/CreateVoucher";
import "./App.css";
import { SendView } from "./components/SendView";
import { SettingsView } from "./components/SettingsView";
import { TransactionHistoryView } from "./components/TransactionHistoryView";
import { TransferSuccessView } from "./components/TransferSuccessView";
import { VoucherDetailsView } from './components/VoucherDetailsView';
import { Activities } from './components/Activities';
import { ReceiveView } from './components/ReceiveView';
import { ReceiveSuccessView } from './components/ReceiveSuccessView';
import { Dashboard } from './components/Dashboard';
import { WalletView } from './components/WalletView';
import { SignRequestView } from './components/SignRequestView';
import { WalletRecovery } from './components/WalletRecovery';
import { RecreateProfile } from './components/RecreateProfile';
import AddressBook from './components/AddressBook';
import { ConflictDetailsView } from './components/ConflictDetailsView';
import { ConflictListView } from './components/ConflictListView';
import { ForkLockOverlay } from './components/ForkLockOverlay';
import { Button } from './components/ui/Button';
import { ReceiveSuccessPayload, VoucherDetails } from './types';

// WICHTIG: Der Import für den Provider
import { SessionProvider, useSession } from './context/SessionContext';
import { Sidebar, INTERNAL_VIEWS } from './components/Sidebar';

export type AppState =
    | { view: "loading" }
    | { view: "needs_profile" }
    | { view: "needs_login" }
    | { view: "logged_in" }
    | { view: "recreate_profile" }
    | { view: "needs_recovery" }
    | { view: "settings" }
    | { view: "create_voucher"; previousView?: AppState }
    | { view: "voucher_details"; voucherId: string; previousView?: AppState }
    | { view: "send_vouchers" }
    | { view: "receive_bundle" }
    | { view: "transaction_history" }
    | { view: "activities" }
    | { view: "transfer_success"; bundleData: number[]; recipientId: string; summary: string }
    | { view: "receive_success"; payload: ReceiveSuccessPayload }
    | { view: "address_book"; initialSearchQuery?: string; previousView?: AppState }
    | { view: "sign_request"; voucherData: VoucherDetails }
    | { view: "conflict_details"; proofId: string; previousView?: AppState }
    | { view: "conflict_list" }
    | { view: "wallet"; initialStatusFilter?: string; initialStandardFilter?: string };

function AppContent() {
    const [appState, setAppState] = useState<AppState>({ view: "loading" });
    const [profileName, setProfileName] = useState<string>("");
    const [isSidebarOpen, setSidebarOpen] = useState(false);
    const [appVersion, setAppVersion] = useState<string>("");
    
    // Zugriff auf SessionContext
    const { notifyLogin, notifyLogout, isForkLocked, isRecoveryRequired, clearLocks } = useSession();

    useEffect(() => {
        if (isForkLocked || isRecoveryRequired) {
            logger.warn("Global lock detected, switching to recovery view state.");
        }
    }, [isForkLocked, isRecoveryRequired]);

    useEffect(() => {
        getVersion().then(setAppVersion);
    }, []);

    useEffect(() => {
        // Log that the frontend application is starting
        logger.info("Frontend application starting, initializing profile check...");

        async function checkProfile() {
            try {
                // First, check if we are already logged in (session active)
                try {
                    const profile = await profileService.getProfile();
                    const displayName = profile.firstName || profile.id;
                    logger.info(`Auto-login successful for profile: ${displayName}`);
                    setProfileName(displayName);
                    notifyLogin();
                    setAppState({ view: "logged_in" });
                    return; // Exit early if auto-login worked
                } catch (e) {
                    // Not logged in or session locked, proceed with normal flow
                    logger.info("No active session found, checking available profiles.");
                }

                const profiles = await authService.listProfiles();
                setAppState({ view: profiles.length > 0 ? "needs_login" : "needs_profile" });
            } catch (e) {
                error(`Failed to check if profile exists: ${e}`);
                // Fallback to creation if there's an error, as it's the safest default
                setAppState({ view: "needs_profile" });
            }
        }
        checkProfile();
    }, []);

    useEffect(() => {
        async function updateTitle() {
            const win = getCurrentWindow();
            if (profileName) {
                await win.setTitle(`Human Money App - ${profileName}`);
            } else {
                await win.setTitle('Human Money App');
            }
        }

        updateTitle();
    }, [profileName]);

    function handleLogout() {
        profileService.logout().catch(e => error(`Logout failed: ${e}`));
        setSidebarOpen(false);
        // Reset the profile name on logout
        setProfileName("");
        setAppState({ view: "needs_login" });
        notifyLogout();
    }

    function renderContent() {
        switch (appState.view) {
            case "loading":
                return (
                    <div className="flex h-full w-full items-center justify-center">
                        <p className="text-theme-light">Loading application...</p>
                    </div>
                );
            case "needs_profile":
                return <CreateNewProfile
                    onProfileCreated={() => {
                        // SessionContext informieren, dass wir eingeloggt sind
                        notifyLogin();
                        setAppState({ view: "logged_in" });
                    }}
                    onSwitchToRecreate={() => setAppState({ view: "recreate_profile" })}
                    onSwitchToLogin={() => setAppState({ view: "needs_login" })}
                />;
            case "needs_login":
                return <Login
                    onLoginSuccess={(name) => {
                        setProfileName(name);
                        // NEU: SessionContext informieren, dass wir eingeloggt sind!
                        notifyLogin();
                        setAppState({ view: "logged_in" });
                    }}
                    onSwitchToRecreate={() => setAppState({ view: "recreate_profile" })}
                    onSwitchToCreate={() => setAppState({ view: "needs_profile" })}
                    onSwitchToReset={() => setAppState({ view: "needs_recovery" })}
                />;
            case "logged_in":
                return <Dashboard
                    profileName={profileName}
                    onNavigateToCreateVoucher={() => setAppState({ view: "create_voucher" })}
                    onNavigateToSend={() => setAppState({ view: "send_vouchers" })}
                    onNavigateToReceive={() => setAppState({ view: "receive_bundle" })}
                    onNavigateToHistory={() => setAppState({ view: "transaction_history" })}
                    onNavigateToActivities={() => setAppState({ view: "activities" })}
                    onNavigateToConflicts={() => setAppState({ view: "conflict_list" })}
                    onNavigateToWallet={(filter) => setAppState({ view: "wallet", initialStatusFilter: filter?.status, initialStandardFilter: filter?.standard })}
                    onNavigateToSettings={() => setAppState({ view: "settings" })}
                    onNavigateToVoucherDetail={(voucherId) => setAppState({ view: "voucher_details", voucherId, previousView: appState })}
                />;
            case "recreate_profile":
                return <RecreateProfile
                    onProfileCreated={() => {
                        notifyLogin();
                        setAppState({ view: "logged_in" });
                    }}
                    onSwitchToLogin={() => setAppState({ view: "needs_login" })}
                />;
            case "settings":
                return <SettingsView onBack={() => setAppState({ view: "logged_in" })} />;
            case "needs_recovery":
                return <WalletRecovery 
                    onRecoverySuccess={() => {
                        notifyLogin();
                        setAppState({ view: "logged_in" });
                    }} 
                    onSwitchToLogin={() => setAppState({ view: "needs_login" })} 
                />;
            case "create_voucher":
                return <CreateVoucher onVoucherCreated={() => setAppState(appState.previousView || { view: "logged_in" })} onCancel={() => setAppState(appState.previousView || { view: "logged_in" })} />;
            case "voucher_details":
                return <VoucherDetailsView
                    voucherId={appState.voucherId}
                    onBack={() => setAppState(appState.previousView || { view: "logged_in" })}
                    onViewConflict={(proofId) => setAppState({ view: "conflict_details", proofId, previousView: appState })}
                />;
            case "address_book":
                return <AddressBook 
                    onBack={() => setAppState(appState.previousView || { view: "logged_in" })} 
                    initialSearchQuery={appState.initialSearchQuery}
                />;
            case "send_vouchers":
                return <SendView
                    profileName={profileName}
                    onBack={() => setAppState({ view: "logged_in" })}
                    onTransferPrepared={(bundleData, recipientId, summary) =>
                        setAppState({ view: "transfer_success", bundleData, recipientId, summary })
                    }
                />;
            case "receive_bundle":
                return <ReceiveView
                    onBack={() => setAppState({ view: "logged_in" })}
                    onReceiveSuccess={(payload) => {
                        if ('localInstanceId' in payload) {
                            // This is a VoucherDetails (Signature Request)
                            setAppState({ view: "sign_request", voucherData: payload });
                        } else if (payload.isSignatureAttached && payload.voucherId) {
                            // This was a signature attachment success
                            setAppState({ view: "voucher_details", voucherId: payload.voucherId });
                        } else {
                            // Normal transfer received
                            setAppState({ view: "receive_success", payload });
                        }
                    }}
                />;
            case "transaction_history":
                return <TransactionHistoryView onBack={() => setAppState({ view: "logged_in" })} />;
            case "activities":
                return <Activities 
                    onBack={() => setAppState({ view: "logged_in" })} 
                    onNavigateToVoucherDetail={(voucherId) => setAppState({ view: "voucher_details", voucherId, previousView: appState })}
                    onNavigateToHistory={() => setAppState({ view: "transaction_history" })}
                />;
            case "transfer_success":
                return <TransferSuccessView
                    bundleData={appState.bundleData}
                    recipientId={appState.recipientId}
                    summary={appState.summary}
                    onDone={() => setAppState({ view: "logged_in" })}
                />;
            case "receive_success":
                return <ReceiveSuccessView payload={appState.payload} onDone={() => setAppState({ view: "logged_in" })} />;
            case "sign_request":
                return <SignRequestView
                    voucherData={appState.voucherData}
                    onBack={() => setAppState({ view: "logged_in" })}
                />;
            case "conflict_details":
                return <ConflictDetailsView
                    proofId={appState.proofId}
                    onBack={() => setAppState(appState.previousView || { view: "logged_in" })}
                />;
            case "conflict_list":
                return <ConflictListView
                    onBack={() => setAppState({ view: "logged_in" })}
                    onViewConflict={(proofId) => setAppState({ view: "conflict_details", proofId, previousView: { view: "conflict_list" } })}
                />;
            case "wallet":
                return <WalletView
                    profileName={profileName}
                    onShowDetails={(voucherId: string) => setAppState({ view: "voucher_details", voucherId, previousView: appState })}
                    onBack={() => setAppState({ view: "logged_in" })}
                    onNavigateToCreateVoucher={() => setAppState({ view: "create_voucher", previousView: appState })}
                    initialStatusFilter={appState.initialStatusFilter}
                    initialStandardFilter={appState.initialStandardFilter}
                />;
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
                <Sidebar 
                    appState={appState}
                    setAppState={setAppState}
                    profileName={profileName}
                    appVersion={appVersion}
                    onLogout={handleLogout}
                    isOpen={isSidebarOpen}
                    setIsOpen={setSidebarOpen}
                />

                <div className="flex flex-1 flex-col overflow-y-auto">
                    {INTERNAL_VIEWS.includes(appState.view) && (
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
                            <div className="flex items-center gap-2">
                                <img src={logo} alt="Logo" className="w-8 h-8 object-contain" />
                                <h1 className="text-lg font-bold text-theme-primary">Human Money App</h1>
                            </div>
                            <div className="w-10"></div> {/* Spacer for symmetry */}
                        </header>
                    )}

                    <main className={`w-full flex-grow ${INTERNAL_VIEWS.includes(appState.view) ? 'p-4 md:p-6 lg:p-8' : ''}`}>
                        {renderContent()}
                    </main>
                </div>

                {/* Fork detected overlay */}
                {isForkLocked && (
                    <ForkLockOverlay 
                        onStartRecovery={() => {
                            clearLocks();
                            setAppState({ view: "needs_recovery" });
                        }} 
                    />
                )}
                
                {/* Fallback for general recovery required (no seal) */}
                {isRecoveryRequired && !isForkLocked && (
                    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
                         <div className="max-w-md w-full bg-white dark:bg-gray-800 rounded-2xl p-8 text-center shadow-xl">
                            <h2 className="text-xl font-bold mb-4">Recovery Required</h2>
                            <p className="text-theme-light mb-6">Your security seal has been lost. You must perform a recovery to continue using this wallet.</p>
                            <Button 
                                className="w-full"
                                onClick={() => {
                                    clearLocks();
                                    setAppState({ view: "needs_recovery" });
                                }}
                            >
                                Start Recovery
                            </Button>
                        </div>
                    </div>
                )}
            </div>
    );
}

// Wir müssen AppContent in eine Wrapper-Komponente auslagern,
// da useSession nur INNERHALB des SessionProviders funktioniert.
function App() {
    return (
        <SessionProvider>
            <AppContent />
        </SessionProvider>
    );
}

export default App;