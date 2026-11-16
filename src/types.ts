// src/types.ts

export * from './types/profile';
export * from './types/voucher';
export * from './types/transaction';
export * from './types/api';
export * from './types/wot';
export * from './types/market';
export * from './types/recovery';

/** Structured error DTO from the Tauri backend */
export interface BackendError {
  code: string;
  message: string;
  details: Record<string, string>;
}

export interface BackupProbeDto {
  sourceKind: 'file' | 'folder';
  protectionMode: string;
  supportsPasswordRestore: boolean;
  supportsMnemonicRestore: boolean;
  blobCount: number;
  customStandardCount: number;
}

export interface LocalProfileCollisionDto {
  folderName: string;
  profileName: string;
  userId: string;
  localEpoch: number | null;
  localTxNonce: number | null;
  isLockedOrConflicted: boolean;
}

export interface ResolvedSyncTargetDto {
  writeUrl: string;
  readUrl: string;
  dropPub: string;
  registerUrl: string | null;
}

export interface MnemonicIdentityPreviewDto {
  userId: string;
  localCollision: LocalProfileCollisionDto | null;
}

export interface BackupInspectionDto {
  profileName: string;
  userId: string;
  epoch: number;
  txNonce: number;
  sealTimestamp: string;
  blobCount: number;
  customStandardCount: number;
  protectionMode: string;
  localCollision: LocalProfileCollisionDto | null;
}

export interface BackupSummaryDto {
  totalBlobs: number;
  changedBlobs: number;
  deletedBlobs: number;
  uploadBytes: number;
  timestamp: string;
  destinationPath: string;
  customStandardCount: number;
}

export interface RestoreOutcomeDto {
  folderName: string;
  profileName: string;
  activeVoucherQueriesCount: number;
  reconciledSpentCount: number;
  restoredStandardsCount: number;
  l2ReconciliationStatus: 'no_servers' | 'offline' | 'reconciled' | string;
}

export interface FrontendBackupSyncResult {
  uploadedTo: number;
  authorizedTargets: number;
  paddedBytes: number;
  timestamp?: string | null;
  errors: string[];
  retryAfterSecs?: number | null;
}

export interface SyncBackupProbeResultDto {
  status: 'found' | 'not_found' | 'unreachable';
  stagedPath: string | null;
  matchedKeyKind: 'stealth' | 'public' | 'grace' | 'slot_id' | 'own' | string | null;
  sourceUrl: string | null;
  inspection: BackupInspectionDto | null;
  userId: string;
  localCollision: LocalProfileCollisionDto | null;
  resolvedSyncTargets: ResolvedSyncTargetDto[];
  error: string | null;
}

export interface FrontendRestoreReconcileResultDto {
  activeVoucherQueriesCount: number;
  reconciledSpentCount: number;
  l2ReconciliationStatus: 'no_servers' | 'offline' | 'reconciled' | string;
  pendingRestoreL2Reconciliation: boolean;
}


