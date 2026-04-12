// src/utils/geoUtils.ts

/**
 * Normalizes a coordinate string into "latitude, longitude" format.
 * Supports various separators like ",", ";", " ", and trims whitespace.
 * Returns null if the input cannot be parsed into valid coordinates.
 */
export function normalizeCoordinates(input: string): string | null {
    if (!input || input.trim() === "") return null;

    // Replace common separators with spaces to make splitting easier
    const cleaned = input.replace(/[,;]/g, " ").trim();
    
    // Split by one or more spaces
    const parts = cleaned.split(/\s+/);
    
    if (parts.length < 2) return null;

    const lat = parseFloat(parts[0]);
    const lon = parseFloat(parts[1]);

    if (isNaN(lat) || isNaN(lon)) return null;

    // Validate ranges
    if (lat < -90 || lat > 90 || lon < -180 || lon > 180) {
        return null;
    }

    // Format to 6 decimal places for high precision but clean look
    return `${lat.toFixed(6)}, ${lon.toFixed(6)}`;
}

/**
 * Checks if a string has a valid coordinate format without fully normalizing it yet.
 */
export function isValidCoordinateString(input: string): boolean {
    return normalizeCoordinates(input) !== null;
}
