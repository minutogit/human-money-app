/**
 * Helper functions for handling User IDs in both SAI (prefix:checksum@did:key:...)
 * and Root (did:key:...) formats.
 */

const DID_KEY_PREFIX = "did:key:";

/**
 * Extracts a human-readable display name from a User ID.
 * 
 * - SAI: "alice:abc@did:key:zABC..." -> "alice"
 * - Root: "did:key:zABC..." -> "zABC12" (first 6 chars of the key)
 */
export function extractDisplayName(userId: string): string {
    if (!userId) return "Unknown";

    if (userId.includes('@')) {
        return userId.split('@')[0].split(':')[0];
    }

    if (userId.startsWith(DID_KEY_PREFIX)) {
        const key = userId.substring(DID_KEY_PREFIX.length);
        return key.substring(0, 8);
    }

    return userId;
}

/**
 * Truncates a User ID for UI display.
 * 
 * - SAI: "alice:abc@did:key:zABC..." -> "alice:abc@...zABC"
 * - Root: "did:key:zABC..." -> "did:key:zABC...ABC"
 */
export function truncateUserId(userId: string): string {
    if (!userId) return "";

    if (userId.includes('@')) {
        const parts = userId.split('@');
        const prefix = parts[0];
        const key = parts[1];
        if (key.length > 25) {
            const afterDidKey = key.startsWith(DID_KEY_PREFIX) ? key.substring(DID_KEY_PREFIX.length) : key;
            return `${prefix}@${DID_KEY_PREFIX}${afterDidKey.substring(0, 12)}...${key.substring(key.length - 5)}`;
        }
        return userId;
    }

    if (userId.startsWith(DID_KEY_PREFIX)) {
        const key = userId.substring(DID_KEY_PREFIX.length);
        if (key.length > 25) {
            return `${DID_KEY_PREFIX}${key.substring(0, 20)}...${key.substring(key.length - 5)}`;
        }
        return userId;
    }

    if (userId.length > 20) {
        return `${userId.substring(0, 10)}...${userId.substring(userId.length - 6)}`;
    }

    return userId;
}

/**
 * Suggests a safe filename based on a User ID.
 * 
 * - SAI: "alice:abc@did:key:z..." -> "alice"
 * - Root: "did:key:zABC..." -> "zABC123"
 */
export function suggestFilename(userId: string): string {
    if (!userId) return "transfer";

    if (userId.includes('@')) {
        const namePart = userId.split('@')[0].split(':')[0];
        return namePart.replace(/[^a-z0-9_-]/gi, '_');
    }

    if (userId.startsWith(DID_KEY_PREFIX)) {
        const key = userId.substring(DID_KEY_PREFIX.length);
        return key.substring(0, 8).replace(/[^a-z0-9_-]/gi, '_');
    }

    return "transfer";
}

/**
 * Extracts the subaccount/profile prefix from a prefixed SAI User ID (`prefix:chk@did:key:...`),
 * or returns `null` for prefix-free root DIDs.
 */
export function extractUserIdPrefix(userId: string): string | null {
    const trimmed = (userId || "").trim();
    const atIdx = trimmed.indexOf('@');
    if (atIdx <= 0) return null;
    const prefixPart = trimmed.slice(0, atIdx).split(':')[0].trim();
    return prefixPart || null;
}

/**
 * Extracts the canonical root `did:key:...` part from either a plain DID
 * or an SAI/subaccount User ID (`prefix:checksum@did:key:...`).
 */
export function extractCanonicalDid(userId: string): string {
    const trimmed = (userId || "").trim();
    if (!trimmed) return "";
    const atIdx = trimmed.lastIndexOf('@');
    return atIdx !== -1 ? trimmed.slice(atIdx + 1).trim() : trimmed;
}

/**
 * Returns true when two User IDs refer to the exact same profile (including prefix).
 */
export function isExactSameUserId(a: string, b: string): boolean {
    const ta = (a || "").trim();
    const tb = (b || "").trim();
    return Boolean(ta && tb && ta === tb);
}

/**
 * Returns true when two User IDs share the same underlying root `did:key:...` identity
 * (either identical profile or sibling subaccount/root).
 */
export function isSameRootIdentity(a: string, b: string): boolean {
    const ca = extractCanonicalDid(a);
    const cb = extractCanonicalDid(b);
    return Boolean(ca && cb && ca.startsWith(DID_KEY_PREFIX) && ca === cb);
}
