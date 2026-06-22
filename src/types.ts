// src/types.ts

export * from './types/profile';
export * from './types/voucher';
export * from './types/transaction';
export * from './types/api';

/** Structured error DTO from the Tauri backend */
export interface BackendError {
  code: string;
  message: string;
  details: Record<string, string>;
}
