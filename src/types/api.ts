// src/types/api.ts

import { PendingReceiveCommit, ReceiveSuccessPayload, IntentDispatchObservation } from './transaction';
import { VoucherDetails } from './voucher';
import { FrontendPostDepositResult, FrontendSyncDescriptor, StoredPublicSync } from './market';
import type { HelpTopicId } from '../help/helpTopics';

export type PrivacyDefault = 'ask' | 'stealth' | 'public';

export type UiMode = 'simple' | 'expert';

export type SettingsSection =
    | 'sync'
    | 'backup'
    | 'network'
    | 'profile'
    | 'standards'
    | 'security'
    | 'connection'
    | 'advanced';

export interface LockServerItemDto {
    writeUrl: string;
    readUrl: string;
    adminPub: string;
    lockPub: string;
    source: 'own' | 'circle' | 'public' | 'cache' | string;
    status: 'ok' | 'unreachable' | 'no_lock' | 'unverified' | string;
    trusted: boolean;
    allowPowTickets: boolean;
    maxRetentionQuarter: number;
    probedAtSecs: number;
    error?: string | null;
}

export interface LockProtectionOverviewDto {
    status: 'ready' | 'degraded' | 'none' | 'syncing' | string;
    readyTrustedCount: number;
    servers: LockServerItemDto[];
    hasOfflineCachedAnchor: boolean;
    unresolvedIntentsCount: number;
    pendingRestoreReconciliation: boolean;
}

export interface GuarantorServerTrustDto {
    writeUrl: string;
    adminPub: string;
    lockPub: string;
    trusted: boolean;
}

export interface GuarantorAnchorCheckDto {
    required: boolean;
    passed: boolean;
    hasLayer2Anchor: boolean;
    retentionOk?: boolean | null;
    sybilIndependentCount: number;
    threshold: number;
    serverCount: number;
    untrustedServersCount: number;
    servers: GuarantorServerTrustDto[];
    issues: string[];
}

export interface AnchorLifecycleDto {
    anchorId: string;
    revision: number;
    reducedGuarantorProtection: boolean;
    unhealthyServers: string[];
    revokedServers: string[];
    activeRotationPhase?: string | null;
    effectiveAtSecs?: number | null;
    lastCheckedSecs: number;
    lastError?: string | null;
}

export interface L2LifecycleStatusDto {
    lastTickSecs: number;
    trackedAnchorsCount: number;
    guarantorRolesCount: number;
    serverOffendersCount: number;
    autoVetoesSent: number;
    lastVetoReason?: string | null;
    anchors: AnchorLifecycleDto[];
}

export interface AnchorDiscoveryDto {
    anchorId: string;
    revision: number;
    threshold: number;
    serverCount: number;
    reducedGuarantorProtection: boolean;
    manifestJson: string;
}

export interface ClaimBundleDto {
    bundleData: number[];
    bundleId: string;
    guarantorId: string;
    claimedAmount: string;
    postDeposit?: FrontendPostDepositResult | null;
    postDepositError?: string | null;
}

export interface UiPreferences {
    uiMode: UiMode;
    lastUsedDirectory?: string | null;
    balanceViewMode: 'spendable' | 'net';
    marketShowUnknown: boolean;
    maxStoredCards: number;
    marketSendCardOnPayment: boolean;
    marketSyncEnabled: boolean;
    shareMinLevel: number;
    marketPollWifiOnly: boolean;
    wotShareViaSync: boolean;
    postAcceptFromContacts: boolean;
    mediaAutoLoadMaxHops: number;
}

export interface SecurityPolicy {
    sessionTimeoutSeconds: number;
    privacyDefault: PrivacyDefault;
    devModeUnlocked: boolean;
    autoBackupEnabled: boolean;
    syncBackupEnabled: boolean;
    backupSyncFolder?: string | null;
    backupProtectionMode?: string | null;
    lockTrustByAdminPub?: Record<string, boolean>;
    marketSelectedSyncs: FrontendSyncDescriptor[];
    publicSyncs: StoredPublicSync[];
}

export type WalletSyncAlertKind =
    | 'stale_local_backup'
    | 'fork_detected'
    | 'server_rollback'
    | 'unexpected_data_loss';

export interface WalletSyncAlert {
    kind: WalletSyncAlertKind | string;
    mirrorWriteUrl: string;
    remoteEpoch?: number | null;
    remoteTxNonce?: number | null;
    localEpoch: number;
    localTxNonce: number;
    detectedAt: string;
    hasSignedAckProof: boolean;
}

export interface InternalLifecycleState {
    pendingRestoreL2Reconciliation: boolean;
    backupDirty: boolean;
    lastBackupAt?: string | null;
    lastSyncBackupAt?: string | null;
    walletSyncAlert?: WalletSyncAlert | null;
}

export interface AppConfigDto {
    uiPreferences: UiPreferences;
    securityPolicy: SecurityPolicy;
    lifecycle: InternalLifecycleState;
}

export interface UiPreferencesPatch {
    uiMode?: UiMode;
    lastUsedDirectory?: string | null;
    balanceViewMode?: 'spendable' | 'net';
    marketShowUnknown?: boolean;
    maxStoredCards?: number;
    marketSendCardOnPayment?: boolean;
    marketSyncEnabled?: boolean;
    shareMinLevel?: number;
    marketPollWifiOnly?: boolean;
    wotShareViaSync?: boolean;
    postAcceptFromContacts?: boolean;
    mediaAutoLoadMaxHops?: number;
}

export interface SecurityPolicyPatch {
    sessionTimeoutSeconds?: number;
    privacyDefault?: PrivacyDefault;
    devModeUnlocked?: boolean;
    autoBackupEnabled?: boolean;
    syncBackupEnabled?: boolean;
    backupSyncFolder?: string | null;
    backupProtectionMode?: string | null;
}

export type ReachPresetId = 'near' | 'medium' | 'far';

export interface ReachPresetDto {
    preset: ReachPresetId | 'custom';
    ownListMaxHops: number;
    cardMaxHops: number;
    shareMinLevel: number;
    mediaAutoLoadMaxHops: number;
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
    | {
          view: "restore_backup";
          previousView?: AppState;
          initialPayload?: string;
          initialBackupPath?: string;
          initialMethod?: 'social' | 'words_or_sheet' | 'file_or_folder';
      }
    | { view: "needs_recovery"; initialPayload?: string }
    | { view: "settings"; initialSection?: SettingsSection }
    | { view: "support" }
    | { view: "bug_report" }
    | { view: "create_voucher"; previousView?: AppState }
    | { view: "voucher_details"; voucherId: string; previousView?: AppState }
    | { view: "send_vouchers"; initialRecipientId?: string; initialVoucherId?: string; initialNote?: string; initialPaymentUri?: string }
    | { view: "receive_bundle"; droppedFile?: { name: string; bytes: number[]; launchItem?: string } }
    | { view: "transaction_history" }
    | { view: "transfer_success"; bundleData: number[]; recipientId: string; summary: string; l2Dispatch?: IntentDispatchObservation[]; bundleId?: string; depositedTo?: number; recipientDepositedTo?: number; depositError?: string }
    | { view: "receive_success"; payload: ReceiveSuccessPayload; pendingCommit?: PendingReceiveCommit; launchItem?: string }
    | { view: "address_book"; initialSearchQuery?: string; initialOpenAddModal?: boolean; initialContactPayload?: string; previousView?: AppState }
    | { view: "sign_request"; voucherData: VoucherDetails; requestMsgId?: string; senderDid?: string; anchorManifestJson?: string }
    | { view: "conflict_details"; proofId: string; previousView?: AppState }
    | { view: "conflict_list" }
    | { view: "wallet"; initialStatusFilter?: string; initialStandardFilter?: string; initialFilter?: 'all' | 'received' | 'own' | 'expiringSoon' }
    | { view: "marketplace"; selectedDid?: string; initialTab?: 'board' | 'people' | 'mine'; initialOpenEditor?: boolean }
    | { view: "help_center"; topic?: HelpTopicId; previousView?: AppState };
