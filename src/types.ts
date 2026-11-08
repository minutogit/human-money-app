// src/types.ts

export * from './types/profile';
export * from './types/voucher';
export * from './types/transaction';
export * from './types/api';
export * from './types/wot';
export * from './types/market';

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

export interface BackupInspectionDto {
  profileName: string;
  userId: string;
  epoch: number;
  txNonce: number;
  sealTimestamp: string;
  blobCount: number;
  customStandardCount: number;
  protectionMode: string;
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

