// src/components/transfer/sendTransferHelpers.ts
import type { TFunction } from 'i18next';
import { transferService, CreateBundleResult } from '../../services/transferService';
import { runPostSyncPass } from '../../sync/syncBridge';
import { parsePaymentRequest } from '../../utils/humocoUri';
import { logger } from '../../utils/log';
import { stringifyError, translateError } from '../../utils/errorHelper';
import type { ProtectActionFn } from '../../utils/fileIo';
import type {
    SourceTransfer,
    VoucherStandardInfo,
} from '../../types';

export type SendState = {
    recipientId: string;
    notes: string;
    sendProfileName: boolean;
    customSenderName: string;
    targetAmountStr: string;
    selectedStandardId: string | null;
    selectedIsTest: boolean | null;
    privacyMode: 'public' | 'stealth' | null;
    isProcessing: boolean;
    feedbackMsg: string;
    showConfirm: boolean;
    recipientError: boolean;
    privacyError: boolean;
};

export type SendAction =
    | { type: 'SET_RECIPIENT'; id: string }
    | { type: 'SET_NOTES'; notes: string }
    | { type: 'TOGGLE_PROFILE_NAME'; value: boolean }
    | { type: 'SET_SENDER_NAME'; name: string }
    | { type: 'SET_TARGET_AMOUNT'; amount: string }
    | { type: 'SET_STANDARD'; id: string | null; isTest: boolean | null; preserveAmount?: boolean }
    | { type: 'SET_PRIVACY'; mode: 'public' | 'stealth' | null }
    | { type: 'SET_PROCESSING'; value: boolean }
    | { type: 'SET_FEEDBACK'; msg: string }
    | { type: 'TOGGLE_CONFIRM'; value: boolean }
    | { type: 'SET_RECIPIENT_ERROR'; value: boolean }
    | { type: 'SET_PRIVACY_ERROR'; value: boolean }
    | { type: 'RESET'; recipientId?: string; notes?: string };

export const initialSendState: SendState = {
    recipientId: '',
    notes: '',
    sendProfileName: true,
    customSenderName: '',
    targetAmountStr: '',
    selectedStandardId: null,
    selectedIsTest: null,
    privacyMode: null,
    isProcessing: false,
    feedbackMsg: '',
    showConfirm: false,
    recipientError: false,
    privacyError: false,
};

export function sendReducer(state: SendState, action: SendAction): SendState {
    switch (action.type) {
        case 'SET_RECIPIENT':
            return { ...state, recipientId: action.id.replace(/\s+/g, ''), recipientError: false };
        case 'SET_NOTES':
            return { ...state, notes: action.notes };
        case 'TOGGLE_PROFILE_NAME':
            return { ...state, sendProfileName: action.value };
        case 'SET_SENDER_NAME':
            return { ...state, customSenderName: action.name };
        case 'SET_TARGET_AMOUNT':
            return { ...state, targetAmountStr: action.amount };
        case 'SET_STANDARD': {
            const changed =
                action.id !== state.selectedStandardId || action.isTest !== state.selectedIsTest;
            return {
                ...state,
                selectedStandardId: action.id,
                selectedIsTest: action.isTest,
                targetAmountStr: action.preserveAmount ? state.targetAmountStr : '',
                privacyMode: changed ? null : state.privacyMode,
                privacyError: false,
            };
        }
        case 'SET_PRIVACY':
            return { ...state, privacyMode: action.mode, privacyError: false };
        case 'SET_PROCESSING':
            return { ...state, isProcessing: action.value };
        case 'SET_FEEDBACK':
            return { ...state, feedbackMsg: action.msg };
        case 'TOGGLE_CONFIRM':
            return { ...state, showConfirm: action.value };
        case 'SET_RECIPIENT_ERROR':
            return { ...state, recipientError: action.value };
        case 'SET_PRIVACY_ERROR':
            return { ...state, privacyError: action.value };
        case 'RESET':
            return {
                ...initialSendState,
                recipientId: action.recipientId ? action.recipientId.replace(/\s+/g, '') : '',
                notes: action.notes ?? '',
            };
        default:
            return state;
    }
}

export function formatProfileDisplayName(
    profile?: {
        firstName?: string;
        lastName?: string;
        organization?: string;
        community?: string;
    } | null,
): string {
    if (!profile) return '';
    const fullName = [profile.firstName, profile.lastName].filter(Boolean).join(' ').trim();
    let name = fullName || profile.organization || '';
    if (profile.community) {
        name = name ? `${name} (${profile.community})` : profile.community;
    }
    return name.startsWith('did:key:') ? '' : name;
}

export interface PaymentRequestResolution {
    matched: boolean;
    to?: string;
    note?: string;
    standardId?: string | null;
    isTest?: boolean;
    amount?: string;
    missingStandardName?: string;
}

export function resolvePaymentRequestInput(
    rawText: string,
    standardIdToUuidMap: Map<string, string>,
    voucherStandards: VoucherStandardInfo[],
    currentSelectedStandardId: string | null,
): PaymentRequestResolution {
    const parsed = parsePaymentRequest(rawText);
    if (!parsed) return { matched: false };

    let matchedStandardId: string | null = null;
    const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

    if (parsed.standardUuid) {
        for (const [id, uuid] of standardIdToUuidMap.entries()) {
            if (uuid === parsed.standardUuid || id === parsed.standardUuid) {
                matchedStandardId = id;
                break;
            }
        }
        if (!matchedStandardId) {
            const isStrictUuid = uuidPattern.test(parsed.standardUuid);
            const foundByIdOrName = voucherStandards.find(
                (s) =>
                    s.id === parsed.standardUuid ||
                    (!isStrictUuid &&
                        s.displayName?.toLowerCase() === parsed.standardUuid?.toLowerCase()),
            );
            if (foundByIdOrName) matchedStandardId = foundByIdOrName.id;
        }
    } else if (parsed.standardName) {
        const found = voucherStandards.find(
            (s) => s.displayName?.toLowerCase() === parsed.standardName?.toLowerCase(),
        );
        if (found) matchedStandardId = found.id;
    }

    if (!matchedStandardId && (parsed.standardUuid || parsed.standardName)) {
        return {
            matched: true,
            to: parsed.to,
            note: parsed.note,
            missingStandardName: parsed.standardName || parsed.standardUuid,
        };
    }

    return {
        matched: true,
        to: parsed.to,
        note: parsed.note,
        standardId: matchedStandardId ?? currentSelectedStandardId,
        isTest: matchedStandardId ? (parsed.isTest ?? false) : parsed.isTest,
        amount: parsed.amount,
    };
}

export interface SendDraftKeyInput {
    recipientId: string;
    targetAmountStr: string;
    standardUuid: string | null;
    isTest: boolean | null;
}

export function buildSendDraftKey({
    recipientId,
    targetAmountStr,
    standardUuid,
    isTest,
}: SendDraftKeyInput): string {
    const normalizedAmount = targetAmountStr.trim().replace(',', '.');
    return [
        recipientId.trim(),
        normalizedAmount,
        standardUuid ?? '',
        isTest === null ? 'null' : String(isTest),
    ].join('|');
}

export interface ConfirmedTransferSnapshot {
    draftKey: string;
    recipientId: string;
    selectedMap: Map<string, string>;
    selectionStats: Record<string, { total: number; totalExact?: string; count: number }>;
    notes: string;
    sendProfileName: boolean;
    customSenderName: string;
    privacyMode: 'public' | 'stealth' | null;
    sendCardOverride: boolean | null;
}

export interface CommitTransferParams {
    snapshot: ConfirmedTransferSnapshot;
    voucherStandards: VoucherStandardInfo[];
    standardIdToUuidMap: Map<string, string>;
    protectAction: ProtectActionFn;
    checkAndTriggerCooldown: (errOrResult: unknown) => boolean;
    translate: TFunction;
}

export async function commitAndDepositTransfer({
    snapshot,
    voucherStandards,
    standardIdToUuidMap,
    protectAction,
    checkAndTriggerCooldown,
    translate,
}: CommitTransferParams): Promise<{
    bundleResult: CreateBundleResult;
    depositedTo: number;
    recipientDepositedTo: number;
    depositError?: string;
} | null> {
    const trimmedName = snapshot.customSenderName.trim();
    const senderName = snapshot.sendProfileName && trimmedName.length > 0 ? trimmedName : null;
    const sources: SourceTransfer[] = Array.from(snapshot.selectedMap.entries()).map(([id, amount]) => ({
        localInstanceId: id,
        amountToSend: amount.trim().replace(',', '.'),
    }));

    const standardDefinitionsToml: Record<string, string> = {};
    voucherStandards.forEach((s) => {
        standardDefinitionsToml[standardIdToUuidMap.get(s.id) || s.id] = s.content;
    });

    const outcome = await protectAction(async (password) => {
        const bundleResult: CreateBundleResult = await transferService.createBundle({
            recipientId: snapshot.recipientId,
            sources,
            notes: snapshot.notes || null,
            senderProfileName: senderName,
            standardDefinitionsToml,
            usePrivacyMode: snapshot.privacyMode === 'stealth',
            sendCardOverride: snapshot.sendCardOverride ?? undefined,
            depositToPost: true,
            password,
        });

        let depositedTo = 0;
        let recipientDepositedTo = 0;
        let depositError: string | undefined;
        if (bundleResult.postDeposit) {
            const depositRes = bundleResult.postDeposit;
            depositedTo = depositRes.depositedTo;
            recipientDepositedTo = depositRes.recipientDepositedTo;
            if (depositRes.errors?.length) {
                depositError = depositRes.errors.join('; ');
                logger.warn(`Post deposit completed with ${depositRes.errors.length} error(s)`);
            }
            runPostSyncPass().catch(() => {});
        } else if (bundleResult.postDepositError) {
            checkAndTriggerCooldown(bundleResult.postDepositError);
            depositError = translateError(bundleResult.postDepositError, translate);
            logger.warn(
                `Post deposit failed after transfer commit: ${stringifyError(bundleResult.postDepositError)}`,
            );
        }

        return { bundleResult, depositedTo, recipientDepositedTo, depositError };
    });

    return outcome ?? null;
}
