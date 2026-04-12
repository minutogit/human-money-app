// src/utils/settingsUtils.ts
import { invoke } from "@tauri-apps/api/core";
import { AppSettings } from "../types";
import { logger } from "./log";

/**
 * Extracts the directory from a full file path.
 */
export function getDirectoryFromPath(path: string): string {
    const parts = path.split(/[/\\]/);
    if (parts.length > 1) {
        parts.pop(); // Remove the filename
        return parts.join('/'); 
    }
    return '';
}

/**
 * Updates the last used directory in the app settings.
 * This requires the wallet password because settings are stored encrypted.
 */
export async function updateLastUsedDirectory(
    fullPath: string, 
    currentSettings: AppSettings, 
    protectAction: <T>(action: (password: string | null) => Promise<T>) => Promise<T | void>
) {
    const newDir = getDirectoryFromPath(fullPath);
    if (!newDir) return;

    if (currentSettings.last_used_directory === newDir) return;

    try {
        const updatedSettings = { ...currentSettings, last_used_directory: newDir };
        
        await protectAction(async (password) => {
            await invoke("save_app_settings", { 
                settings: updatedSettings,
                password 
            });
            logger.info(`Last used directory updated to: ${newDir}`);
            return true;
        });
    } catch (e) {
        logger.error(`Failed to update last used directory: ${e}`);
    }
}
