import { invoke } from "@tauri-apps/api/core";
import { AppSettings } from "../types";

export interface SealForUpload {
    sealHash: string;
    sealBytes: number[];
}

export const settingsService = {
    getSettings: async () => {
        return await invoke<AppSettings>("get_app_settings");
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
    }
};
