import React from "react";
import { ReceiveSuccessPayload, VoucherDetails } from "../../types";
import { CreateNewProfile } from "../auth/CreateNewProfile";
import { Login } from "../auth/Login";
import { CreateVoucher } from "../wallet/CreateVoucher";
import { SendView } from "../transfer/SendView";
import { SettingsView } from "../settings/SettingsView";
import { TransactionHistoryView } from "../transfer/TransactionHistoryView";
import { TransferSuccessView } from "../transfer/TransferSuccessView";
import { VoucherDetailsView } from '../wallet/VoucherDetailsView';
import { ReceiveView } from '../transfer/ReceiveView';
import { ReceiveSuccessView } from '../transfer/ReceiveSuccessView';
import { Dashboard } from '../wallet/Dashboard';
import { WalletView } from '../wallet/WalletView';
import { SignRequestView } from '../wallet/SignRequestView';
import { WalletRecovery } from '../auth/WalletRecovery';
import { RestoreBackupView } from '../auth/RestoreBackupView';
import AddressBook from '../contacts/AddressBook';
import { ConflictDetailsView } from '../conflicts/ConflictDetailsView';
import { ConflictListView } from '../conflicts/ConflictListView';
import { authService } from '../../services/authService';
import { useSession } from '../../context/SessionContext';
import { useNavigation } from '../../context/NavigationContext';
import { SupportView } from '../support/SupportView';
import { BugReportView } from '../support/BugReportView';
import { HelpCenterView } from '../support/HelpCenterView';
import { MarketplaceView } from '../marketplace/MarketplaceView';
import { useTranslation } from 'react-i18next';

interface AppRouterProps {
    onFinishLaunchItem?: (launchItem: string) => void;
}

export const AppRouter: React.FC<AppRouterProps> = ({ onFinishLaunchItem }) => {
    const { t } = useTranslation();
    const { notifyLogin, isAuthenticated } = useSession();
    const { appState, navigate, goBack } = useNavigation();

    switch (appState.view) {
        case "loading":
            return (
                <div className="flex h-full w-full items-center justify-center">
                    <p className="text-theme-light">{t('common.loadingApp', 'App wird geladen...')}</p>
                </div>
            );
        case "needs_profile":
            return <CreateNewProfile
                onProfileCreated={(name, folder) => {
                    notifyLogin(name, folder);
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
                onLoginSuccess={(name, folder) => {
                    notifyLogin(name, folder);
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
                onRestoreSuccess={(outcome) => {
                    notifyLogin(outcome?.profileName, outcome?.folderName || undefined);
                    navigate({ view: "logged_in" });
                }}
                onBack={async () => {
                    if (isAuthenticated) {
                        navigate({ view: "logged_in" });
                        return;
                    }
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
                onRecoverySuccess={async (folder, profileName) => {
                    let resolvedName = profileName;
                    if (!resolvedName && folder) {
                        try {
                            const profiles = await authService.listProfiles();
                            resolvedName = profiles.find((p) => p.folderName === folder)?.profileName;
                        } catch {
                            // fallback below
                        }
                    }
                    notifyLogin(resolvedName, folder);
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
        case "receive_bundle": {
            const droppedLaunchItem = appState.droppedFile?.launchItem;
            return <ReceiveView
                droppedFile={appState.droppedFile}
                onFinishLaunchItem={onFinishLaunchItem}
                onBack={goBack}
                onReceiveSuccess={(payload, pendingCommit) => {
                    if ('localInstanceId' in payload) {
                        if (droppedLaunchItem) {
                            onFinishLaunchItem?.(droppedLaunchItem);
                        }
                        const details = payload as VoucherDetails;
                        navigate({
                            view: "sign_request",
                            voucherData: details,
                            anchorManifestJson: details.anchorManifestJson,
                        });
                    } else if (payload.isSignatureAttached && payload.voucherId) {
                        if (droppedLaunchItem) {
                            onFinishLaunchItem?.(droppedLaunchItem);
                        }
                        navigate({ view: "voucher_details", voucherId: payload.voucherId });
                    } else {
                        navigate({
                            view: "receive_success",
                            payload: payload as ReceiveSuccessPayload,
                            pendingCommit,
                            launchItem: pendingCommit ? droppedLaunchItem : undefined,
                        });
                    }
                }}
            />;
        }
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
                initialRecipientDepositedTo={appState.recipientDepositedTo}
                depositError={appState.depositError}
                onDone={() => navigate({ view: "logged_in" })}
            />;
        case "receive_success":
            return <ReceiveSuccessView
                payload={appState.payload}
                pendingCommit={appState.pendingCommit}
                launchItem={appState.launchItem}
                onFinishLaunchItem={onFinishLaunchItem}
                onDone={() => navigate({ view: "logged_in" })}
            />;
        case "sign_request":
            return <SignRequestView
                voucherData={appState.voucherData}
                requestMsgId={appState.requestMsgId}
                senderDid={appState.senderDid}
                anchorManifestJson={appState.anchorManifestJson}
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
                    <p className="text-theme-error">{t('common.invalidAppState', 'Fehler: Ungültiger Anwendungszustand.')}</p>
                </div>
            );
    }
};
