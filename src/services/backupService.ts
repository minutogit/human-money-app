// src/services/backupService.ts
import { invoke } from "@tauri-apps/api/core";
import {
    BackupProbeDto,
    BackupSummaryDto,
    BackupInspectionDto,
    RestoreOutcomeDto,
    MnemonicLanguage,
    FrontendAutoBackupStatusDto,
} from "../types";

export interface ExportBackupFileArgs {
    destinationPath: string;
    protectionMode: "mnemonic_only" | "custom_password" | string;
    backupPassword?: string | null;
    password?: string | null;
}

export interface SyncBackupFolderArgs {
    folderPath: string;
    protectionMode: "mnemonic_only" | "custom_password" | string;
    backupPassword?: string | null;
    password?: string | null;
    forceFull?: boolean;
}

export interface InspectBackupArgs {
    sourcePath: string;
    credentialMode: "password" | "mnemonic";
    backupPassword?: string | null;
    mnemonic?: string | null;
    passphrase?: string | null;
    userPrefix?: string | null;
    language?: MnemonicLanguage | null;
}

export interface RestoreBackupArgs {
    sourcePath: string;
    credentialMode: "password" | "mnemonic";
    backupPassword?: string | null;
    mnemonic?: string | null;
    passphrase?: string | null;
    userPrefix?: string | null;
    language?: MnemonicLanguage | null;
    newPassword: string;
    localInstanceId: string;
}

export const backupService = {
    probeBackup: async (sourcePath: string): Promise<BackupProbeDto> => {
        return await invoke<BackupProbeDto>("probe_wallet_backup", { sourcePath });
    },

    exportBackupFile: async (args: ExportBackupFileArgs): Promise<BackupSummaryDto> => {
        return await invoke<BackupSummaryDto>("export_wallet_backup_file", {
            destinationPath: args.destinationPath,
            protectionMode: args.protectionMode,
            backupPassword: args.backupPassword ?? null,
            password: args.password ?? null,
        });
    },

    syncBackupFolder: async (args: SyncBackupFolderArgs): Promise<BackupSummaryDto> => {
        return await invoke<BackupSummaryDto>("sync_wallet_backup_folder", {
            folderPath: args.folderPath,
            protectionMode: args.protectionMode,
            backupPassword: args.backupPassword ?? null,
            password: args.password ?? null,
            forceFull: args.forceFull ?? false,
        });
    },

    inspectBackup: async (args: InspectBackupArgs): Promise<BackupInspectionDto> => {
        return await invoke<BackupInspectionDto>("inspect_wallet_backup", {
            sourcePath: args.sourcePath,
            credentialMode: args.credentialMode,
            backupPassword: args.backupPassword ?? null,
            mnemonic: args.mnemonic ?? null,
            passphrase: args.passphrase ?? null,
            userPrefix: args.userPrefix ?? null,
            language: args.language ?? null,
        });
    },

    restoreBackup: async (args: RestoreBackupArgs): Promise<RestoreOutcomeDto> => {
        return await invoke<RestoreOutcomeDto>("restore_wallet_backup", {
            sourcePath: args.sourcePath,
            credentialMode: args.credentialMode,
            backupPassword: args.backupPassword ?? null,
            mnemonic: args.mnemonic ?? null,
            passphrase: args.passphrase ?? null,
            userPrefix: args.userPrefix ?? null,
            language: args.language ?? null,
            newPassword: args.newPassword,
            localInstanceId: args.localInstanceId,
        });
    },

    getBackupStatus: async (): Promise<FrontendAutoBackupStatusDto> => {
        return await invoke<FrontendAutoBackupStatusDto>("get_backup_status");
    },

    runAutoBackupSync: async (): Promise<FrontendAutoBackupStatusDto> => {
        return await invoke<FrontendAutoBackupStatusDto>("run_auto_backup_sync");
    },
};
