// src/utils/fileIo.ts
import { open, save } from '@tauri-apps/plugin-dialog';
import { readFile, writeFile } from '@tauri-apps/plugin-fs';
import type { UiPreferences } from '../types';
import { stringifyError } from './errorHelper';
import { logger } from './log';
import { updateLastUsedDirectory } from './settingsUtils';

export interface FileDialogFilter {
    name: string;
    extensions: string[];
}

export type ProtectActionFn = <T>(
    action: (password: string | null) => Promise<T>,
    options?: { requireFullAccess?: boolean },
) => Promise<T | void>;

export interface SaveFileOptions {
    defaultFilename: string;
    filters?: FileDialogFilter[];
    uiPreferences?: Pick<UiPreferences, 'lastUsedDirectory'> | null;
    protectAction?: ProtectActionFn;
    mimeType?: string;
}

export interface SaveFileResult {
    path: string | null;
    downloaded: boolean;
}

export interface OpenFileOptions {
    filters?: FileDialogFilter[];
    uiPreferences?: Pick<UiPreferences, 'lastUsedDirectory'> | null;
    protectAction?: ProtectActionFn;
}

export interface OpenFileResult {
    path: string;
    bytes: number[];
}

function buildDefaultPath(
    defaultFilename: string,
    uiPreferences?: Pick<UiPreferences, 'lastUsedDirectory'> | null,
): string {
    return uiPreferences?.lastUsedDirectory
        ? `${uiPreferences.lastUsedDirectory}/${defaultFilename}`
        : defaultFilename;
}

function triggerBrowserDownload(
    bytes: Uint8Array,
    filename: string,
    mimeType: string = 'application/octet-stream',
): boolean {
    if (
        typeof window === 'undefined' ||
        typeof document === 'undefined' ||
        typeof URL === 'undefined' ||
        typeof URL.createObjectURL !== 'function'
    ) {
        return false;
    }
    const copy = new Uint8Array(bytes.byteLength);
    copy.set(bytes);
    const blob = new Blob([copy.buffer], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    anchor.click();
    URL.revokeObjectURL(url);
    return true;
}

async function persistLastUsedDirectory(
    filePath: string,
    uiPreferences?: Pick<UiPreferences, 'lastUsedDirectory'> | null,
    protectAction?: ProtectActionFn,
): Promise<void> {
    if (!uiPreferences || !protectAction) {
        return;
    }
    await Promise.resolve(updateLastUsedDirectory(filePath, uiPreferences, protectAction)).catch(
        () => {},
    );
}

/**
 * Prompts the user for a destination path using Tauri's save dialog.
 * Returns `null` when cancelled.
 */
export async function pickSaveFilePath(options: {
    defaultFilename: string;
    filters?: FileDialogFilter[];
    uiPreferences?: Pick<UiPreferences, 'lastUsedDirectory'> | null;
}): Promise<string | null> {
    const selected = await save({
        defaultPath: buildDefaultPath(options.defaultFilename, options.uiPreferences),
        filters: options.filters,
    });
    return typeof selected === 'string' && selected.length > 0 ? selected : null;
}

/**
 * Prompts the user to select a directory using Tauri's open dialog.
 * Returns `null` when cancelled.
 */
export async function pickDirectoryPath(defaultPath?: string): Promise<string | null> {
    const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: defaultPath || undefined,
    });
    return typeof selected === 'string' && selected.length > 0 ? selected : null;
}

/**
 * Saves file bytes via Tauri's save dialog + fs `writeFile`, falling back to
 * an `<a download>` Blob download when running in non-Tauri browser preview.
 * Returns `null` if the user cancels the save dialog.
 */
export async function saveBytesToFile(
    data: Uint8Array | number[],
    options: SaveFileOptions,
): Promise<SaveFileResult | null> {
    const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
    let filePath: string | null = null;
    let dialogUnavailable = false;

    try {
        const selected = await save({
            defaultPath: buildDefaultPath(options.defaultFilename, options.uiPreferences),
            filters: options.filters,
        });
        filePath = typeof selected === 'string' && selected.length > 0 ? selected : null;
    } catch (err) {
        dialogUnavailable = true;
        logger.info(`Tauri save dialog not available, trying browser download: ${stringifyError(err)}`);
    }

    if (filePath) {
        await writeFile(filePath, bytes);
        await persistLastUsedDirectory(filePath, options.uiPreferences, options.protectAction);
        return { path: filePath, downloaded: false };
    }

    if (dialogUnavailable) {
        const downloaded = triggerBrowserDownload(bytes, options.defaultFilename, options.mimeType);
        if (downloaded) {
            return { path: null, downloaded: true };
        }
    }

    return null;
}

/**
 * Opens a single file via Tauri's open dialog + fs `readFile`, updating
 * `lastUsedDirectory` when preferences and `protectAction` are provided.
 * Returns `null` when cancelled.
 */
export async function openFileAsBytes(options: OpenFileOptions = {}): Promise<OpenFileResult | null> {
    const selectedPath = await open({
        multiple: false,
        defaultPath: options.uiPreferences?.lastUsedDirectory ?? undefined,
        filters: options.filters,
    });
    if (typeof selectedPath !== 'string' || selectedPath.length === 0) {
        return null;
    }
    await persistLastUsedDirectory(selectedPath, options.uiPreferences, options.protectAction);
    const fileUint8Array = await readFile(selectedPath);
    return {
        path: selectedPath,
        bytes: Array.from(fileUint8Array),
    };
}
