import { invoke } from "@tauri-apps/api/core";
import { afterWalletMutation } from "../sync/syncBridge";
import { notifyPostInboxUpdated } from "./postService";
import {
    TransactionRecord,
    AssetClassSummary,
    ReceiveCommitMode,
    ReceiveSuccessPayload,
    WalletEvent,
    SourceTransfer,
    IntentDispatchObservation,
    FrontendPostDepositResult,
} from "../types";

interface ReceiveBundleArgs {
    bundleData: number[];
    standardDefinitionsToml: Record<string, string>;
    password?: string;
    forceAcceptToleranceBundle: boolean;
    commitMode?: ReceiveCommitMode;
    purchaseId?: string;
}

export interface CreateBundleArgs {
    recipientId: string;
    sources: SourceTransfer[];
    notes?: string | null;
    senderProfileName: string | null;
    standardDefinitionsToml: Record<string, string>;
    usePrivacyMode: boolean;
    sendCardOverride?: boolean | null;
    depositToPost?: boolean;
    password?: string | null;
}

export interface CreateBundleResult {
    bundleData: number[];
    bundleId: string;
    involvedSourcesDetails: unknown[];
    l2Dispatch?: IntentDispatchObservation[];
    postDeposit?: FrontendPostDepositResult;
    postDepositError?: string;
}

export interface ScoreBreakdown {
    sBoomerang: number;
    sEigen: number;
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
    minRecipientTrustPercent?: number | null;
}

export const transferService = {
    getHistory: async () => {
        return await invoke<TransactionRecord[]>("get_transaction_history");
    },

    prepareTransferPlan: async (args: {
        targetAmount: string;
        standardUuidFilter?: string | null;
        recipientId: string;
        isTestVoucher?: boolean | null;
    }) => {
        return await invoke<TransferPlan>("prepare_transfer_plan", {
            targetAmount: args.targetAmount,
            standardUuidFilter: args.standardUuidFilter ?? null,
            recipientId: args.recipientId,
            isTestVoucher: args.isTestVoucher ?? null,
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
            sendCardOverride: args.sendCardOverride ?? null,
            depositToPost: args.depositToPost ?? false,
            password: args.password
        });
        afterWalletMutation(args.password);
        if (result?.postDeposit) {
            notifyPostInboxUpdated();
        }
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
            forceAcceptToleranceBundle: args.forceAcceptToleranceBundle,
            commitMode: args.commitMode ?? "onlineVerify",
            purchaseId: args.purchaseId ?? null,
        });
        if (result?.newlyCommitted !== false) {
            afterWalletMutation(args.password);
        }
        return result;
    },

    getEventHistory: async (offset: number, limit: number) => {
        return await invoke<WalletEvent[]>("get_event_history", { offset, limit });
    },
};
