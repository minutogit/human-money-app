import { invoke } from "@tauri-apps/api/core";
import { ProfileInfo } from "../types";

export const authService = {
    listProfiles: async () => {
        return await invoke<ProfileInfo[]>("list_profiles");
    },

    login: async (args: any) => {
        return await invoke<void>("login", args);
    },

    getLocalInstanceId: async () => {
        return await invoke<string>("get_local_instance_id");
    },

    handoverToThisDevice: async (args: any) => {
        return await invoke<void>("handover_to_this_device", args);
    },

    verifyProfilePassword: async (args: any) => {
        return await invoke<string>("verify_profile_password", args);
    },

    deleteProfile: async (args: any) => {
        return await invoke<void>("delete_profile", args);
    },

    refreshSessionActivity: async () => {
        return await invoke<void>("refresh_session_activity");
    },

    isSessionActive: async () => {
        return await invoke<boolean>("is_session_active");
    },

    unlockSession: async (password: string, timeout: number) => {
        return await invoke<void>("unlock_session", { password, timeout });
    }
};

