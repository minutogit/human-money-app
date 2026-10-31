import { invoke } from "@tauri-apps/api/core";
import { triggerAutoBackup } from "../utils/autoBackup";
import { WotSummaryDto, WotImportResultDto, VoucherTrustDto } from "../types";

export interface ExportWotPackageArgs {
    recipientDid?: string | null;
    containerPassword?: string | null;
    password?: string | null;
}

export interface ImportWotPackageArgs {
    containerBytes: number[];
    containerPassword?: string | null;
    password?: string | null;
}

export const wotService = {
    getSummary: async (): Promise<WotSummaryDto> => {
        return await invoke<WotSummaryDto>("get_wot_summary");
    },

    exportPackage: async (args: ExportWotPackageArgs = {}): Promise<number[]> => {
        return await invoke<number[]>("export_wot_package", {
            recipientDid: args.recipientDid,
            containerPassword: args.containerPassword,
            password: args.password,
        });
    },

    importPackage: async (args: ImportWotPackageArgs): Promise<WotImportResultDto> => {
        const result = await invoke<WotImportResultDto>("import_wot_package", {
            containerBytes: args.containerBytes,
            containerPassword: args.containerPassword,
            password: args.password,
        });
        triggerAutoBackup();
        return result;
    },

    evaluateVoucherTrustPreview: async (
        creatorId: string,
        guarantorIds: string[]
    ): Promise<VoucherTrustDto> => {
        return await invoke<VoucherTrustDto>("evaluate_voucher_trust_preview", {
            creatorId,
            guarantorIds,
        });
    },
};
