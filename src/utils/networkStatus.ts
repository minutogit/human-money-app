// src/utils/networkStatus.ts

type NavigatorWithConnection = Navigator & {
    connection?: {
        type?: string;
        effectiveType?: string;
        saveData?: boolean;
    };
};

/**
 * Returns true when the runtime reports an explicit cellular or data-saver connection.
 */
export function isMeteredOrCellularConnection(): boolean {
    if (typeof navigator === 'undefined') return false;
    const conn = (navigator as NavigatorWithConnection).connection;
    return Boolean(conn?.saveData || conn?.type === 'cellular');
}

/**
 * Detects whether sync traffic can be treated as WiFi / unmetered.
 * Falls back to `true` when the Network Information API is unavailable (e.g. Linux desktop/Tauri webviews).
 */
export function detectIsWifiConnection(): boolean {
    if (typeof navigator === 'undefined') return true;
    const conn = (navigator as NavigatorWithConnection).connection;
    if (!conn) return true;
    if (conn.saveData || conn.type === 'cellular') return false;
    if (conn.type === 'wifi' || conn.type === 'ethernet') return true;
    return conn.type !== 'none';
}
