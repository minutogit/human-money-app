import { invoke } from "@tauri-apps/api/core";
import { triggerAutoBackup } from "../utils/autoBackup";
import { triggerDebouncedChannelMembersSync } from "./marketService";
import { Contact } from "../types";

export function triggerSyncMembersUpdate(): void {
    triggerDebouncedChannelMembersSync(1500);
}

export const contactService = {
    getContacts: async () => {
        return await invoke<Contact[]>("get_contacts");
    },

    saveContact: async (contact: Contact, password?: string) => {
        const result = await invoke<void>("save_contact", { contact, password });
        triggerAutoBackup();
        triggerSyncMembersUpdate();
        return result;
    },

    deleteContact: async (did: string, password?: string) => {
        const result = await invoke<void>("delete_contact", { did, password });
        triggerAutoBackup();
        triggerSyncMembersUpdate();
        return result;
    }
};
