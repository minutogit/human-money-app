import { invoke } from "@tauri-apps/api/core";
import { PublicProfile, TrustStatus, FrontendQuickContact } from "../types";
import { marketService } from "./marketService";

export interface CreateProfileArgs {
    profileName: string;
    mnemonic: string;
    passphrase?: string;
    userPrefix?: string;
    password: string;
    localInstanceId: string;
    language: string;
    retainMnemonic?: boolean;
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
        const current = await marketService.getMyMarketProfile(password);
        await marketService.saveMyMarketProfile({
            profile: {
                ...current.profile,
                ...profile,
            },
            visibility: current.visibility,
            skills: current.skills,
            maxHops: current.maxHops,
            allowInquiries: current.allowInquiries,
        }, password);
    },

    getUserId: async () => {
        return await invoke<string>("get_user_id");
    },

    getMyQuickContact: async (): Promise<FrontendQuickContact> => {
        return await invoke<FrontendQuickContact>("get_my_quick_contact");
    },

    encodeTextQrSvg: async (text: string): Promise<string> => {
        return await invoke<string>("encode_text_qr_svg", { text });
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
            language: args.language,
            retainMnemonic: args.retainMnemonic ?? true,
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



