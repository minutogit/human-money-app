// src/types/transaction.ts

import { TransactionHistoryEntry } from './voucher';

export interface AggregatedBalance {
    standardName: string;
    standardUuid: string;
    unit: string;
    totalAmount: string;
    displayCurrency: string;
    displayStandardName: string;
    isTestVoucher: boolean;
}

export interface InvolvedVoucherInfo {
    localInstanceId: string;
    voucherId: string;
    standardName: string;
    amount: string;
    unit: string;
    isTestVoucher: boolean;
    displayCurrency: string;
    displayStandardName: string;
}

export interface SourceTransfer {
    localInstanceId: string;
    amountToSend: string;
}

export interface MultiTransferRequest {
    recipientId: string;
    sources: SourceTransfer[];
    notes?: string | null;
    senderProfileName?: string | null;
    standardDefinitionsToml: Record<string, string>;
    usePrivacyMode?: boolean | null;
    password?: string | null;
}

export interface TransactionRecord {
    id: string;
    direction: 'sent' | 'received';
    recipientId?: string;
    senderId?: string;
    timestamp: string;
    summableAmounts: Record<string, string>;
    countableItems: Record<string, number>;
    involvedVouchers: string[];

    // Optionale Felder für detaillierte Records
    bundleData?: number[];
    bundleId?: string;
    notes?: string;
    senderProfileName?: string;
    involvedSourcesDetails?: InvolvedVoucherInfo[];
}

export interface TransferSummary {
    summableAmounts: Record<string, string>;
    countableItems: Record<string, number>;
}

export interface ReceiveSuccessPayload {
    senderId: string;
    senderProfileName?: string;
    notes?: string;
    transferSummary: TransferSummary;
    involvedVouchers: string[];
    involvedVouchersDetails?: InvolvedVoucherInfo[];
    isSignatureAttached?: boolean;
    voucherId?: string;
    verifiableConflicts?: Record<string, TransactionFingerprint[]>;
}

export interface TransactionFingerprint {
    dsTag: string;
    u: string;
    blindedId: string;
    tId: string;
    encryptedTimestamp: string; // u128 usually serializes to string
    layer2Signature: string;
    deletableAt: string;
}

export type ConflictRole = "victim" | "witness";
export type TrustStatus = "clean" | { knownOffender: string } | { resolved: [string, boolean] };

export interface ProofOfDoubleSpendSummary {
    proofId: string;
    offenderId: string;
    forkPointPrevHash: string;
    reportTimestamp: string;
    isResolved: boolean;
    hasL2Verdict: boolean;
    localOverride: boolean;
    conflictRole: ConflictRole;
    affectedVoucherName?: string;
    voucherStandardUuid?: string;
    localNote?: string;
    isTestVoucher: boolean;
}

export interface ProofOfDoubleSpend {
    proofId: string;
    offenderId: string;
    forkPointPrevHash: string;
    conflictingTransactions: TransactionHistoryEntry[];
    deletableAt: string;
    reporterId: string;
    reportTimestamp: string;
    reporterSignature: string;
    resolutions?: ResolutionEndorsement[];
    layer2Verdict?: unknown;
    affectedVoucherName?: string;
    voucherStandardUuid?: string;
    isResolved?: boolean;
    nonRedeemableTestVoucher: boolean;
}

export interface ResolutionEndorsement {
    endorsementId: string;
    proofId: string;
    victimId: string;
    resolutionTimestamp: string;
    notes?: string;
    victimSignature: string;
}

export interface FullProofDetails {
    proof: ProofOfDoubleSpend;
    localOverride: boolean;
    conflictRole: ConflictRole;
    localNote?: string;
}

export type WalletEventType =
    | "voucherCreated"
    | "transferSent"
    | "transferReceived"
    | "voucherQuarantined"
    | "voucherActivated"
    | "voucherVoided"
    | "voucherExpired"
    | { unknown: string };

export interface EventBffData {
    displayCurrency: string;
    amount: string;
    isTestVoucher: boolean;
    counterpartyId?: string;
    counterpartyName?: string;
}

export interface WalletEvent {
    eventId: string;
    localInstanceId: string;
    voucherId: string;
    timestamp: string; // ISO 8601
    eventType: WalletEventType;
    bffData: EventBffData;
}
