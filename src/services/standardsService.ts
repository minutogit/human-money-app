import { invoke } from "@tauri-apps/api/core";
import { VoucherStandardInfo, VoucherStandardDefinition, SignatureImpact } from "../types";

export interface EvaluateSignatureSuitabilityArgs {
    voucher: unknown;
    role: string;
    standardTomlContent: string;
}

export const standardsService = {
    getStandards: async () => {
        return await invoke<VoucherStandardInfo[]>("get_voucher_standards");
    },

    parseStandard: async (tomlContent: string) => {
        return await invoke<VoucherStandardDefinition>("parse_standard_toml", { tomlContent });
    },

    evaluateSignatureSuitability: async (args: EvaluateSignatureSuitabilityArgs) => {
        return await invoke<SignatureImpact>("evaluate_signature_suitability", {
            voucher: args.voucher,
            role: args.role,
            standardTomlContent: args.standardTomlContent
        });
    }
};


