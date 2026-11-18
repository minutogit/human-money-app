import { invoke } from "@tauri-apps/api/core";
import { afterWalletMutation } from "../sync/autoBackup";
import {
    VoucherSummary,
    VoucherDetails,
    NewVoucherData,
    Voucher,
    TrustStatus,
    AggregatedBalance,
    GuarantorAnchorCheckDto,
    ClaimBundleDto,
    DeadQuorumAdjudicationDto,
} from "../types";

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
    anchorManifestJson?: string | null;
}

export const voucherService = {
    getSummaries: async () => {
        return await invoke<VoucherSummary[]>("get_voucher_summaries");
    },

    getDetails: async (localId: string) => {
        return await invoke<VoucherDetails>("get_voucher_details", { localId });
    },

    create: async (standardTomlContent: string, data: NewVoucherData, password?: string) => {
        const result = await invoke<VoucherDetails>("create_new_voucher", { standardTomlContent, data, password });
        afterWalletMutation(password);
        return result;
    },

    removeSignature: async (localInstanceId: string, signatureId: string, password?: string) => {
        const result = await invoke<void>("remove_voucher_signature", { localInstanceId, signatureId, password });
        afterWalletMutation(password);
        return result;
    },
    evaluateGuarantorAnchorCheck: async (args: {
        voucher: Voucher;
        standardTomlContent: string;
        manifestJson?: string | null;
    }) => {
        return await invoke<GuarantorAnchorCheckDto>("evaluate_guarantor_anchor_check", {
            voucher: args.voucher,
            standardTomlContent: args.standardTomlContent,
            manifestJson: args.manifestJson ?? null,
        });
    },

    createSigningRequest: async (localInstanceId: string, config: SigningRequestConfig) => {
        return await invoke<number[]>("create_signing_request_bundle", { 
            localInstanceId, 
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
            includeDetails: args.includeDetails,
            config: args.config,
            password: args.password,
            anchorManifestJson: args.anchorManifestJson ?? null,
        });
    },

    processAndAttachSignature: async (containerBytes: number[], standardTomlContent: string, containerPassword?: string, walletPassword?: string) => {
        const result = await invoke<string>("process_and_attach_signature", { containerBytes, standardTomlContent, containerPassword, walletPassword });
        afterWalletMutation(walletPassword);
        return result;
    },

    checkReputation: async (offenderId: string) => {
        return await invoke<TrustStatus>("check_reputation", { offenderId });
    },

    getProofId: async (localId: string) => {
        return await invoke<string | null>("get_proof_id_for_voucher", { localId });
    },

    getTotalBalanceByCurrency: async () => {
        return await invoke<AggregatedBalance[]>("get_total_balance_by_currency");
    },

    getUserId: async () => {
        return await invoke<string>("get_user_id");
    },

    createGuarantorClaimBundle: async (args: {
        localInstanceId: string;
        guarantorId: string;
        amount: string;
        standardToml: string;
        notes?: string | null;
        password?: string | null;
    }) => {
        const result = await invoke<ClaimBundleDto>("create_guarantor_claim_bundle", {
            localInstanceId: args.localInstanceId,
            guarantorId: args.guarantorId,
            amount: args.amount,
            standardToml: args.standardToml,
            notes: args.notes ?? null,
            password: args.password ?? null,
        });
        afterWalletMutation(args.password);
        return result;
    },

    createDeadQuorumSwapOffer: async (args: {
        deadAnchorIdHex: string;
        replacementAnchorIdHex: string;
        claimWindowSecs?: number | null;
        powBits?: number | null;
        password?: string | null;
    }) => {
        return await invoke<string>("create_dead_quorum_swap_offer_cmd", {
            deadAnchorIdHex: args.deadAnchorIdHex,
            replacementAnchorIdHex: args.replacementAnchorIdHex,
            claimWindowSecs: args.claimWindowSecs ?? null,
            powBits: args.powBits ?? null,
            password: args.password ?? null,
        });
    },

    createDeadQuorumSwapClaim: async (args: {
        offerJson: string;
        localInstanceId: string;
        password?: string | null;
    }) => {
        return await invoke<string>("create_dead_quorum_swap_claim_cmd", {
            offerJson: args.offerJson,
            localInstanceId: args.localInstanceId,
            password: args.password ?? null,
        });
    },

    adjudicateDeadQuorumSwap: async (args: {
        offerJson: string;
        submissionsJson: string[];
        standardToml: string;
        password?: string | null;
    }) => {
        const result = await invoke<DeadQuorumAdjudicationDto>("adjudicate_dead_quorum_swap_cmd", {
            offerJson: args.offerJson,
            submissionsJson: args.submissionsJson,
            standardToml: args.standardToml,
            password: args.password ?? null,
        });
        afterWalletMutation(args.password);
        return result;
    }
};
