// src/services/marketService.ts

import { invoke } from "@tauri-apps/api/core";
import {
    FrontendMarketSnapshot,
    FrontendMyMarketProfile,
    FrontendSaveMyMarketProfileRequest,
    FrontendAdInput,
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
    FrontendAppNoticesResponse,
} from "../types";
import { APP_EVENTS, emitAppEvent } from "../utils/appEvents";
import { detectIsWifiConnection } from "../utils/networkStatus";
import { notifyAppNoticesUpdated, notifyMyProfileUpdated } from "../utils/syncEvents";

export const marketService = {
    getMyMarketProfile: async (password?: string): Promise<FrontendMyMarketProfile> => {
        return await invoke<FrontendMyMarketProfile>("get_my_market_profile", password ? { password } : {});
    },

    saveMyMarketProfile: async (
        request: FrontendSaveMyMarketProfileRequest,
        password?: string
    ): Promise<FrontendMyMarketProfile> => {
        const res = await invoke<FrontendMyMarketProfile>("save_my_market_profile", {
            request,
            password: password || undefined,
        });
        notifyMyProfileUpdated(res);
        emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
        return res;
    },

    setCardPublished: async (
        published: boolean,
        deleteRemote: boolean = false,
        password?: string
    ): Promise<FrontendMyMarketProfile> => {
        const res = await invoke<FrontendMyMarketProfile>("set_card_published", {
            published,
            deleteRemote,
            password: password || undefined,
        });
        notifyMyProfileUpdated(res);
        emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
        return res;
    },

    upsertAd: async (ad: FrontendAdInput, password?: string): Promise<FrontendMyMarketProfile> => {
        const res = await invoke<FrontendMyMarketProfile>("upsert_ad", {
            ad,
            password: password || undefined,
        });
        emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
        return res;
    },

    renewAd: async (id: string, durationDays: number, password?: string): Promise<FrontendMyMarketProfile> => {
        const res = await invoke<FrontendMyMarketProfile>("renew_ad", {
            id,
            durationDays,
            password: password || undefined,
        });
        emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
        return res;
    },

    deleteAd: async (id: string, password?: string): Promise<FrontendMyMarketProfile> => {
        const res = await invoke<FrontendMyMarketProfile>("delete_ad", {
            id,
            password: password || undefined,
        });
        emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
        return res;
    },

    getMarketView: async (password?: string): Promise<FrontendMarketSnapshot> => {
        return await invoke<FrontendMarketSnapshot>("get_market_view", password ? { password } : {});
    },

    exportForwardCard: async (did: string, password?: string): Promise<FrontendForwardCardExport> => {
        return await invoke<FrontendForwardCardExport>("export_forward_card", { did, password: password || undefined });
    },

    importContactCard: async (payload: string, password?: string): Promise<FrontendImportCardResult> => {
        const res = await invoke<FrontendImportCardResult>("import_contact_card", { payload, password: password || undefined });
        if (res?.status === 'accepted') {
            emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
            emitAppEvent(APP_EVENTS.CONTACTS_UPDATED);
        }
        return res;
    },

    deleteForeignContactCard: async (did: string, password?: string): Promise<void> => {
        await invoke<void>("delete_foreign_contact_card", { did, password: password || undefined });
        emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
    },

    getAdImage: async (ownerDid: string, imageToken: string, password?: string): Promise<string | null> => {
        return await invoke<string | null>("get_ad_image", { ownerDid, imageToken, password: password || undefined });
    },

    // --- Sync Service Management ---

    prepareSyncSetup: async (
        args?: {
            policy?: FrontendMemberPolicyDto;
            maxMembers?: number;
            maxTotalStorageMb?: number;
            maxUserStorageMb?: number;
        },
        password?: string
    ): Promise<FrontendSyncSetupDraft> => {
        return await invoke<FrontendSyncSetupDraft>("prepare_sync_setup", {
            policy: args?.policy,
            maxMembers: args?.maxMembers,
            maxTotalStorageMb: args?.maxTotalStorageMb,
            maxUserStorageMb: args?.maxUserStorageMb,
            password: password || undefined,
        });
    },

    getRustSyncConfig: async (password?: string): Promise<string> => {
        return await invoke<string>("get_rust_sync_config", {
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

    getSyncOverviewStatus: async (password?: string): Promise<FrontendSyncOverviewStatus> => {
        return await invoke<FrontendSyncOverviewStatus>("get_sync_overview_status", password ? { password } : {});
    },

    deleteSyncAdmin: async (password?: string): Promise<void> => {
        return await invoke<void>("delete_sync_admin", password ? { password } : {});
    },

    revokeOwnSyncServer: async (
        effectiveAt: number | null,
        reason?: string,
        replacementWriteUrl?: string,
        password?: string
    ): Promise<FrontendSyncAdminStatus | null> => {
        return await invoke<FrontendSyncAdminStatus | null>("revoke_own_sync_server", {
            effectiveAt,
            reason: reason || undefined,
            replacementWriteUrl: replacementWriteUrl || undefined,
            password: password || undefined,
        });
    },

    discoverAvailableSyncs: async (password?: string): Promise<FrontendDiscoveredSync[]> => {
        return await invoke<FrontendDiscoveredSync[]>("discover_available_syncs", password ? { password } : {});
    },

    probeContactSyncs: async (force: boolean = false, password?: string): Promise<FrontendSyncDescriptor[]> => {
        return await invoke<FrontendSyncDescriptor[]>("probe_contact_syncs", {
            force,
            password: password || undefined,
        });
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
        args?: { isWifi?: boolean; forceUpload?: boolean; background?: boolean },
        password?: string
    ): Promise<FrontendMarketSyncResult> => {
        const payload: Record<string, unknown> = {
            isWifi: args?.isWifi !== undefined ? args.isWifi : detectIsWifiConnection(),
        };
        if (args?.forceUpload !== undefined) payload.forceUpload = args.forceUpload;
        if (args?.background) payload.respectCooldown = true;
        if (password) payload.password = password;
        const res = await invoke<FrontendMarketSyncResult>("sync_market_channels", payload);
        notifyAppNoticesUpdated();
        if (res) {
            if (res.importedCards > 0 || (res.newAds ?? 0) > 0) {
                emitAppEvent(APP_EVENTS.MARKET_CARD_UPDATED);
            }
            if (res.importedCards > 0) {
                emitAppEvent(APP_EVENTS.CONTACTS_UPDATED);
            }
            if (res.importedWotLists > 0) {
                emitAppEvent(APP_EVENTS.WOT_UPDATED);
            }
            if (res.uploadedTo > 0 || res.polledSyncs > 0 || res.importedCards > 0 || res.importedWotLists > 0) {
                emitAppEvent(APP_EVENTS.SYNC_COMPLETED, { source: 'market' });
            }
        }
        return res;
    },

    getAppNotices: async (password?: string): Promise<FrontendAppNoticesResponse> => {
        return await invoke<FrontendAppNoticesResponse>("get_app_notices", password ? { password } : {});
    },

    markAppNoticesRead: async (
        noticeId?: string | null,
        password?: string
    ): Promise<FrontendAppNoticesResponse> => {
        return await invoke<FrontendAppNoticesResponse>("mark_app_notices_read", {
            noticeId: noticeId || undefined,
            password: password || undefined,
        });
    },

    clearAppNotices: async (password?: string): Promise<FrontendAppNoticesResponse> => {
        return await invoke<FrontendAppNoticesResponse>("clear_app_notices", password ? { password } : {});
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
