import { invoke } from "@tauri-apps/api/core";
import { ProfileInfo } from "../types";

export type SessionAccessMode = 'full' | 'dailyPay';
export type PinScope = 'dailyPay';

export interface SessionStatusDto {
    isUnlocked: boolean;
    hasCachedSession: boolean;
    accessMode: SessionAccessMode | 'locked';
    baseAccessMode: SessionAccessMode | 'locked';
}

export interface ProfilePinStatusDto {
    enabled: boolean;
    scope: PinScope | null;
    failedAttempts: number;
    maxAttempts: number;
    remainingAttempts: number;
}

export interface LoginArgs {
    folderName: string;
    password?: string;
    cleanupOnLogin?: boolean;
    localInstanceId?: string;
}

export interface PinLoginArgs {
    folderName: string;
    pin: string;
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

    getSessionStatus: async (): Promise<SessionStatusDto> => {
        return await invoke<SessionStatusDto>("get_session_status");
    },

    unlockSession: async (password: string, timeout: number): Promise<SessionStatusDto> => {
        return await invoke<SessionStatusDto>("unlock_session", { password, durationSeconds: timeout });
    },

    unlockSessionWithPin: async (
        pin: string,
        timeout: number,
        folderName?: string
    ): Promise<SessionStatusDto> => {
        return await invoke<SessionStatusDto>("unlock_session_with_pin", {
            pin,
            durationSeconds: timeout,
            folderName: folderName ?? null,
        });
    },

    lockSession: async () => {
        return await invoke<void>("lock_session");
    },

    getProfilePinStatus: async (folderName?: string): Promise<ProfilePinStatusDto> => {
        return await invoke<ProfilePinStatusDto>("get_profile_pin_status", {
            folderName: folderName || undefined,
        });
    },

    enableProfilePin: async (
        masterPassword: string,
        pin: string,
        scope: PinScope,
        folderName?: string
    ): Promise<ProfilePinStatusDto> => {
        return await invoke<ProfilePinStatusDto>("enable_profile_pin", {
            masterPassword,
            pin,
            scope,
            folderName: folderName ?? null,
        });
    },

    disableProfilePin: async (masterPassword: string, folderName?: string): Promise<void> => {
        return await invoke<void>("disable_profile_pin", {
            masterPassword,
            folderName: folderName ?? null,
        });
    },

    loginWithPin: async (args: PinLoginArgs): Promise<SessionAccessMode> => {
        return await invoke<SessionAccessMode>("login_with_pin", {
            folderName: args.folderName,
            pin: args.pin,
            cleanupOnLogin: args.cleanupOnLogin ?? true,
            localInstanceId: args.localInstanceId,
        });
    },

    elevateSessionWithPassword: async (masterPassword: string): Promise<SessionStatusDto> => {
        return await invoke<SessionStatusDto>("elevate_session_with_password", {
            masterPassword,
        });
    },
};

