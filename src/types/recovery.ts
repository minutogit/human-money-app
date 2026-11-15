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

export interface ActiveRecoverySessionDto {
    sasCode: string;
    createdAt: number;
    nameHint: string | null;
    requestUri: string;
    ownerDid: string | null;
    ownerDisplayName: string | null;
    rotationEpoch: number | null;
    thresholdM: number | null;
    collectedCount: number;
    questionIds: [number, number, number] | null;
    hasEnoughShares: boolean;
    discoveredSyncUrls: string[];
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

export interface EmergencyKitRestoreInput {
    kitUri?: string;
    manualMnemonic?: string;
    manualLanguageCode?: number;
    manualPassphrase?: string;
    manualUserPrefix?: string;
    manualSyncUrls?: string[];
}

export interface RecoveryCompletionResultDto {
    profileName: string;
    mode: string;
    outcome: string;
    backupEpoch: number | null;
    backupTxNonce: number | null;
    pendingRestoreL2Reconciliation: boolean;
}
