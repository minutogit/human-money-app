import { invoke } from "@tauri-apps/api/core";
import { AppSettings, AnchorSyncStatus, RollbackReport } from "../types";

export type { AnchorSyncStatus, RollbackReport };

export interface SealForUpload {
    sealHash: string;
    sealBytes: number[];
}

export const settingsService = {
    getSettings: async () => {
        return await invoke<AppSettings>("get_app_settings");
    },

    refreshGatewayCache: async (password?: string) => {
        return await invoke<string[]>("refresh_l2_gateway_cache", { password });
    },

    saveSettings: async (settings: AppSettings, password?: string) => {
        return await invoke<void>("save_app_settings", { settings, password });
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

    checkAnchorSync: async () => {
        return await invoke<AnchorSyncStatus>("check_anchor_sync");
    },

    reconcileRollback: async (password?: string) => {
        return await invoke<RollbackReport>("reconcile_rollback", { password });
    },

    flushPendingAnchorSync: async (password?: string) => {
        return await invoke<boolean>("flush_pending_anchor_sync", { password });
    },

    benchmarkL2Servers: async (urls?: string[]) => {
        return await invoke<Array<{ url: string; latencyMs?: number; reachable: boolean }>>("benchmark_l2_servers", { urls });
    }
};
