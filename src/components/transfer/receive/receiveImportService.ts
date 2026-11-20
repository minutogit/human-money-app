// src/components/transfer/receive/receiveImportService.ts
import { transferService } from '../../../services/transferService';
import { voucherService } from '../../../services/voucherService';
import { wotService } from '../../../services/wotService';
import { integrityService } from '../../../services/integrityService';
import { marketService } from '../../../services/marketService';
import { parseStandardInfo } from '../../../hooks/useParsedStandards';
import { ReceiveImportKind } from '../../../utils/humocoFileClassifier';
import { isBackendError, stringifyError } from '../../../utils/errorHelper';
import { APP_EVENTS, emitAppEvent } from '../../../utils/appEvents';
import type { ProtectActionFn } from '../../../utils/fileIo';
import type {
    PendingReceiveCommit,
    ReceiveCommitMode,
    ReceiveSuccessPayload,
    VoucherDetails,
    VoucherStandardInfo,
    WotImportResultDto,
} from '../../../types';

export function emitConflictsUpdatedIfPresent(payload: ReceiveSuccessPayload): void {
    if (
        (payload.verifiableConflicts && Object.keys(payload.verifiableConflicts).length > 0) ||
        (payload.conflictSummaries?.some((c) => !c.isResolved && !c.localOverride) ?? false)
    ) {
        emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
    }
}

export function classifyToleranceError(err: unknown): 'Soft' | 'Critical' | null {
    const errStr = stringifyError(err);
    const isTolerance =
        (isBackendError(err) &&
            (err.code === 'error.transfer.bundleRecoveryZone' ||
                err.code === 'error.transfer.bundleToleranceZone')) ||
        errStr.includes('ToleranceZone');
    if (!isTolerance) return null;
    const isCritical =
        (isBackendError(err) && err.code === 'error.transfer.bundleRecoveryZone') ||
        errStr.includes('Extended');
    return isCritical ? 'Critical' : 'Soft';
}

export async function commitPendingReceiveBundle(params: {
    pendingCommit: PendingReceiveCommit;
    forceAcceptToleranceBundle: boolean;
    commitMode?: ReceiveCommitMode;
    purchaseId?: string;
    protectAction?: ProtectActionFn;
}): Promise<ReceiveSuccessPayload | null> {
    const runCommit = async (walletPassword: string | null) =>
        await transferService.receiveBundle({
            bundleData: params.pendingCommit.bundleData,
            standardDefinitionsToml: params.pendingCommit.standardDefinitionsToml,
            password: walletPassword || params.pendingCommit.bundlePassword || undefined,
            forceAcceptToleranceBundle: params.forceAcceptToleranceBundle,
            commitMode: params.commitMode ?? 'forceTrustOffline',
            purchaseId: params.purchaseId,
        });
    const payload = params.protectAction
        ? await params.protectAction(runCommit)
        : await runCommit(null);
    if (!payload) return null;
    emitConflictsUpdatedIfPresent(payload);
    return payload;
}

export function buildStandardDefinitionsToml(
    standards: VoucherStandardInfo[],
): Record<string, string> {
    const map: Record<string, string> = {};
    for (const standard of standards) {
        const parsed = parseStandardInfo(standard);
        if (parsed.uuid) {
            map[parsed.uuid] = standard.content;
        }
    }
    return map;
}

export function isAuthOrDecryptError(err: unknown): boolean {
    const errMsg = stringifyError(err);
    const errCode = isBackendError(err) ? err.code : '';
    return (
        errCode === 'error.wallet.locked' ||
        errCode === 'error.auth.sessionTimeout' ||
        errCode === 'error.auth.passwordRequired' ||
        errMsg.includes('Password required') ||
        errMsg.includes('Session timed out') ||
        errMsg.includes('Wallet is locked') ||
        errMsg.toLowerCase().includes('decrypt') ||
        errMsg.includes('SymmetricEncryption') ||
        errMsg.includes('MacError')
    );
}

export type ReceiveImportOutcome =
    | {
          kind: 'success';
          payload: ReceiveSuccessPayload | VoucherDetails;
          pendingCommit?: PendingReceiveCommit;
      }
    | { kind: 'signatureAttached'; voucherId: string }
    | { kind: 'wotImported'; result: WotImportResultDto }
    | { kind: 'proofImported'; proofId: string }
    | { kind: 'cardImported'; status: string; did?: string | null }
    | { kind: 'cancelled' };

export interface ExecuteReceiveImportParams {
    fileData: number[];
    type: ReceiveImportKind;
    password?: string;
    ensureStandardsToml: () => Promise<Record<string, string>>;
    protectAction: ProtectActionFn;
}

/**
 * Executes cryptographic verification & import for a classified Human Money payload.
 */
export async function executeReceiveImport({
    fileData,
    type,
    password,
    ensureStandardsToml,
    protectAction,
}: ExecuteReceiveImportParams): Promise<ReceiveImportOutcome> {
    if (type === 'transfer') {
        const standardDefinitionsToml = await ensureStandardsToml();

        const payload = await protectAction(async (walletPassword) => {
            return await transferService.receiveBundle({
                bundleData: fileData,
                standardDefinitionsToml,
                password: walletPassword || password || undefined,
                forceAcceptToleranceBundle: false,
                commitMode: 'onlineVerify',
            });
        });

        if (payload) {
            emitConflictsUpdatedIfPresent(payload);
        }

        const pendingCommit: PendingReceiveCommit | undefined =
            payload && payload.newlyCommitted === false && !payload.isReplay
                ? {
                      bundleData: fileData,
                      standardDefinitionsToml,
                      bundlePassword: password || undefined,
                      forceAcceptToleranceBundle: false,
                  }
                : undefined;

        return payload ? { kind: 'success', payload, pendingCommit } : { kind: 'cancelled' };
    }

    if (type === 'ask') {
        const openedVoucher = await voucherService.openSigningRequest(
            fileData,
            password || undefined,
        );
        return { kind: 'success', payload: openedVoucher };
    }

    if (type === 'sig') {
        const updatedInstanceId = await protectAction(async (walletPassword) => {
            return await voucherService.processAndAttachSignature(
                fileData,
                password || undefined,
                walletPassword || undefined,
            );
        });

        return updatedInstanceId
            ? { kind: 'signatureAttached', voucherId: updatedInstanceId }
            : { kind: 'cancelled' };
    }

    if (type === 'wot') {
        const wotResult = await protectAction(async (walletPassword) => {
            return await wotService.importPackage({
                containerBytes: fileData,
                containerPassword: password || null,
                password: walletPassword || null,
            });
        });
        return wotResult ? { kind: 'wotImported', result: wotResult } : { kind: 'cancelled' };
    }

    if (type === 'proof') {
        const proofId = await protectAction(async (walletPassword) => {
            return await integrityService.importDoubleSpendProof(
                fileData,
                walletPassword || undefined,
            );
        });
        return proofId ? { kind: 'proofImported', proofId } : { kind: 'cancelled' };
    }

    const text = new TextDecoder().decode(new Uint8Array(fileData));
    const cardResult = await protectAction(async (walletPassword) => {
        return await marketService.importContactCard(text, walletPassword || undefined);
    });
    return cardResult
        ? { kind: 'cardImported', status: cardResult.status, did: cardResult.did }
        : { kind: 'cancelled' };
}
