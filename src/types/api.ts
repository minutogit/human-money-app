// src/types/api.ts

import { ReceiveSuccessPayload, IntentDispatchObservation } from './transaction';
import { VoucherDetails } from './voucher';
import { FrontendSyncDescriptor, StoredPublicSync } from './market';
import type { HelpTopicId } from '../help/helpTopics';

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

export type UiMode = 'simple' | 'expert';

export type SealSyncStatus = 'Synced' | 'PendingUpload';

export interface SealUploadData {
    sealBytes: number[];
    sealHash: string;
}

export type AnchorSyncStatus =
    | 'Synchronized'
    | 'Offline'
    | 'NoServers'
    | 'PendingLeapLock'
    | 'Inconclusive'
    | 'SameDeviceRollback'
    | 'SupersededByHandover'
    | 'ConcurrentCloneDetected'
    | 'RollbackDetected'
    | 'ForkConflict';

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

export interface RegionalSourceDto {
    url: string;
    publicKey: string;
    bearerToken?: string | null;
}

export interface AppSettings {
    sessionTimeoutSeconds: number;
    lastUsedDirectory?: string;
    privacyDefault: PrivacyDefault;
    activeNetwork?: string;
    bootstrapSeedUrl?: string | null;
    customServers?: CustomServerDto[];
    exclusiveCustomServers?: boolean;
    cachedPeersMainnet?: string[];
    cachedPeersTestnet?: string[];
    devModeUnlocked?: boolean;
    regionalMeshSources?: RegionalSourceDto[];
    enablePosMode?: boolean;
    marketShowUnknown?: boolean;
    maxStoredCards?: number;
    marketSendCardOnPayment?: boolean;
    marketSyncEnabled?: boolean;
    shareMinLevel?: number;
    marketPollWifiOnly?: boolean;
    marketSelectedSyncs?: FrontendSyncDescriptor[];
    publicSyncs?: StoredPublicSync[];
    wotShareViaSync?: boolean;
    syncBackupEnabled?: boolean;
    lastSyncBackupAt?: string | null;
    pendingRestoreL2Reconciliation?: boolean;
    balanceViewMode?: 'spendable' | 'net';
    autoBackupEnabled?: boolean;
    backupFrequency?: 'after_mutation' | 'on_close' | 'daily' | 'manual';
    backupDirty?: boolean;
    backupSyncFolder?: string | null;
    backupProtectionMode?: string | null;
    lastBackupAt?: string | null;
    postAcceptFromContacts?: boolean;
    uiMode?: UiMode;
}

export type AutoBackupStatus =
    | 'ok'
    | 'pending'
    | 'warning'
    | 'disconnected'
    | 'notConfigured'
    | 'synced'
    | 'folderDisconnected'
    | 'outdated'
    | 'manualOnly'
    | 'locked';

export interface FrontendAutoBackupStatusDto {
    status: AutoBackupStatus;
    lastBackupAt?: string | null;
    backupDirty: boolean;
    syncFolder?: string | null;
    changedBlobs?: number | null;
    syncBackupEnabled: boolean;
    lastSyncBackupAt?: string | null;
    hasAuthorizedSyncBackupTarget: boolean;
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
    | { view: "restore_backup"; previousView?: AppState; initialPayload?: string }
    | { view: "needs_recovery" }
    | { view: "settings"; initialSection?: 'sync' | 'backup' | 'network' | 'profile' | 'standards' | 'security' | 'connection' | 'advanced' }
    | { view: "support" }
    | { view: "bug_report" }
    | { view: "create_voucher"; previousView?: AppState }
    | { view: "voucher_details"; voucherId: string; previousView?: AppState }
    | { view: "send_vouchers"; initialRecipientId?: string; initialVoucherId?: string; initialNote?: string }
    | { view: "receive_bundle"; droppedFile?: { name: string; bytes: number[] } }
    | { view: "transaction_history" }
    | { view: "activities"; previousView?: AppState }
    | { view: "transfer_success"; bundleData: number[]; recipientId: string; summary: string; l2Dispatch?: IntentDispatchObservation[]; bundleId?: string; depositedTo?: number; depositError?: string }
    | { view: "receive_success"; payload: ReceiveSuccessPayload }
    | { view: "address_book"; initialSearchQuery?: string; initialOpenAddModal?: boolean; previousView?: AppState }
    | { view: "sign_request"; voucherData: VoucherDetails; requestMsgId?: string; senderDid?: string }
    | { view: "conflict_details"; proofId: string; previousView?: AppState }
    | { view: "conflict_list" }
    | { view: "wallet"; initialStatusFilter?: string; initialStandardFilter?: string; initialFilter?: 'all' | 'received' | 'own' | 'expiringSoon' }
    | { view: "marketplace"; selectedDid?: string; initialTab?: 'board' | 'people' | 'mine'; initialOpenEditor?: boolean }
    | { view: "help_center"; topic?: HelpTopicId; previousView?: AppState };
