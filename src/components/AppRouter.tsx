import React from "react";
import { AppState, ReceiveSuccessPayload, VoucherDetails } from "../types";
import { CreateNewProfile } from "./CreateNewProfile";
import { Login } from "./Login";
import { CreateVoucher } from "./CreateVoucher";
import { SendView } from "./SendView";
import { SettingsView } from "./SettingsView";
import { TransactionHistoryView } from "./TransactionHistoryView";
import { TransferSuccessView } from "./TransferSuccessView";
import { VoucherDetailsView } from './VoucherDetailsView';
import { Activities } from './Activities';
import { ReceiveView } from './ReceiveView';
import { ReceiveSuccessView } from './ReceiveSuccessView';
import { Dashboard } from './Dashboard';
import { WalletView } from './WalletView';
import { SignRequestView } from './SignRequestView';
import { WalletRecovery } from './WalletRecovery';
import { RecreateProfile } from './RecreateProfile';
import AddressBook from './AddressBook';
import { ConflictDetailsView } from './ConflictDetailsView';
import { ConflictListView } from './ConflictListView';
import { useSession } from '../context/SessionContext';

interface AppRouterProps {
    appState: AppState;
    setAppState: React.Dispatch<React.SetStateAction<AppState>>;
    profileName: string;
    setProfileName: React.Dispatch<React.SetStateAction<string>>;
}

export const AppRouter: React.FC<AppRouterProps> = ({
    appState,
    setAppState,
    profileName,
    setProfileName
}) => {
    const { notifyLogin } = useSession();

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
                        setAppState({ view: "sign_request", voucherData: payload as VoucherDetails });
                    } else if (payload.isSignatureAttached && payload.voucherId) {
                        setAppState({ view: "voucher_details", voucherId: payload.voucherId });
                    } else {
                        setAppState({ view: "receive_success", payload: payload as ReceiveSuccessPayload });
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
};
