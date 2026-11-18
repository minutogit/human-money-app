// src/types.ts

import type { AppState } from './types/api';

export * from './types/profile';
export * from './types/voucher';
export * from './types/transaction';
export * from './types/api';
export * from './types/wot';
export * from './types/market';
export * from './types/backup';
export * from './types/recovery';

/** Structured error DTO from the Tauri backend */
export interface BackendError {
  code: string;
  message: string;
  details: Record<string, string>;
}

/** Views accessible without an active authenticated session */
export const PUBLIC_VIEWS: ReadonlySet<AppState['view']> = new Set<AppState['view']>([
  'loading',
  'needs_profile',
  'needs_login',
  'restore_backup',
  'needs_recovery',
  'help_center',
]);

/** Returns true if the given view can be rendered without an active session. */
export function isPublicView(view: AppState['view']): boolean {
  return PUBLIC_VIEWS.has(view);
}
