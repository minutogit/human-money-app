import { invoke } from "@tauri-apps/api/core";
import { ProfileInfo } from "../types";

export interface LoginArgs {
    folderName: string;
    password?: string;
    cleanupOnLogin?: boolean;
    localInstanceId?: string;
}

export interface HandoverArgs {
    folderName: string;
    password?: string;
    targetInstanceId: string;
}

export const authService = {
    listProfiles: async () => {
        return await invoke<ProfileInfo[]>("list_profiles");
    },

    login: async (args: LoginArgs) => {
        return await invoke<void>("login", {
            folderName: args.folderName,
            password: args.password,
            cleanupOnLogin: args.cleanupOnLogin,
            localInstanceId: args.localInstanceId
        });
    },

    getLocalInstanceId: async () => {
        return await invoke<string>("get_local_instance_id");
    },

    handoverToThisDevice: async (args: HandoverArgs) => {
        return await invoke<void>("handover_to_this_device", {
            folderName: args.folderName,
            password: args.password,
            localInstanceId: args.targetInstanceId
        });
    },

    verifyProfilePassword: async (args: { folderName: string; password?: string }) => {
        return await invoke<string>("verify_profile_password", {
            folderName: args.folderName,
            password: args.password
        });
    },

    deleteProfile: async (args: { folderName: string; password?: string }) => {
        return await invoke<void>("delete_profile", {
            folderName: args.folderName,
            password: args.password
        });
    },

    refreshSessionActivity: async () => {
        return await invoke<void>("refresh_session_activity");
    },

    isSessionActive: async () => {
        return await invoke<boolean>("is_session_active");
    },

    unlockSession: async (password: string, timeout: number) => {
        return await invoke<void>("unlock_session", { password, durationSeconds: timeout });
    }
};

