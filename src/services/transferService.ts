import { invoke } from "@tauri-apps/api/core";
import { TransactionRecord, AssetClassSummary, ReceiveSuccessPayload, WalletEvent, SourceTransfer } from "../types";

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
}

export const transferService = {
    getHistory: async () => {
        return await invoke<TransactionRecord[]>("get_transaction_history");
    },

    createBundle: async (args: CreateBundleArgs) => {
        return await invoke<CreateBundleResult>("create_transfer_bundle", {
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
    },

    saveTransactionRecord: async (record: TransactionRecord, password?: string) => {
        return await invoke<void>("save_transaction_record", { record, password });
    },

    getActiveAssetClasses: async () => {
        return await invoke<AssetClassSummary[]>("get_active_asset_classes");
    },

    receiveBundle: async (args: ReceiveBundleArgs) => {
        return await invoke<ReceiveSuccessPayload>("receive_bundle", {
            bundleData: args.bundleData,
            standardDefinitionsToml: args.standardDefinitionsToml,
            password: args.password,
            forceAcceptToleranceBundle: args.forceAcceptToleranceBundle
        });
    },

    getEventHistory: async (offset: number, limit: number) => {
        return await invoke<WalletEvent[]>("get_event_history", { offset, limit });
    }
};
