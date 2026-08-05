import { invoke } from "@tauri-apps/api/core";
import { VoucherStandardInfo, VoucherStandardDefinition, SignatureImpact, ContainerConfig } from "../types";

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
    },

    exportStandard: async (standardId: string, config: ContainerConfig) => {
        return await invoke<number[]>("export_voucher_standard", { standardId, config });
    },

    inspectStandardFile: async (containerBytes: number[], password?: string | null) => {
        return await invoke<VoucherStandardDefinition>("inspect_voucher_standard_file", { containerBytes, password });
    },

    importStandard: async (containerBytes: number[], password?: string | null) => {
        return await invoke<string>("import_voucher_standard", { containerBytes, password });
    },

    deleteStandard: async (standardId: string) => {
        return await invoke<void>("delete_voucher_standard", { standardId });
    }
};


