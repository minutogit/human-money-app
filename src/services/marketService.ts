// src/services/marketService.ts

import { invoke } from "@tauri-apps/api/core";
import {
    FrontendMarketPerson,
    FrontendOwnContactCard,
    FrontendSaveOwnCardRequest,
    FrontendForwardCardExport,
    FrontendImportCardResult,
    FrontendMemberPolicyDto,
    FrontendDropSetupDraft,
    FrontendDropAdminStatus,
    FrontendDropAdminUpdateRequest,
    FrontendDropConnectionTestResult,
    FrontendDiscoveredDrop,
    FrontendDropDescriptor,
    FrontendDropSyncResult,
    FrontendTrustChangeImpact,
    FrontendPublicDropInfo,
    FrontendDropOverviewStatus,
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

    // --- Phase 3: Drop Management (Ablagen) ---

    prepareDropSetup: async (
        args?: { policy?: FrontendMemberPolicyDto; maxMembers?: number; maxTotalStorageMb?: number },
        password?: string
    ): Promise<FrontendDropSetupDraft> => {
        return await invoke<FrontendDropSetupDraft>("prepare_drop_setup", {
            policy: args?.policy,
            maxMembers: args?.maxMembers,
            maxTotalStorageMb: args?.maxTotalStorageMb,
            password: password || undefined,
        });
    },

    saveDropSetupZip: async (filePath: string, password?: string): Promise<void> => {
        return await invoke<void>("save_drop_setup_zip", {
            filePath,
            password: password || undefined,
        });
    },

    finalizeDropSetup: async (baseUrl: string, password?: string): Promise<FrontendDropAdminStatus> => {
        return await invoke<FrontendDropAdminStatus>("finalize_drop_setup", {
            baseUrl,
            password: password || undefined,
        });
    },

    testDropConnection: async (baseUrl: string, password?: string): Promise<FrontendDropConnectionTestResult> => {
        return await invoke<FrontendDropConnectionTestResult>("test_drop_connection", {
            baseUrl,
            password: password || undefined,
        });
    },

    getDropAdminStatus: async (password?: string): Promise<FrontendDropAdminStatus | null> => {
        return await invoke<FrontendDropAdminStatus | null>("get_drop_admin_status", password ? { password } : {});
    },

    updateDropAdminConfig: async (
        update: FrontendDropAdminUpdateRequest,
        pushNow: boolean = false,
        password?: string
    ): Promise<FrontendDropAdminStatus> => {
        return await invoke<FrontendDropAdminStatus>("update_drop_admin_config", {
            update,
            pushNow,
            password: password || undefined,
        });
    },

    blockDropSlot: async (pubkeyHex64: string, password?: string): Promise<FrontendDropAdminStatus> => {
        return await invoke<FrontendDropAdminStatus>("block_drop_slot", {
            pubkeyHex64,
            password: password || undefined,
        });
    },

    probePublicDrop: async (url: string, password?: string): Promise<FrontendPublicDropInfo> => {
        return await invoke<FrontendPublicDropInfo>("probe_public_drop", {
            url,
            password: password || undefined,
        });
    },

    savePublicDrop: async (
        params: {
            writeUrl: string;
            readUrl: string;
            dropPub: string;
            registerUrl?: string | null;
            enabled: boolean;
        },
        password?: string
    ): Promise<FrontendPublicDropInfo> => {
        return await invoke<FrontendPublicDropInfo>("save_public_drop", {
            writeUrl: params.writeUrl,
            readUrl: params.readUrl,
            dropPub: params.dropPub,
            registerUrl: params.registerUrl || undefined,
            enabled: params.enabled,
            password: password || undefined,
        });
    },

    removePublicDrop: async (
        writeUrl: string,
        deleteRemoteSlots: boolean = false,
        password?: string
    ): Promise<void> => {
        return await invoke<void>("remove_public_drop", {
            writeUrl,
            deleteRemoteSlots,
            password: password || undefined,
        });
    },

    rotateDropAdminKey: async (password?: string): Promise<FrontendDropAdminStatus> => {
        return await invoke<FrontendDropAdminStatus>("rotate_drop_admin_key", password ? { password } : {});
    },

    getDropOverviewStatus: async (password?: string): Promise<FrontendDropOverviewStatus> => {
        return await invoke<FrontendDropOverviewStatus>("get_drop_overview_status", password ? { password } : {});
    },

    deleteDropAdmin: async (password?: string): Promise<void> => {
        return await invoke<void>("delete_drop_admin", password ? { password } : {});
    },

    discoverAvailableDrops: async (password?: string): Promise<FrontendDiscoveredDrop[]> => {
        return await invoke<FrontendDiscoveredDrop[]>("discover_available_drops", password ? { password } : {});
    },

    setSelectedUploadDrops: async (
        drops: FrontendDropDescriptor[],
        password?: string
    ): Promise<FrontendDropDescriptor[]> => {
        return await invoke<FrontendDropDescriptor[]>("set_selected_upload_drops", {
            drops,
            password: password || undefined,
        });
    },

    syncMarketDrops: async (
        args?: { isWifi?: boolean; forceUpload?: boolean },
        password?: string
    ): Promise<FrontendDropSyncResult> => {
        return await invoke<FrontendDropSyncResult>("sync_market_drops", {
            isWifi: args?.isWifi,
            forceUpload: args?.forceUpload,
            password: password || undefined,
        });
    },

    deleteOwnDropSlots: async (password?: string): Promise<number> => {
        return await invoke<number>("delete_own_drop_slots", password ? { password } : {});
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
};

