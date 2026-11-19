// src/utils/settingsUtils.ts
import { settingsService } from "../services/settingsService";
import { UiPreferences } from "../types";
import { logger } from "./log";
import { stringifyError } from "./errorHelper";

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
 * Updates the last used directory in UI preferences via an atomic patch.
 */
export async function updateLastUsedDirectory(
    fullPath: string,
    currentUiPreferences: Pick<UiPreferences, 'lastUsedDirectory'> | null | undefined,
    protectAction: <T>(action: (password: string | null) => Promise<T>) => Promise<T | void>,
): Promise<UiPreferences | void> {
    const newDir = getDirectoryFromPath(fullPath);
    if (!newDir) return;

    if (currentUiPreferences?.lastUsedDirectory === newDir) return;

    try {
        return await protectAction(async (password) => {
            const updated = await settingsService.patchUiPreferences(
                { lastUsedDirectory: newDir },
                password || undefined,
            );
            logger.info(`Last used directory updated to: ${newDir}`);
            return updated;
        });
    } catch (e) {
        logger.error(`Failed to update last used directory: ${stringifyError(e)}`);
    }
}
