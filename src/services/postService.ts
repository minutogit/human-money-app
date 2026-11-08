// src/services/postService.ts

import { invoke } from "@tauri-apps/api/core";
import {
    FrontendPostDepositResult,
    FrontendPostInboxSummary,
    FrontendPostSyncResult,
    FrontendMarketInquiryItem,
    FrontendDropDescriptor,
} from "../types";
import { triggerAutoBackup } from "../utils/autoBackup";

export const POST_INBOX_UPDATED_EVENT = 'humoco:post-inbox-updated';

export function notifyPostInboxUpdated(): void {
    if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(POST_INBOX_UPDATED_EVENT));
    }
}

export const postService = {
    depositTransferToPost: async (
        bundleId: string,
        password?: string
    ): Promise<FrontendPostDepositResult> => {
        const res = await invoke<FrontendPostDepositResult>("deposit_transfer_to_post", {
            bundleId,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
        return res;
    },

    sendSigningRequestViaPost: async (
        recipientDid: string,
        containerBytes: number[] | Uint8Array,
        password?: string
    ): Promise<FrontendPostDepositResult> => {
        const bytes = Array.isArray(containerBytes) ? containerBytes : Array.from(containerBytes);
        const res = await invoke<FrontendPostDepositResult>("send_signing_request_via_post", {
            recipientDid,
            containerBytes: bytes,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
        return res;
    },

    replyDetachedSignatureViaPost: async (
        requestMsgId: string | null | undefined,
        recipientDid: string,
        containerBytes: number[] | Uint8Array,
        password?: string
    ): Promise<FrontendPostDepositResult> => {
        const bytes = Array.isArray(containerBytes) ? containerBytes : Array.from(containerBytes);
        const res = await invoke<FrontendPostDepositResult>("reply_detached_signature_via_post", {
            requestMsgId: requestMsgId || undefined,
            recipientDid,
            containerBytes: bytes,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
        return res;
    },

    sendWotHandshakeViaPost: async (
        recipientDid: string,
        proposedLevel: number,
        replyRequested: boolean = true,
        password?: string
    ): Promise<FrontendPostDepositResult> => {
        const res = await invoke<FrontendPostDepositResult>("send_wot_handshake_via_post", {
            recipientDid,
            proposedLevel,
            replyRequested,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
        return res;
    },

    acceptWotHandshake: async (
        msgId: string,
        trustLevel: number,
        sendReturnHandshake: boolean = true,
        password?: string
    ): Promise<FrontendPostDepositResult> => {
        const res = await invoke<FrontendPostDepositResult>("accept_wot_handshake", {
            msgId,
            trustLevel,
            sendReturnHandshake,
            password: password || undefined,
        });
        triggerAutoBackup();
        notifyPostInboxUpdated();
        return res;
    },

    sendMarketInquiryViaPost: async (
        recipientDid: string,
        listingId: string,
        listingTitle: string,
        message: string,
        recipientDrops: FrontendDropDescriptor[],
        password?: string
    ): Promise<FrontendMarketInquiryItem> => {
        const res = await invoke<FrontendMarketInquiryItem>("send_market_inquiry_via_post", {
            recipientDid,
            listingId,
            listingTitle,
            message,
            recipientDrops,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
        return res;
    },

    replyMarketInquiryViaPost: async (
        inquiryId: string,
        decision: string,
        note?: string | null,
        paymentUri?: string | null,
        amountHint?: string | null,
        standardUuid?: string | null,
        password?: string
    ): Promise<FrontendMarketInquiryItem> => {
        const res = await invoke<FrontendMarketInquiryItem>("reply_market_inquiry_via_post", {
            inquiryId,
            decision,
            note: note || undefined,
            paymentUri: paymentUri || undefined,
            amountHint: amountHint || undefined,
            standardUuid: standardUuid || undefined,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
        return res;
    },

    dismissPendingPostItem: async (
        itemKind: 'sign_request' | 'wot_handshake' | 'seller_inquiry' | 'buyer_inquiry',
        id: string,
        password?: string
    ): Promise<void> => {
        await invoke<void>("dismiss_pending_post_item", {
            itemKind,
            id,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
    },

    getPostInboxSummary: async (
        password?: string
    ): Promise<FrontendPostInboxSummary> => {
        return await invoke<FrontendPostInboxSummary>("get_post_inbox_summary", password ? { password } : {});
    },

    syncPostDrops: async (
        isWifi?: boolean,
        password?: string
    ): Promise<FrontendPostSyncResult> => {
        const res = await invoke<FrontendPostSyncResult>("sync_post_drops", {
            isWifi: typeof isWifi === 'boolean' ? isWifi : undefined,
            password: password || undefined,
        });
        if (res.importedPayments > 0 || res.importedSignatures > 0) {
            triggerAutoBackup();
        }
        notifyPostInboxUpdated();
        return res;
    },
};
