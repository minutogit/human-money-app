import { invoke } from "@tauri-apps/api/core";
import { VoucherSummary, VoucherDetails, NewVoucherData, Voucher, TrustStatus, AggregatedBalance } from "../types";

export type SigningRequestConfig = 
    | { type: "Cleartext" }
    | { type: "Password"; value: string }
    | { type: "TargetDid"; value: [string, string] };

export interface DetachedSignatureResponseArgs {
    voucher: unknown;
    role: string;
    includeDetails: boolean;
    config: SigningRequestConfig;
    password?: string | null;
}

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

    createSigningRequest: async (localInstanceId: string, config: SigningRequestConfig) => {
        return await invoke<number[]>("create_signing_request_bundle", { 
            local_instance_id: localInstanceId, 
            config 
        });
    },

    openSigningRequest: async (containerBytes: number[], password?: string) => {
        return await invoke<VoucherDetails>("open_voucher_signing_request", { containerBytes, password });
    },

    createDetachedSignatureResponseBundle: async (args: DetachedSignatureResponseArgs) => {
        return await invoke<number[]>("create_detached_signature_response_bundle", {
            voucher: args.voucher,
            role: args.role,
            include_details: args.includeDetails,
            config: args.config,
            password: args.password
        });
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
