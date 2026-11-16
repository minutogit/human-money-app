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
import { ReceiveView } from './ReceiveView';
import { ReceiveSuccessView } from './ReceiveSuccessView';
import { Dashboard } from './Dashboard';
import { WalletView } from './WalletView';
import { SignRequestView } from './SignRequestView';
import { WalletRecovery } from './WalletRecovery';
import { RestoreBackupView } from './RestoreBackupView';
import AddressBook from './AddressBook';
import { ConflictDetailsView } from './ConflictDetailsView';
import { ConflictListView } from './ConflictListView';
import { authService } from '../services/authService';
import { useSession } from '../context/SessionContext';
import { useNavigation } from '../context/NavigationContext';
import { SupportView } from './SupportView';
import { BugReportView } from './BugReportView';
import { HelpCenterView } from './HelpCenterView';
import { MarketplaceView } from './marketplace/MarketplaceView';

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
                onSwitchToLogin={() => navigate({ view: "needs_login" })}
                onSwitchToRestore={(method) =>
                    navigate({
                        view: "restore_backup",
                        ...(method ? { initialMethod: method } : {}),
                    })
                }
            />;
        case "needs_login":
            return <Login
                onLoginSuccess={(name) => {
                    notifyLogin(name);
                    navigate({ view: "logged_in" });
                }}
                onSwitchToRestoreBackup={() => navigate({ view: "restore_backup" })}
                onSwitchToCreate={() => navigate({ view: "needs_profile" })}
                onSwitchToReset={() => navigate({ view: "needs_recovery" })}
            />;
        case "logged_in":
            return <Dashboard />;
        case "restore_backup":
            return <RestoreBackupView
                initialPayload={appState.initialPayload}
                initialBackupPath={appState.initialBackupPath}
                initialMethod={appState.initialMethod}
                onRestoreSuccess={() => {
                    notifyLogin();
                    navigate({ view: "logged_in" });
                }}
                onBack={async () => {
                    try {
                        const profiles = await authService.listProfiles();
                        if (!profiles || profiles.length === 0) {
                            navigate({ view: "needs_profile" });
                        } else {
                            navigate({ view: "needs_login" });
                        }
                    } catch {
                        navigate({ view: "needs_login" });
                    }
                }}
            />;
        case "settings":
            return <SettingsView onBack={goBack} />;
        case "support":
            return <SupportView onBack={goBack} />;
        case "bug_report":
            return <BugReportView onBack={goBack} />;
        case "help_center":
            return <HelpCenterView initialTopic={appState.topic} onBack={goBack} />;
        case "needs_recovery":
            return <WalletRecovery 
                initialPayload={appState.initialPayload}
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
                initialOpenAddModal={appState.initialOpenAddModal}
                initialContactPayload={appState.initialContactPayload}
            />;
        case "send_vouchers":
            return <SendView
                initialRecipientId={appState.initialRecipientId}
                initialVoucherId={appState.initialVoucherId}
                initialNote={appState.initialNote}
                initialPaymentUri={appState.initialPaymentUri}
            />;
        case "receive_bundle":
            return <ReceiveView
                droppedFile={appState.droppedFile}
                onBack={goBack}
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
        case "transfer_success":
            return <TransferSuccessView
                bundleData={appState.bundleData}
                recipientId={appState.recipientId}
                summary={appState.summary}
                l2Dispatch={appState.l2Dispatch}
                bundleId={appState.bundleId}
                initialDepositedTo={appState.depositedTo}
                depositError={appState.depositError}
                onDone={() => navigate({ view: "logged_in" })}
            />;
        case "receive_success":
            return <ReceiveSuccessView payload={appState.payload} onDone={() => navigate({ view: "logged_in" })} />;
        case "sign_request":
            return <SignRequestView
                voucherData={appState.voucherData}
                requestMsgId={appState.requestMsgId}
                senderDid={appState.senderDid}
                onBack={goBack}
            />;
        case "conflict_details":
            return <ConflictDetailsView
                proofId={appState.proofId}
                onBack={goBack}
            />;
        case "conflict_list":
            return <ConflictListView
                onBack={goBack}
                onViewConflict={(proofId) => navigate({ view: "conflict_details", proofId, previousView: { view: "conflict_list" } })}
            />;
        case "wallet":
            return <WalletView />;
        case "marketplace":
            return (
                <MarketplaceView
                    onBack={goBack}
                    initialSelectedDid={appState.selectedDid}
                    initialTab={appState.initialTab}
                    initialOpenEditor={appState.initialOpenEditor}
                />
            );
        default:
            return (
                <div className="flex h-full w-full items-center justify-center">
                    <p className="text-theme-error">Error: Invalid application state.</p>
                </div>
            );
    }
};
