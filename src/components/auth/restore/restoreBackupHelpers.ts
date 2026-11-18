import type { TFunction } from 'i18next';
import {
    BackupInspectionDto,
    LocalProfileCollisionDto,
} from '../../../types';
import { SeedPhraseValue } from '../SeedPhraseInput';

export type TopMethod = 'choose' | 'social' | 'words_or_sheet' | 'file_or_folder' | 'completed';
export type WordsSubPhase =
    | 'input'
    | 'direct_setup'
    | 'sync_found'
    | 'sync_not_found'
    | 'sync_unreachable';

export interface RestoreFeedback {
    type: 'error' | 'success';
    message: string;
}

export const EMPTY_SEED_STATE: SeedPhraseValue = {
    mnemonic: '',
    wordCount: 12,
    language: 'english',
    passphrase: '',
    userPrefix: '',
    isValid: false,
    detectedKit: null,
};

export function resolveDefaultCollisionChoice(
    coll: LocalProfileCollisionDto,
    insp: BackupInspectionDto
): 'reset_pw' | 'replace' {
    const localEpoch = coll.localEpoch ?? 0;
    const localNonce = coll.localTxNonce ?? 0;
    const localNewerOrEqual =
        localEpoch > insp.epoch || (localEpoch === insp.epoch && localNonce >= insp.txNonce);
    return localNewerOrEqual && !coll.isLockedOrConflicted ? 'reset_pw' : 'replace';
}

export function parseEffectiveSyncInputs(
    syncInput: string,
    syncEdited: boolean,
    detectedKit: SeedPhraseValue['detectedKit']
): string[] {
    const trimmedSync = syncInput.trim();
    if (!trimmedSync) return [];
    if (
        !syncEdited &&
        detectedKit &&
        detectedKit.syncDescriptorsJson.length > 0 &&
        trimmedSync === detectedKit.syncUrls.join(', ')
    ) {
        return detectedKit.syncDescriptorsJson;
    }
    if (trimmedSync.startsWith('{')) {
        return [trimmedSync];
    }
    return trimmedSync
        .split(/[\s,;\n]+/)
        .map((s) => s.trim())
        .filter(Boolean);
}

export function validateDevicePasswordPair(
    newPassword: string,
    confirmNewPassword: string,
    t: TFunction
): string | null {
    if (newPassword.length < 8) {
        return t('profile.passwordMinLength');
    }
    if (newPassword !== confirmNewPassword) {
        return t('profile.passwordMismatch');
    }
    return null;
}

export function buildBackupCredentialArgs(
    credentialMode: 'password' | 'mnemonic',
    backupPassword: string,
    seed: SeedPhraseValue,
    includeUserPrefix = true
) {
    return {
        credentialMode,
        backupPassword: credentialMode === 'password' ? backupPassword : null,
        mnemonic: credentialMode === 'mnemonic' ? seed.mnemonic : null,
        passphrase: credentialMode === 'mnemonic' && seed.passphrase ? seed.passphrase : null,
        ...(includeUserPrefix
            ? {
                  userPrefix:
                      credentialMode === 'mnemonic' && seed.userPrefix.trim()
                          ? seed.userPrefix.trim()
                          : null,
              }
            : {}),
        language: credentialMode === 'mnemonic' ? seed.language : null,
    };
}
