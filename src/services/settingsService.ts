import { invoke } from "@tauri-apps/api/core";
import {
    AnchorDiscoveryDto,
    AppConfigDto,
    LockProtectionOverviewDto,
    ReachPresetDto,
    ReachPresetId,
    SecurityPolicy,
    SecurityPolicyPatch,
    UiPreferences,
    UiPreferencesPatch,
    VoucherL2StatusDto,
} from "../types";

export type { VoucherL2StatusDto, LockProtectionOverviewDto };

export const settingsService = {
    getConfig: async (): Promise<AppConfigDto> => {
        return await invoke<AppConfigDto>("get_app_config");
    },

    patchUiPreferences: async (
        patch: UiPreferencesPatch,
        password?: string,
    ): Promise<UiPreferences> => {
        return await invoke<UiPreferences>("patch_ui_preferences", { patch, password });
    },

    patchSecurityPolicy: async (
        patch: SecurityPolicyPatch,
        password?: string,
    ): Promise<SecurityPolicy> => {
        return await invoke<SecurityPolicy>("patch_security_policy", { patch, password });
    },

    getReachPreset: async (password?: string): Promise<ReachPresetDto> => {
        return await invoke<ReachPresetDto>("get_reach_preset", { password });
    },

    applyReachPreset: async (preset: ReachPresetId, password?: string): Promise<ReachPresetDto> => {
        return await invoke<ReachPresetDto>("apply_reach_preset", { preset, password });
    },

    getLatestLogs: async (): Promise<string> => {
        return await invoke<string>("get_latest_logs");
    },

    getLockProtectionOverview: async (): Promise<LockProtectionOverviewDto> => {
        return await invoke<LockProtectionOverviewDto>("get_lock_protection_overview");
    },

    refreshLockServers: async (password?: string | null): Promise<LockProtectionOverviewDto> => {
        return await invoke<LockProtectionOverviewDto>("refresh_lock_servers", {
            password: password ?? null,
        });
    },

    setLockServerTrust: async (
        adminPub: string,
        trusted: boolean,
        password?: string | null,
    ): Promise<LockProtectionOverviewDto> => {
        return await invoke<LockProtectionOverviewDto>("set_lock_server_trust", {
            adminPub,
            trusted,
            password: password ?? null,
        });
    },

    prewarmL2Checkout: async () => {
        return await invoke<void>("prewarm_l2_checkout");
    },

    checkVoucherL2Status: async (localInstanceId: string, password?: string | null) => {
        return await invoke<VoucherL2StatusDto>("check_voucher_l2_status", {
            localInstanceId,
            password: password ?? null,
        });
    },

    discoverAnchorManifest: async (args: {
        anchorIdHex: string;
        creatorDid?: string | null;
        guarantorDids?: string[];
        password?: string | null;
    }) => {
        return await invoke<AnchorDiscoveryDto | null>("discover_anchor_manifest_cmd", {
            anchorIdHex: args.anchorIdHex,
            creatorDid: args.creatorDid ?? null,
            guarantorDids: args.guarantorDids ?? [],
            password: args.password ?? null,
        });
    },
};
