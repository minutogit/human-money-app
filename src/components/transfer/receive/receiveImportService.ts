// src/components/transfer/receive/receiveImportService.ts
import { transferService } from '../../../services/transferService';
import { voucherService } from '../../../services/voucherService';
import { wotService } from '../../../services/wotService';
import { integrityService } from '../../../services/integrityService';
import { marketService } from '../../../services/marketService';
import { parseStandardInfo } from '../../../hooks/useParsedStandards';
import { extractDetachedSignatureVoucherId, ReceiveImportKind } from '../../../utils/humocoFileClassifier';
import { isBackendError, stringifyError } from '../../../utils/errorHelper';
import { APP_EVENTS, emitAppEvent } from '../../../utils/appEvents';
import type { ProtectActionFn } from '../../../utils/fileIo';
import type {
    ReceiveSuccessPayload,
    VoucherDetails,
    VoucherStandardInfo,
    VoucherSummary,
    WotImportResultDto,
} from '../../../types';

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

async function resolveDetachedSigCandidates(
    fileData: number[],
    standardDefinitionsToml: Record<string, string>,
): Promise<string[]> {
    const targetVoucherId = extractDetachedSignatureVoucherId(fileData);
    let matchedStandardUuid: string | null = null;
    const summaries = await voucherService.getSummaries().catch(() => [] as VoucherSummary[]);
    const incompleteVouchers = summaries.filter((v) => {
        const statusStr = typeof v.status === 'string' ? v.status : Object.keys(v.status)[0];
        return statusStr?.toLowerCase() === 'incomplete';
    });

    if (targetVoucherId) {
        const searchOrder = [
            ...incompleteVouchers,
            ...summaries.filter((s) => !incompleteVouchers.includes(s)),
        ];
        for (const v of searchOrder) {
            try {
                const details = await voucherService.getDetails(v.localInstanceId);
                if (details?.voucher?.voucherId === targetVoucherId) {
                    matchedStandardUuid =
                        v.voucherStandardUuid || details.voucher.voucherStandard?.uuid || null;
                    break;
                }
            } catch {
                // Ignore details fetch failure for individual vouchers
            }
        }
    }

    if (!matchedStandardUuid && incompleteVouchers.length > 0) {
        const incompleteUuids = Array.from(
            new Set(incompleteVouchers.map((v) => v.voucherStandardUuid).filter(Boolean)),
        );
        if (incompleteUuids.length === 1) {
            matchedStandardUuid = incompleteUuids[0];
        }
    }

    const allStandards = Object.values(standardDefinitionsToml);
    const preferredToml = matchedStandardUuid
        ? standardDefinitionsToml[matchedStandardUuid]
        : undefined;
    const candidates = preferredToml
        ? [preferredToml, ...allStandards.filter((s) => s !== preferredToml)]
        : allStandards;
    return candidates.length > 0 ? candidates : [''];
}

export type ReceiveImportOutcome =
    | { kind: 'success'; payload: ReceiveSuccessPayload | VoucherDetails }
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
                password: password || walletPassword || undefined,
                forceAcceptToleranceBundle: false,
            });
        });

        if (
            payload &&
            ((payload.verifiableConflicts && Object.keys(payload.verifiableConflicts).length > 0) ||
                (payload.conflictSummaries?.some((c) => !c.isResolved && !c.localOverride) ?? false))
        ) {
            emitAppEvent(APP_EVENTS.CONFLICTS_UPDATED);
        }

        return payload ? { kind: 'success', payload } : { kind: 'cancelled' };
    }

    if (type === 'ask') {
        const openedVoucher = await voucherService.openSigningRequest(
            fileData,
            password || undefined,
        );
        return { kind: 'success', payload: openedVoucher };
    }

    if (type === 'sig') {
        const standardDefinitionsToml = await ensureStandardsToml();
        const tomlCandidates = await resolveDetachedSigCandidates(
            fileData,
            standardDefinitionsToml,
        );
        const updatedInstanceId = await protectAction(async (walletPassword) => {
            let lastErr: unknown = null;
            for (const stdToml of tomlCandidates) {
                try {
                    return await voucherService.processAndAttachSignature(
                        fileData,
                        stdToml,
                        password || undefined,
                        walletPassword || undefined,
                    );
                } catch (err) {
                    if (isAuthOrDecryptError(err)) {
                        throw err;
                    }
                    lastErr = err;
                }
            }
            throw lastErr;
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
