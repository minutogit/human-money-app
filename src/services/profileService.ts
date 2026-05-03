import { invoke } from "@tauri-apps/api/core";
import { PublicProfile, TrustStatus } from "../types";

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

    createProfile: async (args: any) => {
        return await invoke<void>("create_profile", args);
    },

    getWordlist: async (language: string) => {
        return await invoke<string[]>("get_bip39_wordlist", { language });
    },

    validateMnemonic: async (mnemonic: string, language: string) => {
        return await invoke<void>("validate_mnemonic", { mnemonic, language });
    },

    recoverWallet: async (args: any) => {
        return await invoke<void>("recover_wallet_and_set_new_password", args);
    },

    checkReputation: async (offenderId: string) => {
        return await invoke<TrustStatus>("check_reputation", { offenderId });
    },

    logout: async () => {
        return await invoke<void>("logout");
    }
};



