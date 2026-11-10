// src/services/marketService.ts

import { invoke } from "@tauri-apps/api/core";
import {
    FrontendMarketPerson,
    FrontendOwnContactCard,
    FrontendSaveOwnCardRequest,
    FrontendForwardCardExport,
    FrontendImportCardResult,
    FrontendMemberPolicyDto,
    FrontendSyncSetupDraft,
    FrontendSyncAdminStatus,
    FrontendSyncAdminUpdateRequest,
    FrontendSyncConnectionTestResult,
    FrontendDiscoveredSync,
    FrontendSyncDescriptor,
    FrontendMarketSyncResult,
    FrontendTrustChangeImpact,
    FrontendPublicSyncInfo,
    FrontendSyncOverviewStatus,
} from "../types";

export const marketService = {
    getMarketView: async (password?: string): Promise<FrontendMarketPerson[]> => {
        return await invoke<FrontendMarketPerson[]>("get_market_view", password ? { password } : {});
    },

    getOwnContactCard: async (password?: string): Promise<FrontendOwnContactCard | null> => {
        return await invoke<FrontendOwnContactCard | null>("get_own_contact_card", password ? { password } : {});
    },

    saveOwnContactCard: async (request: FrontendSaveOwnCardRequest, password?: string): Promise<FrontendOwnContactCard> => {
        return await invoke<FrontendOwnContactCard>("save_own_contact_card", { request, password: password || undefined });
    },

    deleteOwnContactCard: async (password?: string): Promise<void> => {
        return await invoke<void>("delete_own_contact_card", password ? { password } : {});
    },

    exportForwardCard: async (did: string, password?: string): Promise<FrontendForwardCardExport> => {
        return await invoke<FrontendForwardCardExport>("export_forward_card", { did, password: password || undefined });
    },

    importContactCard: async (payload: string, password?: string): Promise<FrontendImportCardResult> => {
        return await invoke<FrontendImportCardResult>("import_contact_card", { payload, password: password || undefined });
    },

    // --- Phase 3 & 4: Sync Service Management ---

    prepareSyncSetup: async (
        args?: { policy?: FrontendMemberPolicyDto; maxMembers?: number; maxTotalStorageMb?: number },
        password?: string
    ): Promise<FrontendSyncSetupDraft> => {
        return await invoke<FrontendSyncSetupDraft>("prepare_sync_setup", {
            policy: args?.policy,
            maxMembers: args?.maxMembers,
            maxTotalStorageMb: args?.maxTotalStorageMb,
            password: password || undefined,
        });
    },

    saveSyncSetupZip: async (filePath: string, password?: string): Promise<void> => {
        return await invoke<void>("save_sync_setup_zip", {
            filePath,
            password: password || undefined,
        });
    },

    finalizeSyncSetup: async (baseUrl: string, password?: string): Promise<FrontendSyncAdminStatus> => {
        return await invoke<FrontendSyncAdminStatus>("finalize_sync_setup", {
            baseUrl,
            password: password || undefined,
        });
    },

    testSyncConnection: async (baseUrl: string, password?: string): Promise<FrontendSyncConnectionTestResult> => {
        return await invoke<FrontendSyncConnectionTestResult>("test_sync_connection", {
            baseUrl,
            password: password || undefined,
        });
    },

    getSyncAdminStatus: async (password?: string): Promise<FrontendSyncAdminStatus | null> => {
        return await invoke<FrontendSyncAdminStatus | null>("get_sync_admin_status", password ? { password } : {});
    },

    updateSyncAdminConfig: async (
        update: FrontendSyncAdminUpdateRequest,
        pushNow: boolean = false,
        password?: string
    ): Promise<FrontendSyncAdminStatus> => {
        return await invoke<FrontendSyncAdminStatus>("update_sync_admin_config", {
            update,
            pushNow,
            password: password || undefined,
        });
    },

    blockSyncSlot: async (pubkeyHex64: string, password?: string): Promise<FrontendSyncAdminStatus> => {
        return await invoke<FrontendSyncAdminStatus>("block_sync_slot", {
            pubkeyHex64,
            password: password || undefined,
        });
    },

    probePublicSync: async (url: string, password?: string): Promise<FrontendPublicSyncInfo> => {
        return await invoke<FrontendPublicSyncInfo>("probe_public_sync", {
            url,
            password: password || undefined,
        });
    },

    savePublicSync: async (
        params: {
            writeUrl: string;
            readUrl: string;
            dropPub: string;
            registerUrl?: string | null;
            enabled: boolean;
        },
        password?: string
    ): Promise<FrontendPublicSyncInfo> => {
        return await invoke<FrontendPublicSyncInfo>("save_public_sync", {
            writeUrl: params.writeUrl,
            readUrl: params.readUrl,
            dropPub: params.dropPub,
            registerUrl: params.registerUrl || undefined,
            enabled: params.enabled,
            password: password || undefined,
        });
    },

    removePublicSync: async (
        writeUrl: string,
        deleteRemoteSlots: boolean = false,
        password?: string
    ): Promise<void> => {
        return await invoke<void>("remove_public_sync", {
            writeUrl,
            deleteRemoteSlots,
            password: password || undefined,
        });
    },

    rotateSyncAdminKey: async (password?: string): Promise<FrontendSyncAdminStatus> => {
        return await invoke<FrontendSyncAdminStatus>("rotate_sync_admin_key", password ? { password } : {});
    },

    getSyncOverviewStatus: async (password?: string): Promise<FrontendSyncOverviewStatus> => {
        return await invoke<FrontendSyncOverviewStatus>("get_sync_overview_status", password ? { password } : {});
    },

    deleteSyncAdmin: async (password?: string): Promise<void> => {
        return await invoke<void>("delete_sync_admin", password ? { password } : {});
    },

    discoverAvailableSyncs: async (password?: string): Promise<FrontendDiscoveredSync[]> => {
        return await invoke<FrontendDiscoveredSync[]>("discover_available_syncs", password ? { password } : {});
    },

    setSelectedUploadSyncs: async (
        syncs: FrontendSyncDescriptor[],
        password?: string
    ): Promise<FrontendSyncDescriptor[]> => {
        return await invoke<FrontendSyncDescriptor[]>("set_selected_upload_syncs", {
            syncs,
            password: password || undefined,
        });
    },

    syncMarketChannels: async (
        args?: { isWifi?: boolean; forceUpload?: boolean },
        password?: string
    ): Promise<FrontendMarketSyncResult> => {
        return await invoke<FrontendMarketSyncResult>("sync_market_channels", {
            isWifi: args?.isWifi,
            forceUpload: args?.forceUpload,
            password: password || undefined,
        });
    },

    deleteOwnSyncSlots: async (password?: string): Promise<number> => {
        return await invoke<number>("delete_own_sync_slots", password ? { password } : {});
    },

    previewContactTrustChange: async (
        did: string,
        newLevel: number,
        password?: string
    ): Promise<FrontendTrustChangeImpact> => {
        return await invoke<FrontendTrustChangeImpact>("preview_contact_trust_change", {
            did,
            newLevel,
            password: password || undefined,
        });
    },

    syncChannelMembers: async (password?: string): Promise<number> => {
        return await invoke<number>("sync_channel_members", password ? { password } : {});
    },
};

let channelMembersDebounceTimer: number | null = null;

/**
 * Triggers a 1.5s (1500ms) debounced call to sync_channel_members.
 * Rapid consecutive edits to contacts/trust levels coalesce into a single upload.
 */
export function triggerDebouncedChannelMembersSync(delayMs: number = 1500): void {
    if (typeof window === 'undefined') return;
    if (channelMembersDebounceTimer !== null) {
        window.clearTimeout(channelMembersDebounceTimer);
    }
    channelMembersDebounceTimer = window.setTimeout(async () => {
        channelMembersDebounceTimer = null;
        try {
            await marketService.syncChannelMembers();
        } catch {
            // Background upload errors handled silently
        }
    }, delayMs);
}
