import { invoke } from "@tauri-apps/api/core";
import { PublicProfile, TrustStatus } from "../types";

export interface CreateProfileArgs {
    profileName: string;
    mnemonic: string;
    passphrase?: string;
    userPrefix?: string;
    password: string;
    localInstanceId: string;
    language: string;
}

export interface RecoverWalletArgs {
    folderName: string;
    mnemonic: string;
    passphrase?: string;
    newPassword: string;
    localInstanceId: string;
    language: string;
}

export const profileService = {
    getProfile: async () => {
        return await invoke<PublicProfile>("get_user_profile");
    },

    saveProfile: async (profile: PublicProfile, password?: string) => {
        return await invoke<void>("save_user_profile", { profile, password });
    },

    getIdentity: async () => {
        return await invoke<string>("get_user_id");
    },

    generateMnemonic: async (wordCount: number, language: string) => {
        return await invoke<string>("generate_mnemonic", { wordCount, language });
    },

    createProfile: async (args: CreateProfileArgs) => {
        return await invoke<void>("create_profile", {
            profileName: args.profileName,
            mnemonic: args.mnemonic,
            passphrase: args.passphrase,
            userPrefix: args.userPrefix,
            password: args.password,
            localInstanceId: args.localInstanceId,
            language: args.language
        });
    },

    getWordlist: async (language: string) => {
        return await invoke<string[]>("get_bip39_wordlist", { language });
    },

    validateMnemonic: async (mnemonic: string, language: string) => {
        return await invoke<void>("validate_mnemonic", { mnemonic, language });
    },

    recoverWallet: async (args: RecoverWalletArgs) => {
        return await invoke<void>("recover_wallet_and_set_new_password", {
            folderName: args.folderName,
            mnemonic: args.mnemonic,
            passphrase: args.passphrase,
            newPassword: args.newPassword,
            localInstanceId: args.localInstanceId,
            language: args.language
        });
    },

    checkReputation: async (offenderId: string) => {
        return await invoke<TrustStatus>("check_reputation", { offenderId });
    },

    logout: async () => {
        return await invoke<void>("logout");
    }
};



