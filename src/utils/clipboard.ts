import {
    readText as tauriReadText,
    writeText as tauriWriteText,
} from '@tauri-apps/plugin-clipboard-manager';

/**
 * Copies text to the system clipboard using the Tauri clipboard plugin
 * with a fallback to the browser Clipboard API.
 */
export async function copyToClipboard(text: string): Promise<boolean> {
    try {
        await tauriWriteText(text);
        return true;
    } catch {
        try {
            if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
                await navigator.clipboard.writeText(text);
                return true;
            }
        } catch {
            // Fallback also failed
        }
        return false;
    }
}

export interface ClipboardReadResult {
    text: string;
    error: boolean;
}

/**
 * Reads plain text from the system clipboard using the Tauri clipboard plugin
 * with a fallback to the browser Clipboard API.
 */
export async function readFromClipboard(): Promise<ClipboardReadResult> {
    try {
        const text = await tauriReadText();
        if (text && text.length > 0) {
            return { text, error: false };
        }
        if (typeof navigator !== 'undefined' && navigator.clipboard?.readText) {
            try {
                const navText = await navigator.clipboard.readText();
                if (navText && navText.length > 0) {
                    return { text: navText, error: false };
                }
            } catch {
                // Ignore secondary fallback error when primary read succeeded as empty
            }
        }
        return { text: text ?? '', error: false };
    } catch {
        try {
            if (typeof navigator !== 'undefined' && navigator.clipboard?.readText) {
                const navText = await navigator.clipboard.readText();
                return { text: navText ?? '', error: false };
            }
        } catch {
            // Both Tauri and browser clipboard failed
        }
        return { text: '', error: true };
    }
}
