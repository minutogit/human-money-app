import { invoke } from "@tauri-apps/api/core";
import { VoucherSummary, VoucherDetails, NewVoucherData, Voucher, TrustStatus, AggregatedBalance } from "../types";

export const voucherService = {
    getSummaries: async () => {
        return await invoke<VoucherSummary[]>("get_voucher_summaries");
    },

    getDetails: async (localId: string) => {
        return await invoke<VoucherDetails>("get_voucher_details", { localId });
    },

    create: async (standardTomlContent: string, data: NewVoucherData, password?: string) => {
        return await invoke<Voucher>("create_new_voucher", { standardTomlContent, data, password });
    },

    removeSignature: async (localInstanceId: string, signatureId: string, password?: string) => {
        return await invoke<void>("remove_voucher_signature", { localInstanceId, signatureId, password });
    },

    createSigningRequest: async (localInstanceId: string, config: any) => {
        return await invoke<number[]>("create_signing_request_bundle", { localInstanceId, config });
    },

    openSigningRequest: async (containerBytes: number[], password?: string) => {
        return await invoke<VoucherDetails>("open_voucher_signing_request", { containerBytes, password });
    },

    createDetachedSignatureResponseBundle: async (args: any) => {
        return await invoke<number[]>("create_detached_signature_response_bundle", args);
    },


    processAndAttachSignature: async (containerBytes: number[], standardTomlContent: string, containerPassword?: string, walletPassword?: string) => {
        return await invoke<string>("process_and_attach_signature", { containerBytes, standardTomlContent, containerPassword, walletPassword });
    },

    checkReputation: async (offenderId: string) => {
        return await invoke<TrustStatus>("check_reputation", { offenderId });
    },

    getSourceSender: async (localId: string) => {
        return await invoke<string | null>("get_voucher_source_sender", { localId });
    },

    getProofId: async (localId: string) => {
        return await invoke<string | null>("get_proof_id_for_voucher", { localId });
    },

    getTotalBalanceByCurrency: async () => {
        return await invoke<AggregatedBalance[]>("get_total_balance_by_currency");
    }
};


export const utilityService = {
    getUserId: async () => {
        return await invoke<string>("get_user_id");
    }
};
