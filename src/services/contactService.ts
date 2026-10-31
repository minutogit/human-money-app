import { invoke } from "@tauri-apps/api/core";
import { triggerAutoBackup } from "../utils/autoBackup";
import { Contact } from "../types";

export const contactService = {
    getContacts: async () => {
        return await invoke<Contact[]>("get_contacts");
    },

    saveContact: async (contact: Contact, password?: string) => {
        const result = await invoke<void>("save_contact", { contact, password });
        triggerAutoBackup();
        return result;
    },

    deleteContact: async (did: string, password?: string) => {
        const result = await invoke<void>("delete_contact", { did, password });
        triggerAutoBackup();
        return result;
    }
};
