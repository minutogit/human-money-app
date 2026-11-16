import { invoke } from '@tauri-apps/api/core';
import {
    ActiveRecoverySessionDto,
    ApproveGuardianRecoveryResultDto,
    ConfirmEmergencySheetResultDto,
    ExportedMnemonicKitDto,
    GuardianRecoveryRequestInspectionDto,
    InspectedEmergencyKitDto,
    RecoveryCompletionResultDto,
    RecoveryProtectionStatusDto,
    RevokeSocialRecoveryResultDto,
    SetupSocialRecoveryResultDto,
} from '../types/recovery';

export interface SetupSocialRecoveryParams {
    password: string;
    guardianDids: string[];
    thresholdM: number;
    questionIds: [number, number, number];
    answers: [string, string, string];
}

export interface CompleteSocialRecoveryParams {
    ownerDid: string;
    rotationEpoch: number;
    answers: [string, string, string];
    bip39Passphrase?: string;
    mode: 'restore_new_device' | 'reset_local_password';
    newPassword: string;
    profileName?: string;
    targetFolderName?: string;
    localInstanceId: string;
    allowEmptyFallback?: boolean;
    replaceFolderName?: string;
}

export const recoveryService = {
    exportProfileMnemonic: async (password: string): Promise<ExportedMnemonicKitDto | null> => {
        return await invoke<ExportedMnemonicKitDto | null>('export_profile_mnemonic', { password });
    },

    inspectEmergencyKitUri: async (uri: string): Promise<InspectedEmergencyKitDto> => {
        return await invoke<InspectedEmergencyKitDto>('inspect_emergency_kit_uri', { uri });
    },

    confirmEmergencySheetSaved: async (
        password: string,
        wordIndex: number,
        controlWord: string
    ): Promise<ConfirmEmergencySheetResultDto> => {
        return await invoke<ConfirmEmergencySheetResultDto>('confirm_emergency_sheet_saved', {
            password,
            wordIndex,
            controlWord,
        });
    },

    getRecoveryProtectionStatus: async (): Promise<RecoveryProtectionStatusDto> => {
        return await invoke<RecoveryProtectionStatusDto>('get_recovery_protection_status');
    },

    setupSocialRecoveryGuardians: async (
        params: SetupSocialRecoveryParams
    ): Promise<SetupSocialRecoveryResultDto> => {
        return await invoke<SetupSocialRecoveryResultDto>('setup_social_recovery_guardians', {
            password: params.password,
            guardianDids: params.guardianDids,
            thresholdM: params.thresholdM,
            questionIds: params.questionIds,
            answers: params.answers,
        });
    },

    revokeSocialRecovery: async (password?: string): Promise<RevokeSocialRecoveryResultDto> => {
        return await invoke<RevokeSocialRecoveryResultDto>('revoke_social_recovery', { password });
    },

    startSocialRecoverySession: async (
        nameHint?: string,
        sessionMode?: 'restore_new_device' | 'reset_local_password',
        targetFolderName?: string
    ): Promise<ActiveRecoverySessionDto> => {
        return await invoke<ActiveRecoverySessionDto>('start_social_recovery_session', {
            nameHint: nameHint ?? null,
            sessionMode: sessionMode ?? null,
            targetFolderName: targetFolderName ?? null,
        });
    },

    getActiveRecoverySession: async (): Promise<ActiveRecoverySessionDto | null> => {
        return await invoke<ActiveRecoverySessionDto | null>('get_active_recovery_session');
    },

    cancelSocialRecoverySession: async (): Promise<void> => {
        return await invoke<void>('cancel_social_recovery_session');
    },

    importGuardianRecoveryReply: async (replyUri: string): Promise<ActiveRecoverySessionDto> => {
        return await invoke<ActiveRecoverySessionDto>('import_guardian_recovery_reply', { replyUri });
    },

    inspectGuardianRecoveryRequest: async (
        requestUri: string,
        password?: string
    ): Promise<GuardianRecoveryRequestInspectionDto> => {
        return await invoke<GuardianRecoveryRequestInspectionDto>('inspect_guardian_recovery_request', {
            requestUri,
            password,
        });
    },

    approveGuardianRecoveryRequest: async (
        requestUri: string,
        ownerDid: string,
        password?: string
    ): Promise<ApproveGuardianRecoveryResultDto> => {
        return await invoke<ApproveGuardianRecoveryResultDto>('approve_guardian_recovery_request', {
            requestUri,
            ownerDid,
            password,
        });
    },

    completeSocialRecovery: async (
        params: CompleteSocialRecoveryParams
    ): Promise<RecoveryCompletionResultDto> => {
        return await invoke<RecoveryCompletionResultDto>('complete_social_recovery', {
            ownerDid: params.ownerDid,
            rotationEpoch: params.rotationEpoch,
            answers: params.answers,
            bip39Passphrase: params.bip39Passphrase,
            mode: params.mode,
            newPassword: params.newPassword,
            profileName: params.profileName,
            targetFolderName: params.targetFolderName,
            localInstanceId: params.localInstanceId,
            allowEmptyFallback: params.allowEmptyFallback ?? null,
            replaceFolderName: params.replaceFolderName ?? null,
        });
    },
};
