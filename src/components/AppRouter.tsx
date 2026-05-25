import React from "react";
import { ReceiveSuccessPayload, VoucherDetails } from "../types";
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
import { useNavigation } from '../context/NavigationContext';
import { SupportView } from './SupportView';

export const AppRouter: React.FC = () => {
    const { notifyLogin } = useSession();
    const { appState, navigate, goBack } = useNavigation();

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
                    navigate({ view: "logged_in" });
                }}
                onSwitchToRecreate={() => navigate({ view: "recreate_profile" })}
                onSwitchToLogin={() => navigate({ view: "needs_login" })}
            />;
        case "needs_login":
            return <Login
                onLoginSuccess={(name) => {
                    notifyLogin(name);
                    navigate({ view: "logged_in" });
                }}
                onSwitchToRecreate={() => navigate({ view: "recreate_profile" })}
                onSwitchToCreate={() => navigate({ view: "needs_profile" })}
                onSwitchToReset={() => navigate({ view: "needs_recovery" })}
            />;
        case "logged_in":
            return <Dashboard />;
        case "recreate_profile":
            return <RecreateProfile
                onProfileCreated={() => {
                    notifyLogin();
                    navigate({ view: "logged_in" });
                }}
                onSwitchToLogin={() => navigate({ view: "needs_login" })}
            />;
        case "settings":
            return <SettingsView onBack={() => navigate({ view: "logged_in" })} />;
        case "support":
            return <SupportView />;
        case "needs_recovery":
            return <WalletRecovery 
                onRecoverySuccess={() => {
                    notifyLogin();
                    navigate({ view: "logged_in" });
                }} 
                onSwitchToLogin={() => navigate({ view: "needs_login" })} 
            />;
        case "create_voucher":
            return <CreateVoucher 
                onVoucherCreated={goBack} 
                onCancel={goBack} 
            />;
        case "voucher_details":
            return <VoucherDetailsView />;
        case "address_book":
            return <AddressBook 
                onBack={goBack} 
                initialSearchQuery={appState.initialSearchQuery}
            />;
        case "send_vouchers":
            return <SendView />;
        case "receive_bundle":
            return <ReceiveView
                onBack={() => navigate({ view: "logged_in" })}
                onReceiveSuccess={(payload) => {
                    if ('localInstanceId' in payload) {
                        navigate({ view: "sign_request", voucherData: payload as VoucherDetails });
                    } else if (payload.isSignatureAttached && payload.voucherId) {
                        navigate({ view: "voucher_details", voucherId: payload.voucherId });
                    } else {
                        navigate({ view: "receive_success", payload: payload as ReceiveSuccessPayload });
                    }
                }}
            />;
        case "transaction_history":
            return <TransactionHistoryView />;
        case "activities":
            return <Activities />;
        case "transfer_success":
            return <TransferSuccessView
                bundleData={appState.bundleData}
                recipientId={appState.recipientId}
                summary={appState.summary}
                onDone={() => navigate({ view: "logged_in" })}
            />;
        case "receive_success":
            return <ReceiveSuccessView payload={appState.payload} onDone={() => navigate({ view: "logged_in" })} />;
        case "sign_request":
            return <SignRequestView
                voucherData={appState.voucherData}
                onBack={() => navigate({ view: "logged_in" })}
            />;
        case "conflict_details":
            return <ConflictDetailsView
                proofId={appState.proofId}
                onBack={goBack}
            />;
        case "conflict_list":
            return <ConflictListView
                onBack={() => navigate({ view: "logged_in" })}
                onViewConflict={(proofId) => navigate({ view: "conflict_details", proofId, previousView: { view: "conflict_list" } })}
            />;
        case "wallet":
            return <WalletView />;
        default:
            return (
                <div className="flex h-full w-full items-center justify-center">
                    <p className="text-theme-error">Error: Invalid application state.</p>
                </div>
            );
    }
};
