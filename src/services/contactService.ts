import { invoke } from "@tauri-apps/api/core";
import { triggerAutoBackup } from "../utils/autoBackup";
import { channelMembersSyncManager } from "./channelMembersSyncManager";
import { Contact } from "../types";

export function triggerSyncMembersUpdate(delayMs: number = 250, password?: string): void {
    channelMembersSyncManager.trigger({ delayMs, resetQuota: true, password });
}

export const contactService = {
    getContacts: async () => {
        return await invoke<Contact[]>("get_contacts");
    },

    saveContact: async (contact: Contact, password?: string) => {
        const result = await invoke<void>("save_contact", { contact, password });
        triggerAutoBackup();
        triggerSyncMembersUpdate(250, password);
        return result;
    },

    deleteContact: async (did: string, password?: string) => {
        const result = await invoke<void>("delete_contact", { did, password });
        triggerAutoBackup();
        triggerSyncMembersUpdate(250, password);
        return result;
    }
};
