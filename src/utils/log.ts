import { invoke } from "@tauri-apps/api/core";

/**
 * Logs a message to the Tauri backend with a specified log level.
 * @param level The log level ('info', 'warn', 'error', etc.).
 * @param message The message to log.
 */
async function logToBackend(level: string, message: string) {
    try {
        await invoke("log_to_backend", { level, message });
    } catch (e) {
        console.error(`Failed to log to backend: ${e}`);
    }
}

export const logger = {
    info: (message: string) => logToBackend("info", message),
    warn: (message: string) => logToBackend("warn", message),
    error: (message: string) => logToBackend("error", message),
};