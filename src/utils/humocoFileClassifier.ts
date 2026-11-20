// src/utils/humocoFileClassifier.ts
import type { TFunction } from 'i18next';
import type { FileDialogFilter } from './fileIo';
import { extractHumocoDeepLink } from './humocoUri';

export { extractHumocoDeepLink };

export type HumocoFileKind =
    | 'transfer'
    | 'ask'
    | 'sig'
    | 'wot'
    | 'standard'
    | 'proof'
    | 'card'
    | 'backup'
    | 'deeplink'
    | 'unknown';

export type ReceiveImportKind = Exclude<
    HumocoFileKind,
    'standard' | 'backup' | 'deeplink' | 'unknown'
>;

export interface HumocoFileClassification {
    kind: HumocoFileKind;
    source: 'magic' | 'extension' | 'unknown';
    deepLink?: string;
    containerType?: string;
}

export interface ClassifyHumocoFileInput {
    fileName?: string;
    bytes?: Uint8Array | number[];
    text?: string;
}

export const RECEIVE_FILE_EXTENSIONS = [
    '.transfer',
    '.hmcbundle',
    '.hmc',
    '.hmcpost',
    '.ask',
    '.humocoreq',
    '.sig',
    '.humocosig',
    '.wot',
    '.hmcwot',
    '.standard',
    '.toml',
    '.hmcs',
    '.hmcproof',
    '.hmcard',
    '.humoco',
] as const;

export const SUPPORTED_DROP_EXTENSIONS = [
    ...RECEIVE_FILE_EXTENSIONS,
    '.hmcbackup',
] as const;

const EXTENSION_KIND_MAP: ReadonlyArray<readonly [string, HumocoFileKind]> = [
    ['.hmcbackup', 'backup'],
    ['.hmcard', 'card'],
    ['.hmcproof', 'proof'],
    ['.wot', 'wot'],
    ['.hmcwot', 'wot'],
    ['.ask', 'ask'],
    ['.humocoreq', 'ask'],
    ['.sig', 'sig'],
    ['.humocosig', 'sig'],
    ['.standard', 'standard'],
    ['.toml', 'standard'],
    ['.hmcs', 'standard'],
    ['.transfer', 'transfer'],
    ['.hmcbundle', 'transfer'],
    ['.hmc', 'transfer'],
    ['.hmcpost', 'transfer'],
];

const CONTAINER_TYPE_MAP: Record<string, HumocoFileKind> = {
    TransactionBundle: 'transfer',
    VoucherForSigning: 'ask',
    DetachedSignature: 'sig',
    TrustAssertion: 'wot',
    ProofOfDoubleSpend: 'proof',
    VoucherStandardDefinition: 'standard',
};

function decodeUtf8Text(bytes?: Uint8Array | number[], maxBytes: number = 8 * 1024 * 1024): string | null {
    if (!bytes) return null;
    const len = bytes instanceof Uint8Array ? bytes.byteLength : bytes.length;
    if (len === 0 || len > maxBytes) return null;
    try {
        const view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
        return new TextDecoder('utf-8', { fatal: false }).decode(view);
    } catch {
        return null;
    }
}

/**
 * Checks whether a file name has a recognized Human Money drop or import extension.
 */
export function isSupportedHumocoFileName(
    fileName: string,
    includeBackup: boolean = true,
): boolean {
    const lower = fileName.trim().toLowerCase();
    const list = includeBackup ? SUPPORTED_DROP_EXTENSIONS : RECEIVE_FILE_EXTENSIONS;
    return list.some((ext) => lower.endsWith(ext));
}

/**
 * Resolves the kind mapped purely from a file extension, or `'unknown'`.
 */
export function classifyExtension(fileName?: string): HumocoFileKind {
    if (!fileName) return 'unknown';
    const lower = fileName.trim().toLowerCase();
    for (const [ext, kind] of EXTENSION_KIND_MAP) {
        if (lower.endsWith(ext)) {
            return kind;
        }
    }
    return 'unknown';
}

function classifyJsonObject(obj: Record<string, unknown>): HumocoFileClassification | null {
    if (obj.format === 'hmc-backup-manifest-v1') {
        return { kind: 'backup', source: 'magic', containerType: 'hmc-backup-manifest-v1' };
    }

    if (typeof obj.c === 'string' && obj.c in CONTAINER_TYPE_MAP) {
        return {
            kind: CONTAINER_TYPE_MAP[obj.c],
            source: 'magic',
            containerType: obj.c,
        };
    }

    if (typeof obj.card_wire === 'string') {
        return { kind: 'card', source: 'magic', containerType: 'CardEnvelope' };
    }

    if (obj.typ === 'wot-provider-filter+v1') {
        return { kind: 'wot', source: 'magic', containerType: 'wot-provider-filter+v1' };
    }

    if (
        Array.isArray(obj.lists) &&
        obj.lists.every((item) => typeof item === 'string')
    ) {
        return { kind: 'wot', source: 'magic', containerType: 'WotSyncPackage' };
    }

    if (
        typeof obj.proof_id === 'string' &&
        Array.isArray(obj.conflicting_transactions)
    ) {
        return { kind: 'proof', source: 'magic', containerType: 'ProofOfDoubleSpend' };
    }

    return null;
}

function looksLikeVoucherStandardToml(text: string): boolean {
    const hasUuid = /^\s*uuid\s*=\s*["'][^"']+["']/m.test(text);
    if (!hasUuid) return false;
    return (
        /^\s*(unit|issuer_name|amount_decimal_places)\s*=/m.test(text) ||
        /^\[immutable(\.identity)?\]/m.test(text)
    );
}

/**
 * Classifies a Human Money file by inspecting JSON/TOML container magic first,
 * falling back to file extension detection and embedded `humoco://` deep links.
 */
export function classifyHumocoFile(input: ClassifyHumocoFileInput): HumocoFileClassification {
    const extKind = classifyExtension(input.fileName);
    const lowerName = (input.fileName || '').trim().toLowerCase();
    const rawText = input.text ?? decodeUtf8Text(input.bytes);

    if (rawText) {
        const trimmed = rawText.trim();
        if (trimmed.startsWith('{')) {
            try {
                const parsed = JSON.parse(trimmed) as unknown;
                if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
                    const magicResult = classifyJsonObject(parsed as Record<string, unknown>);
                    if (magicResult) {
                        return magicResult;
                    }
                }
            } catch {
                // Not valid JSON; continue to TOML / deep link / extension fallback.
            }
        }

        if (looksLikeVoucherStandardToml(trimmed)) {
            return { kind: 'standard', source: 'magic', containerType: 'VoucherStandardToml' };
        }

        if (
            extKind === 'unknown' ||
            lowerName.endsWith('.txt') ||
            lowerName.endsWith('.html') ||
            lowerName.endsWith('.json')
        ) {
            const deepLink = extractHumocoDeepLink(trimmed);
            if (deepLink) {
                return { kind: 'deeplink', source: 'magic', deepLink };
            }
        }
    }

    if (extKind !== 'unknown') {
        if (extKind === 'standard' && lowerName.endsWith('.toml') && rawText != null) {
            return { kind: 'unknown', source: 'unknown' };
        }
        return { kind: extKind, source: 'extension' };
    }

    return { kind: 'unknown', source: 'unknown' };
}

/**
 * Extracts `voucher_id` from an unencrypted `DetachedSignature` container payload
 * to aid standard matching before verification.
 */
export function extractDetachedSignatureVoucherId(
    fileData: Uint8Array | number[],
): string | null {
    const jsonStr = decodeUtf8Text(fileData);
    if (!jsonStr) return null;

    try {
        const container = JSON.parse(jsonStr) as {
            ciphertext?: unknown;
            recipients?: unknown;
        };
        if (
            typeof container.ciphertext === 'string' &&
            (!Array.isArray(container.recipients) || container.recipients.length === 0)
        ) {
            const base64 = container.ciphertext.replace(/-/g, '+').replace(/_/g, '/');
            const payloadStr = window.atob(base64);
            const sigObj = JSON.parse(payloadStr) as {
                Signature?: { voucher_id?: unknown };
            };
            const inner = sigObj.Signature;
            if (inner && typeof inner.voucher_id === 'string') {
                return inner.voucher_id;
            }
        }
    } catch {
        // Ciphertext may be encrypted or non-JSON; ignore safely.
    }
    return null;
}

export function isReceiveImportKind(kind: HumocoFileKind): kind is ReceiveImportKind {
    return (
        kind === 'transfer' ||
        kind === 'ask' ||
        kind === 'sig' ||
        kind === 'wot' ||
        kind === 'proof' ||
        kind === 'card'
    );
}

/**
 * Builds canonical localized Tauri file dialog filters for the Receive / Import view.
 */
export function getReceiveFileDialogFilters(t: TFunction): FileDialogFilter[] {
    return [
        {
            name: t('transfer.importFile.allFilesFilter', { defaultValue: 'Alle Human-Money-Dateien' }),
            extensions: [
                'transfer',
                'hmcbundle',
                'hmc',
                'hmcpost',
                'humoco',
                'json',
                'ask',
                'sig',
                'standard',
                'toml',
                'hmcs',
                'humocoreq',
                'humocosig',
                'wot',
                'hmcwot',
                'hmcproof',
                'hmcard',
            ],
        },
        {
            name: t('transfer.importFile.transferFilter', { defaultValue: 'Zahlungsdatei (.transfer)' }),
            extensions: ['transfer', 'hmcbundle', 'hmc', 'hmcpost'],
        },
        {
            name: t('transfer.importFile.askFilter', { defaultValue: 'Unterschrifts-Anfrage (.ask)' }),
            extensions: ['ask', 'humocoreq'],
        },
        {
            name: t('transfer.importFile.sigFilter', { defaultValue: 'Unterschrifts-Antwort (.sig)' }),
            extensions: ['sig', 'humocosig'],
        },
        {
            name: t('transfer.importFile.proofFilter', { defaultValue: 'Konfliktnachweis (.hmcproof)' }),
            extensions: ['hmcproof'],
        },
        {
            name: t('transfer.importFile.standardFilter', { defaultValue: 'Gutschein-Standard (.standard, .toml)' }),
            extensions: ['standard', 'toml', 'hmcs'],
        },
        {
            name: t('transfer.importFile.wotFilter', { defaultValue: 'Vertrauensnetz-Datei (.wot)' }),
            extensions: ['wot', 'hmcwot'],
        },
        {
            name: t('marketplace.cardFilterName', { defaultValue: 'Visitenkarte (.hmcard)' }),
            extensions: ['hmcard'],
        },
        {
            name: t('common.allFiles', { defaultValue: 'Alle Dateien' }),
            extensions: ['*'],
        },
    ];
}

/**
 * Translates non-accepted CardImportResult status strings into user-facing feedback.
 */
export function formatCardImportStatusFeedback(status: string, t: TFunction): string {
    const statusMap: Record<string, string> = {
        ignored_stale: t('marketplace.importIgnoredDesc'),
        rejected_expired: t('marketplace.importExpiredDesc'),
        rejected_hops: t('marketplace.importHopsDesc'),
        rejected_offender: t('marketplace.importOffenderDesc'),
    };
    return statusMap[status] || status;
}
