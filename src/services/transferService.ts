import { invoke } from "@tauri-apps/api/core";
import { triggerAutoBackup } from "../utils/autoBackup";
import { TransactionRecord, AssetClassSummary, ReceiveSuccessPayload, WalletEvent, SourceTransfer, IntentDispatchObservation, PurchaseCompletion } from "../types";

interface ReceiveBundleArgs {
    bundleData: number[];
    standardDefinitionsToml: Record<string, string>;
    password?: string;
    forceAcceptToleranceBundle: boolean;
}

export interface CreateBundleArgs {
    recipientId: string;
    sources: SourceTransfer[];
    notes?: string | null;
    senderProfileName: string;
    standardDefinitionsToml: Record<string, string>;
    usePrivacyMode: boolean;
    password?: string | null;
}

export interface CreateBundleResult {
    bundleData: number[];
    bundleId: string;
    involvedSourcesDetails: unknown[];
    l2Dispatch?: IntentDispatchObservation[];
}

export interface ScoreBreakdown {
    sBoomerang: number;
    sEigen: number;
    sCorridor: number;
    sAge: number;
    sChain: number;
    sFit: number;
    sWot?: number;
}

export interface RankedVoucher {
    localInstanceId: string;
    score: number;
    breakdown: ScoreBreakdown;
    reason: string;
    amount: string;
    standardUuid: string;
}

export interface TransferPlan {
    selected: RankedVoucher[];
    totalSelected: string;
    target: string;
    needsSplit: boolean;
    planReason: string;
}

export const transferService = {
    getHistory: async () => {
        return await invoke<TransactionRecord[]>("get_transaction_history");
    },

    prepareTransferPlan: async (args: { targetAmount: string; standardUuidFilter?: string | null; recipientId: string }) => {
        return await invoke<TransferPlan>("prepare_transfer_plan", {
            targetAmount: args.targetAmount,
            standardUuidFilter: args.standardUuidFilter ?? null,
            recipientId: args.recipientId
        });
    },

    rankVouchers: async (args: { targetAmount: string; standardUuidFilter?: string | null; recipientId: string }) => {
        return await invoke<RankedVoucher[]>("rank_vouchers_for_recipient", {
            targetAmount: args.targetAmount,
            standardUuidFilter: args.standardUuidFilter ?? null,
            recipientId: args.recipientId
        });
    },

    createBundle: async (args: CreateBundleArgs) => {
        const result = await invoke<CreateBundleResult>("create_transfer_bundle", {
            recipientId: args.recipientId,
            sources: args.sources.map(s => ({
                localInstanceId: s.localInstanceId,
                amountToSend: s.amountToSend
            })),
            notes: args.notes,
            senderProfileName: args.senderProfileName,
            standardDefinitionsToml: args.standardDefinitionsToml,
            usePrivacyMode: args.usePrivacyMode,
            password: args.password
        });
        triggerAutoBackup();
        return result;
    },

    saveTransactionRecord: async (record: TransactionRecord, password?: string) => {
        const result = await invoke<void>("save_transaction_record", { record, password });
        triggerAutoBackup();
        return result;
    },

    getActiveAssetClasses: async () => {
        return await invoke<AssetClassSummary[]>("get_active_asset_classes");
    },

    receiveBundle: async (args: ReceiveBundleArgs) => {
        const result = await invoke<ReceiveSuccessPayload>("receive_bundle", {
            bundleData: args.bundleData,
            standardDefinitionsToml: args.standardDefinitionsToml,
            password: args.password,
            forceAcceptToleranceBundle: args.forceAcceptToleranceBundle
        });
        triggerAutoBackup();
        return result;
    },

    getEventHistory: async (offset: number, limit: number) => {
        return await invoke<WalletEvent[]>("get_event_history", { offset, limit });
    },

    verifyLiveCheckoutAndRelease: async (
        proofJson: string,
        expectedContextJson: string,
        regionalViewsJson?: string | null,
        explicitMerchantApproval: boolean = false,
        password?: string | null
    ) => {
        return await invoke<import("../types").CheckoutReleaseVerdict>("verify_live_checkout_and_release", {
            proofJson,
            expectedContextJson,
            regionalViewsJson: regionalViewsJson ?? null,
            explicitMerchantApproval,
            password: password ?? null,
        });
    },

    evaluateCheckoutRelease: async (outcome: string, explicitMerchantApproval: boolean = false) => {
        return await invoke<import("../types").CheckoutReleaseVerdict>("evaluate_checkout_release", {
            outcome,
            explicitMerchantApproval,
        });
    },

    verifyCheckoutResponseWithAuthority: async (args: {
        localId: string;
        responseBytes: number[];
        network: number;
        shardId: number;
        parentLockHex: string;
        childLockHex: string;
        committeePubkeysHex: string[];
        explicitMerchantApproval?: boolean;
        password?: string;
    }) => {
        return await invoke<import("../types").CheckoutReleaseVerdict>("verify_checkout_response_with_authority", {
            localId: args.localId,
            responseBytes: args.responseBytes,
            network: args.network,
            shardId: args.shardId,
            parentLockHex: args.parentLockHex,
            childLockHex: args.childLockHex,
            committeePubkeysHex: args.committeePubkeysHex,
            explicitMerchantApproval: args.explicitMerchantApproval ?? false,
            password: args.password,
        });
    },

    checkoutReceivedBundle: async (args: {
        purchaseId: string;
        bundleData: number[];
        standardDefinitionsToml: Record<string, string>;
        password?: string | null;
    }) => {
        const result = await invoke<PurchaseCompletion>("checkout_received_bundle", {
            purchaseId: args.purchaseId,
            bundleData: args.bundleData,
            standardDefinitionsToml: args.standardDefinitionsToml,
            password: args.password ?? null,
        });
        triggerAutoBackup();
        return result;
    },

    submitL2SendIntent: async (args: {
        localInstanceId: string;
        password?: string | null;
    }) => {
        return await invoke<IntentDispatchObservation>("submit_l2_send_intent", {
            localInstanceId: args.localInstanceId,
            password: args.password ?? null,
        });
    }
};
