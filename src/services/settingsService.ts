import { invoke } from "@tauri-apps/api/core";
import {
    AnchorServerInputDto,
    AppSettings,
    L2LockStatusDto,
    LockServerProbeResult,
    NominatedLockServerMeta,
    ReachPresetDto,
    ReachPresetId,
    RollbackReport,
    StoredAnchorTemplateMeta,
    VoucherL2StatusDto,
} from "../types";

export type { RollbackReport, VoucherL2StatusDto, L2LockStatusDto };

export interface SealForUpload {
    sealHash: string;
    sealBytes: number[];
}

export interface AssembleAnchorTemplateArgs {
    label: string;
    retainUntilQuarterStr: string;
    servers: AnchorServerInputDto[];
    guarantorMigPubsHex?: string[];
    threshold?: number | null;
    password?: string | null;
}

export const settingsService = {
    getSettings: async () => {
        return await invoke<AppSettings>("get_app_settings");
    },

    saveSettings: async (settings: AppSettings, password?: string) => {
        return await invoke<void>("save_app_settings", { settings, password });
    },

    getReachPreset: async (password?: string): Promise<ReachPresetDto> => {
        return await invoke<ReachPresetDto>("get_reach_preset", { password });
    },

    applyReachPreset: async (preset: ReachPresetId, password?: string): Promise<ReachPresetDto> => {
        return await invoke<ReachPresetDto>("apply_reach_preset", { preset, password });
    },

    getSealSyncStatus: async () => {
        return await invoke<string>("get_seal_sync_status");
    },

    getSealForUpload: async () => {
        return await invoke<SealForUpload>("get_seal_for_upload");
    },

    acknowledgeSealSync: async (uploadedSealHash: string, password?: string) => {
        return await invoke<void>("acknowledge_seal_sync", { uploadedSealHash, password });
    },

    reconcileRollback: async (password?: string) => {
        return await invoke<RollbackReport>("reconcile_rollback", { password });
    },

    probeLockServer: async (writeUrl: string, expectedAdminPub?: string | null, password?: string | null) => {
        return await invoke<LockServerProbeResult>("probe_lock_server_cmd", {
            writeUrl,
            expectedAdminPub: expectedAdminPub ?? null,
            password: password ?? null,
        });
    },

    setOwnNominatedLockServer: async (writeUrl: string | null, asn?: string | null, password?: string | null) => {
        return await invoke<NominatedLockServerMeta | null>("set_own_nominated_lock_server", {
            writeUrl,
            asn: asn ?? null,
            password: password ?? null,
        });
    },

    assembleAnchorTemplate: async (args: AssembleAnchorTemplateArgs) => {
        return await invoke<StoredAnchorTemplateMeta>("assemble_anchor_template", {
            label: args.label,
            retainUntilQuarterStr: args.retainUntilQuarterStr,
            servers: args.servers,
            guarantorMigPubsHex: args.guarantorMigPubsHex ?? [],
            threshold: args.threshold ?? null,
            password: args.password ?? null,
        });
    },

    prewarmL2Checkout: async () => {
        return await invoke<void>("prewarm_l2_checkout");
    },

    checkL2LockStatus: async () => {
        return await invoke<L2LockStatusDto>("check_l2_lock_status");
    },

    checkVoucherL2Status: async (localInstanceId: string, password?: string | null) => {
        return await invoke<VoucherL2StatusDto>("check_voucher_l2_status", {
            localInstanceId,
            password: password ?? null,
        });
    },
};
