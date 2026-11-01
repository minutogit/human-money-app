import { invoke } from "@tauri-apps/api/core";
import { triggerAutoBackup } from "../utils/autoBackup";
import { WotSummaryDto, WotImportResultDto, VoucherTrustDto, WotProviderDto } from "../types";

export interface ExportWotPackageArgs {
    recipientDid?: string | null;
    containerPassword?: string | null;
    shareWithAllContacts?: boolean | null;
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

    setMaxHops: async (maxHops: number, password?: string | null): Promise<WotSummaryDto> => {
        const result = await invoke<WotSummaryDto>("set_wot_max_hops", {
            maxHops,
            password: password ?? null,
        });
        triggerAutoBackup();
        return result;
    },

    exportPackage: async (args: ExportWotPackageArgs = {}): Promise<number[]> => {
        return await invoke<number[]>("export_wot_package", {
            recipientDid: args.recipientDid,
            containerPassword: args.containerPassword,
            shareWithAllContacts: args.shareWithAllContacts ?? null,
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

    getProviders: async (): Promise<WotProviderDto[]> => {
        return await invoke<WotProviderDto[]>("get_wot_providers");
    },

    setProviderTrust: async (
        issuerDid: string,
        level: number,
        password?: string | null
    ): Promise<WotProviderDto[]> => {
        const result = await invoke<WotProviderDto[]>("set_wot_provider_trust", {
            issuerDid,
            level,
            password: password ?? null,
        });
        triggerAutoBackup();
        return result;
    },

    removeProvider: async (
        issuerDid: string,
        password?: string | null
    ): Promise<WotProviderDto[]> => {
        const result = await invoke<WotProviderDto[]>("remove_wot_provider", {
            issuerDid,
            password: password ?? null,
        });
        triggerAutoBackup();
        return result;
    },

    refreshProviderUrl: async (
        issuerDid: string,
        password?: string | null
    ): Promise<WotProviderDto> => {
        const result = await invoke<WotProviderDto>("refresh_wot_provider_url", {
            issuerDid,
            password: password ?? null,
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
