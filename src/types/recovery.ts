export type GuardianAssignmentStatus =
    | { status: 'pending' }
    | { status: 'confirmed'; ackedAt: number };

export interface GuardianAssignmentRecord {
    userId: string;
    displayName: string;
    shareIndex: number;
    status: GuardianAssignmentStatus;
    assignedAt: number;
    sealedEntry?: unknown;
}

export interface RecoveryOwnerState {
    rotationEpoch: number;
    thresholdM: number;
    questionIds: [number, number, number];
    guardians: GuardianAssignmentRecord[];
    emergencySheetConfirmedAt?: number;
}

export interface ExportedMnemonicKitDto {
    phrase: string;
    languageCode: number;
    hasBip39Passphrase: boolean;
    kitUri: string;
    qrSvg: string;
    syncUrls: string[];
    guardianNames: string[];
}

export interface ConfirmEmergencySheetResultDto {
    confirmedAt: number;
    backupSyncResult: unknown | null;
}

export interface GuardianStatusDto {
    userId: string;
    displayName: string;
    shareIndex: number;
    status: GuardianAssignmentStatus;
    assignedAt: number;
    lastContactedAt: number | null;
    isStale90d: boolean;
    updatedAt: number;
}

export interface GuardianWardDto {
    ownerDid: string;
    ownerDisplayName: string;
    rotationEpoch: number;
    thresholdM: number;
    totalN: number;
    updatedAt: number;
}

export interface RecoveryProtectionStatusDto {
    hasVoucher: boolean;
    confirmedGuardiansCount: number;
    requiredThreshold: number;
    guardians: GuardianStatusDto[];
    emergencySheetConfirmedAt: number | null;
    hasIdentityRecovery: boolean;
    syncBackupCount: number;
    hasSyncBackup: boolean;
    hasLocalBackup: boolean;
    hasVoucherBackup: boolean;
    guardianWards: GuardianWardDto[];
    rehearsalDue?: boolean;
    questionIds?: [number, number, number] | null;
    rehearsalStage?: number;
    nextRehearsalAt?: number | null;
}

export interface RehearsalCheckResultDto {
    allCorrect: boolean;
    questionMatches: [boolean, boolean, boolean];
    rehearsalStage: number;
    nextRehearsalAt: number;
}

export interface SetupSocialRecoveryResultDto {
    rotationEpoch: number;
    thresholdM: number;
    guardiansCount: number;
    postSyncResult: unknown;
    backupSyncResult: unknown | null;
}

export interface RevokeSocialRecoveryResultDto {
    rotationEpoch: number;
    revokedGuardiansCount: number;
    postSyncResult: unknown;
    backupSyncResult: unknown | null;
}

import { LocalProfileCollisionDto, ResolvedSyncTargetDto, MnemonicLanguage } from '../types';

export interface ActiveRecoverySessionDto {
    sasCode: string;
    createdAt: number;
    nameHint: string | null;
    requestUri: string;
    sessionMode: 'restore_new_device' | 'reset_local_password' | string;
    targetFolderName: string | null;
    ownerDid: string | null;
    ownerDisplayName: string | null;
    rotationEpoch: number | null;
    thresholdM: number | null;
    totalN?: number | null;
    collectedCount: number;
    collectedShareIndices?: number[];
    questionIds: [number, number, number] | null;
    hasEnoughShares: boolean;
    discoveredSyncUrls: string[];
    lastImportStatus?: 'added' | 'duplicate' | string | null;
}

export interface GuardianRecoveryRequestInspectionDto {
    sasCode: string;
    requestedAt: number;
    isStale48h: boolean;
    nameHint: string | null;
    wards: GuardianWardDto[];
}

export interface ApproveGuardianRecoveryResultDto {
    replyUri: string;
    replyQrSvg: string;
    sasCode: string;
    ownerDisplayName: string;
}

export interface InspectedEmergencyKitDto {
    phrase: string;
    wordCount: number;
    language: MnemonicLanguage;
    hasBip39Passphrase: boolean;
    userPrefix: string | null;
    syncUrls: string[];
    syncDescriptorsJson: string[];
}

export interface RecoveryCompletionResultDto {
    folderName: string;
    profileName: string;
    mode: string;
    outcome: string;
    backupEpoch: number | null;
    backupTxNonce: number | null;
    activeVoucherQueriesCount: number;
    reconciledSpentCount: number;
    restoredStandardsCount: number;
    l2ReconciliationStatus: 'no_servers' | 'offline' | 'reconciled' | string;
    pendingRestoreL2Reconciliation: boolean;
    localCollision: LocalProfileCollisionDto | null;
    resolvedSyncTargets: ResolvedSyncTargetDto[];
}
