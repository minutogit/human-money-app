import type { CheckIntegrityOptions } from '../../hooks/useIntegrityCheck';
import type { SessionAccessMode } from '../../services/authService';
import type { IntegrityReport } from '../../types';

export type SecurityLockdownKind = 'handover' | 'clone' | 'fork' | null;

export interface SecurityLockdownDetails {
    targetEpoch?: number;
    isSeedRecovery?: boolean;
}

export interface ProtectActionOptions {
    requireFullAccess?: boolean;
}

export interface SessionContextType {
    protectAction: <T>(
        action: (password: string | null) => Promise<T>,
        options?: ProtectActionOptions,
    ) => Promise<T | void>;
    requireFullAccess: <T>(action: (masterPassword: string) => Promise<T>) => Promise<T | void>;
    elevateSession: (masterPassword: string) => Promise<void>;
    accessMode: SessionAccessMode;
    baseAccessMode: SessionAccessMode;
    hasCachedSession: boolean;
    isAuthenticated: boolean;
    profileName: string;
    displayProfileName?: string;
    userAvatarUrl?: string | null;
    folderName?: string;
    setProfileName: (name: string) => void;
    setFolderName?: (folder: string) => void;
    notifyLogin: (name?: string, folder?: string) => void;
    notifyLogout: () => void;
    isForkLocked: boolean;
    lockdownKind: SecurityLockdownKind;
    lockdownDetails: SecurityLockdownDetails | null;
    triggerSecurityLockdown: (kind: SecurityLockdownKind, details?: SecurityLockdownDetails) => void;
    isRecoveryRequired: boolean;
    integrityReport: IntegrityReport | null;
    checkIntegrity: (options?: CheckIntegrityOptions) => Promise<void>;
    clearLocks: () => void;
    lockSession: () => Promise<void>;
    sessionTimeoutSeconds?: number | null;
    refreshSessionStatus?: () => Promise<void>;
}

export interface PendingModalRequest {
    id: number;
    mode: 'session' | 'stepUp';
    credentialKind: 'password' | 'pin';
    resolve: (value: unknown) => void;
    reject: (reason: unknown) => void;
    action: (password: string | null) => Promise<unknown>;
    retriedExpired?: boolean;
}
