// src/services/postService.ts

import { invoke } from "@tauri-apps/api/core";
import {
    FrontendPostDepositResult,
    FrontendPostInboxSummary,
    FrontendPostSyncResult,
    FrontendMarketInquiryItem,
    FrontendSyncDescriptor,
    InquiryTarget,
    ReceiveSuccessPayload,
} from "../types";
import { afterWalletMutation, triggerAutoBackup } from "../sync/syncBridge";
import { APP_EVENTS, emitAppEvent } from "../utils/appEvents";
import { detectIsWifiConnection } from "../utils/networkStatus";
import { notifyAppNoticesUpdated } from "../utils/syncEvents";

export function notifyPostInboxUpdated(): void {
    emitAppEvent(APP_EVENTS.POST_INBOX_UPDATED);
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
        afterWalletMutation(password, {
            syncMembers: true,
            scheduleMarket: trustLevel >= 1,
            contactsUpdated: true,
            wotUpdated: true,
        });
        notifyPostInboxUpdated();
        return res;
    },

    sendMarketInquiryViaPost: async (
        recipientDid: string,
        target: InquiryTarget,
        message: string,
        recipientSyncs: FrontendSyncDescriptor[],
        password?: string
    ): Promise<FrontendMarketInquiryItem> => {
        const res = await invoke<FrontendMarketInquiryItem>("send_market_inquiry_via_post", {
            recipientDid,
            target,
            message,
            recipientSyncs,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
        return res;
    },

    sendMarketInquiryMessageViaPost: async (
        inquiryId: string,
        options: {
            text?: string | null;
            decision?: 'confirm' | 'decline' | null;
            paymentUri?: string | null;
            amountHint?: string | null;
            standardUuid?: string | null;
        },
        password?: string
    ): Promise<FrontendMarketInquiryItem> => {
        const res = await invoke<FrontendMarketInquiryItem>("send_market_inquiry_message_via_post", {
            inquiryId,
            text: options.text || undefined,
            decision: options.decision || undefined,
            paymentUri: options.paymentUri || undefined,
            amountHint: options.amountHint || undefined,
            standardUuid: options.standardUuid || undefined,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
        return res;
    },

    markInquiryRead: async (
        inquiryId: string,
        password?: string
    ): Promise<void> => {
        await invoke<void>("mark_inquiry_read", {
            inquiryId,
            password: password || undefined,
        });
        notifyPostInboxUpdated();
    },

    dismissPendingPostItem: async (
        itemKind: 'sign_request' | 'wot_handshake' | 'inquiry' | 'unsupported' | 'tolerance_payment',
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

    confirmPendingPostPayment: async (
        msgId: string,
        password?: string
    ): Promise<ReceiveSuccessPayload> => {
        const res = await invoke<ReceiveSuccessPayload>("confirm_pending_post_payment", {
            msgId,
            password: password || undefined,
        });
        if (res?.newlyCommitted !== false) {
            afterWalletMutation(password);
        }
        notifyPostInboxUpdated();
        return res;
    },

    getPostInboxSummary: async (
        password?: string
    ): Promise<FrontendPostInboxSummary> => {
        return await invoke<FrontendPostInboxSummary>("get_post_inbox_summary", password ? { password } : {});
    },

    syncPostChannels: async (
        isWifi?: boolean,
        password?: string
    ): Promise<FrontendPostSyncResult> => {
        const res = await invoke<FrontendPostSyncResult>("sync_post_channels", {
            isWifi: typeof isWifi === 'boolean' ? isWifi : detectIsWifiConnection(),
            password: password || undefined,
        });
        if (!res) {
            return res;
        }
        const hasBackupChanges =
            res.uploadedTo > 0 ||
            res.importedPayments > 0 ||
            res.importedReceipts > 0 ||
            res.importedSignatures > 0 ||
            res.newSignRequests > 0 ||
            res.newWotHandshakes > 0 ||
            (res.importedWotLists ?? 0) > 0 ||
            res.newInquiries > 0 ||
            res.newInquiryMessages > 0 ||
            (res.newTolerancePayments ?? 0) > 0;
        const hasInboxChanges = hasBackupChanges || res.prunedBoxes > 0;

        if (hasBackupChanges) {
            triggerAutoBackup();
        }
        if (hasInboxChanges) {
            notifyPostInboxUpdated();
            emitAppEvent(APP_EVENTS.SYNC_COMPLETED, { source: 'post' });
        }
        if (res.newWotHandshakes > 0 || (res.importedWotLists ?? 0) > 0) {
            emitAppEvent(APP_EVENTS.WOT_UPDATED);
        }
        if (hasInboxChanges || res.polledSyncs > 0) {
            notifyAppNoticesUpdated();
        }
        return res;
    },
};
