// src/types/api.ts

import { ReceiveSuccessPayload } from './transaction';
import { VoucherDetails } from './voucher';

export interface ReceiveBundleArgs {
    bundleData: number[];
    standardDefinitionsToml: Record<string, string>;
    password?: string | null;
    forceAcceptToleranceBundle: boolean;
}

export interface OpenVoucherSigningRequestArgs {
    containerBytes: number[];
    password?: string | null;
}

export interface ProcessAndAttachSignatureArgs {
    containerBytes: number[];
    standardTomlContent: string;
    containerPassword?: string | null;
    walletPassword?: string | null;
}

export type PrivacyDefault = 'ask' | 'stealth' | 'public';

export type SealSyncStatus = 'Synced' | 'PendingUpload';

export interface SealUploadData {
    sealBytes: number[];
    sealHash: string;
}

export type AnchorSyncStatus =
    | 'Synchronized'
    | 'Offline'
    | 'RollbackDetected'
    | 'ForkConflict'
    | 'PendingLeapLock';

export interface RollbackReport {
    quarantinedVoucherIds?: string[];
    archivedVoucherIds?: string[];
    retainedVoucherIds?: string[];
    newTxNonce?: number;
    consolidatedStateHash?: string;
    quarantined_voucher_ids?: string[];
    archived_voucher_ids?: string[];
    retained_voucher_ids?: string[];
    new_tx_nonce?: number;
    consolidated_state_hash?: string;
    message: string;
}

export interface CustomServerDto {
    url: string;
    name: string;
    authToken?: string;
    peerToken?: string;
    isActive: boolean;
}

export interface AppSettings {
    bundleRetentionDays: number;
    sessionTimeoutSeconds: number;
    lastUsedDirectory?: string;
    privacyDefault: PrivacyDefault;
    l2NodeUrls?: string[];
    activeNetwork?: string;
    bootstrapSeedUrl?: string | null;
    customServers?: CustomServerDto[];
    exclusiveCustomServers?: boolean;
    cachedPeersMainnet?: string[];
    cachedPeersTestnet?: string[];
    devModeUnlocked?: boolean;
}

export type IntegrityReport =
    | { type: "valid" }
    | { type: "missingItems", items: string[] }
    | { type: "manipulatedItems", items: string[] }
    | { type: "unknownItems", items: string[] }
    | { type: "integrityOutdated" }
    | { type: "invalidSignature" }
    | { type: "missingIntegrityRecord" };

export type AppState =
    | { view: "loading" }
    | { view: "needs_profile" }
    | { view: "needs_login" }
    | { view: "logged_in" }
    | { view: "recreate_profile" }
    | { view: "needs_recovery" }
    | { view: "settings" }
    | { view: "support" }
    | { view: "bug_report" }
    | { view: "create_voucher"; previousView?: AppState }
    | { view: "voucher_details"; voucherId: string; previousView?: AppState }
    | { view: "send_vouchers" }
    | { view: "receive_bundle" }
    | { view: "transaction_history" }
    | { view: "activities" }
    | { view: "transfer_success"; bundleData: number[]; recipientId: string; summary: string }
    | { view: "receive_success"; payload: ReceiveSuccessPayload }
    | { view: "address_book"; initialSearchQuery?: string; previousView?: AppState }
    | { view: "sign_request"; voucherData: VoucherDetails }
    | { view: "conflict_details"; proofId: string; previousView?: AppState }
    | { view: "conflict_list" }
    | { view: "wallet"; initialStatusFilter?: string; initialStandardFilter?: string }
    | { view: "concept"; previousView?: AppState };
