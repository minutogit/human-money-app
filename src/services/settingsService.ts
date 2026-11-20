import { invoke } from "@tauri-apps/api/core";
import {
    AnchorDiscoveryDto,
    AppConfigDto,
    L2LifecycleStatusDto,
    LockProtectionOverviewDto,
    MnemonicLanguage,
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

    getSealSyncStatus: async () => {
        return await invoke<string>("get_seal_sync_status");
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

    runL2LifecycleTick: async (force: boolean, password?: string | null) => {
        return await invoke<L2LifecycleStatusDto>("run_l2_lifecycle_tick", {
            force,
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

    createKeySuccessorAttestation: async (args: {
        newMnemonic: string;
        language?: MnemonicLanguage | null;
        passphrase?: string | null;
        reason: string;
        newCreatorMigPubHex?: string | null;
        password?: string | null;
    }) => {
        return await invoke<string>("create_key_successor_attestation_cmd", {
            newMnemonic: args.newMnemonic,
            language: args.language ?? null,
            passphrase: args.passphrase ?? null,
            reason: args.reason,
            newCreatorMigPubHex: args.newCreatorMigPubHex ?? null,
            password: args.password ?? null,
        });
    },

    importKeySuccessorAttestation: async (attestationJson: string, password?: string | null) => {
        return await invoke<string>("import_key_successor_attestation_cmd", {
            attestationJson,
            password: password ?? null,
        });
    },
};
