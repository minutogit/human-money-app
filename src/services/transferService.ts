import { invoke } from "@tauri-apps/api/core";
import { TransactionRecord, AssetClassSummary } from "../types";

export const transferService = {
    getHistory: async () => {
        return await invoke<TransactionRecord[]>("get_transaction_history");
    },

    createBundle: async (args: any) => {
        return await invoke<any>("create_transfer_bundle", args);
    },

    saveTransactionRecord: async (record: TransactionRecord, password?: string) => {
        return await invoke<void>("save_transaction_record", { record, password });
    },

    getActiveAssetClasses: async () => {
        return await invoke<AssetClassSummary[]>("get_active_asset_classes");
    },

    receiveBundle: async (args: ReceiveBundleArgs) => {
        return await invoke<ReceiveSuccessPayload>("receive_bundle", args as any);
    },

    getEventHistory: async (offset: number, limit: number) => {
        return await invoke<any[]>("get_event_history", { offset, limit });
    }
};


interface ReceiveBundleArgs {
    bundleData: number[];
    standardDefinitionsToml: Record<string, string>;
    password?: string;
    forceAcceptToleranceBundle: boolean;
}

import { ReceiveSuccessPayload } from "../types";

