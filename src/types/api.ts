// src/types/api.ts

import { ReceiveSuccessPayload, IntentDispatchObservation } from './transaction';
import { VoucherDetails } from './voucher';
import { FrontendSyncDescriptor, StoredPublicSync } from './market';
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

export interface LockServerProbeResult {
    writeUrl: string;
    readUrl: string;
    dropPub: string;
    adminPub: string;
    lockPub: string;
    delegationValid: boolean;
    wafProbePassed: boolean;
    selftestPassed: boolean;
    keyLeakSafe: boolean;
    passed: boolean;
    latencyMs: number;
    maxRetentionQuarter: number;
    powBaseBits: number;
    allowPowTickets: boolean;
    inodeWarning: boolean;
    usedInodes: number;
    maxInodes: number;
    probedAtSecs: number;
    error?: string | null;
}

export interface StoredAnchorServerMeta {
    writeUrl: string;
    readUrl: string;
    dropPub: string;
    adminPub: string;
    lockPub: string;
    nominatedBy: string;
    operatorDomain?: string | null;
    asn?: string | null;
    infoFetchedAtSecs: number;
    maxRetentionQuarter: number;
    powBaseBits: number;
}

export interface StoredAnchorTemplateMeta {
    anchorId: string;
    label: string;
    threshold: number;
    retainUntilQuarter: number;
    createdAtSecs: number;
    servers: StoredAnchorServerMeta[];
}

export interface NominatedLockServerMeta {
    writeUrl: string;
    readUrl: string;
    dropPub: string;
    adminPub: string;
    lockPub: string;
    asn?: string | null;
    probedAtSecs: number;
}

export interface AnchorServerInputDto {
    writeUrl: string;
    nominatedBy: 'creator' | 'guarantor';
    guarantorIndex?: number | null;
    asn?: string | null;
}

export interface L2LockStatusDto {
    status: 'Ok' | 'Syncing' | 'NoServers' | 'Degraded';
    unresolvedIntentsCount: number;
    pendingRestoreReconciliation: boolean;
    templatesCount: number;
    hasOwnNominatedServer: boolean;
    staleServersCount: number;
    maxProbeAgeSecs?: number | null;
}

export interface GuarantorAnchorCheckDto {
    required: boolean;
    passed: boolean;
    hasLayer2Anchor: boolean;
    retentionOk?: boolean | null;
    ownServerPresent?: boolean | null;
    sybilIndependentCount: number;
    threshold: number;
    serverCount: number;
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
}

export interface ApprovedSwapClaimDto {
    claimantDid: string;
    voucherId: string;
    tipTId: string;
    approvedAmount: string;
}

export interface RejectedSwapClaimDto {
    claimantDid: string;
    voucherId: string;
    tipTId: string;
    reason: string;
}

export interface DeadQuorumAdjudicationDto {
    approved: ApprovedSwapClaimDto[];
    rejected: RejectedSwapClaimDto[];
    doubleSpendProofsCount: number;
}

export interface AppSettings {
    sessionTimeoutSeconds: number;
    lastUsedDirectory?: string;
    privacyDefault: PrivacyDefault;
    devModeUnlocked?: boolean;
    l2AnchorTemplatesMeta?: StoredAnchorTemplateMeta[];
    ownNominatedLockServer?: NominatedLockServerMeta | null;
    l2ProbeCache?: Record<string, LockServerProbeResult>;
    enablePosMode?: boolean;
    posCashierActive?: boolean;
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
    backupDirty?: boolean;
    backupSyncFolder?: string | null;
    backupProtectionMode?: string | null;
    lastBackupAt?: string | null;
    postAcceptFromContacts?: boolean;
    uiMode?: UiMode;
    mediaAutoLoadMaxHops?: number;
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
    | { view: "receive_bundle"; droppedFile?: { name: string; bytes: number[] } }
    | { view: "transaction_history" }
    | { view: "transfer_success"; bundleData: number[]; recipientId: string; summary: string; l2Dispatch?: IntentDispatchObservation[]; bundleId?: string; depositedTo?: number; recipientDepositedTo?: number; depositError?: string }
    | { view: "receive_success"; payload: ReceiveSuccessPayload }
    | { view: "address_book"; initialSearchQuery?: string; initialOpenAddModal?: boolean; initialContactPayload?: string; previousView?: AppState }
    | { view: "sign_request"; voucherData: VoucherDetails; requestMsgId?: string; senderDid?: string; anchorManifestJson?: string }
    | { view: "conflict_details"; proofId: string; previousView?: AppState }
    | { view: "conflict_list" }
    | { view: "wallet"; initialStatusFilter?: string; initialStandardFilter?: string; initialFilter?: 'all' | 'received' | 'own' | 'expiringSoon' }
    | { view: "marketplace"; selectedDid?: string; initialTab?: 'board' | 'people' | 'mine'; initialOpenEditor?: boolean }
    | { view: "help_center"; topic?: HelpTopicId; previousView?: AppState };
