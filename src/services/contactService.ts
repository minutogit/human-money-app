import { invoke } from "@tauri-apps/api/core";
import { triggerAutoBackup } from "../utils/autoBackup";
import { marketService } from "./marketService";
import { Contact } from "../types";

export function triggerDropMembersUpdate(): void {
    marketService.getDropAdminStatus()
        .then((status) => {
            if (status && status.descriptor && status.descriptor.writeUrl) {
                marketService.updateDropAdminConfig({}, true).catch(() => {});
            }
        })
        .catch(() => {});
}

export const contactService = {
    getContacts: async () => {
        return await invoke<Contact[]>("get_contacts");
    },

    saveContact: async (contact: Contact, password?: string) => {
        const result = await invoke<void>("save_contact", { contact, password });
        triggerAutoBackup();
        triggerDropMembersUpdate();
        return result;
    },

    deleteContact: async (did: string, password?: string) => {
        const result = await invoke<void>("delete_contact", { did, password });
        triggerAutoBackup();
        triggerDropMembersUpdate();
        return result;
    }
};
