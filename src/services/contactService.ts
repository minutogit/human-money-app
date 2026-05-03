import { invoke } from "@tauri-apps/api/core";
import { Contact } from "../types";

export const contactService = {
    getContacts: async () => {
        return await invoke<Contact[]>("get_contacts");
    },

    saveContact: async (contact: Contact, password?: string) => {
        return await invoke<void>("save_contact", { contact, password });
    },

    deleteContact: async (did: string, password?: string) => {
        return await invoke<void>("delete_contact", { did, password });
    }
};
