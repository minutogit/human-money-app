import { invoke } from "@tauri-apps/api/core";
import { afterWalletMutation } from "../sync/syncBridge";
import { Contact } from "../types";

export const contactService = {
    getContacts: async () => {
        return await invoke<Contact[]>("get_contacts");
    },

    saveContact: async (contact: Contact, password?: string) => {
        const result = await invoke<void>("save_contact", { contact, password });
        afterWalletMutation(password, {
            syncMembers: true,
            scheduleMarket: (contact.trustLevel ?? 0) >= 1,
            contactsUpdated: true,
        });
        return result;
    },

    deleteContact: async (did: string, password?: string) => {
        const result = await invoke<void>("delete_contact", { did, password });
        afterWalletMutation(password, {
            syncMembers: true,
            contactsUpdated: true,
        });
        return result;
    }
};
