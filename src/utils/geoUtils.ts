// src/utils/geoUtils.ts

import { Address } from "../types";

/**
 * Tries to extract coordinates (lat, lon) from various map URL formats
 * (e.g. Google Maps, OpenStreetMap).
 * Returns string "lat, lon" or null if no pattern is matched.
 */
export function extractCoordinatesFromUrl(url: string): string | null {
    if (!url) return null;

    // Pattern for typical lat/lon coordinate formats in URLs
    // Google Maps: /@51.1656,10.4515,14z or ?q=51.1656,10.4515
    // OpenStreetMap: #map=16/51.1600/10.4500
    // general query params: lat=51.1656&lon=10.4515 or q=51.1656,10.4515

    // Match @lat,lon
    const atPattern = /@(-?\d+\.\d+),(-?\d+\.\d+)/;
    const matchAt = url.match(atPattern);
    if (matchAt) {
        const lat = parseFloat(matchAt[1]);
        const lon = parseFloat(matchAt[2]);
        if (isValidCoordinate(lat, lon)) return `${lat.toFixed(6)}, ${lon.toFixed(6)}`;
    }

    // Match #map=zoom/lat/lon
    const osmPattern = /#map=\d+\/(-?\d+\.\d+)\/(-?\d+\.\d+)/;
    const matchOsm = url.match(osmPattern);
    if (matchOsm) {
        const lat = parseFloat(matchOsm[1]);
        const lon = parseFloat(matchOsm[2]);
        if (isValidCoordinate(lat, lon)) return `${lat.toFixed(6)}, ${lon.toFixed(6)}`;
    }

    // Match query parameters like ?lat=xxx&lon=yyy or &lat=xxx&lon=yyy
    const latParam = url.match(/[?&]lat=(-?\d+\.\d+)/);
    const lonParam = url.match(/[?&]lon=(-?\d+\.\d+)/);
    if (latParam && lonParam) {
        const lat = parseFloat(latParam[1]);
        const lon = parseFloat(lonParam[1]);
        if (isValidCoordinate(lat, lon)) return `${lat.toFixed(6)}, ${lon.toFixed(6)}`;
    }

    // Match query parameters like q=lat,lon
    const qParam = url.match(/[?&]q=(-?\d+\.\d+),(-?\d+\.\d+)/);
    if (qParam) {
        const lat = parseFloat(qParam[1]);
        const lon = parseFloat(qParam[2]);
        if (isValidCoordinate(lat, lon)) return `${lat.toFixed(6)}, ${lon.toFixed(6)}`;
    }

    return null;
}

function isValidCoordinate(lat: number, lon: number): boolean {
    return lat >= -90 && lat <= 90 && lon >= -180 && lon <= 180;
}

/**
 * Normalizes a coordinate string into "latitude, longitude" format.
 * Supports various separators like ",", ";", " ", and trims whitespace.
 * Also supports map URLs by extracting coordinates from them.
 * Returns null if the input cannot be parsed into valid coordinates.
 */
export function normalizeCoordinates(input: string): string | null {
    if (!input || input.trim() === "") return null;

    // Check if it's a URL or contains domain patterns, and try to extract coordinates
    if (input.includes("http://") || input.includes("https://") || input.includes("www.") || input.includes("maps")) {
        const extracted = extractCoordinatesFromUrl(input);
        if (extracted) return extracted;
    }

    // Replace common separators with spaces to make splitting easier
    const cleaned = input.replace(/[,;]/g, " ").trim();
    
    // Split by one or more spaces
    const parts = cleaned.split(/\s+/);
    
    if (parts.length < 2) return null;

    const lat = parseFloat(parts[0]);
    const lon = parseFloat(parts[1]);

    if (isNaN(lat) || isNaN(lon)) return null;

    if (!isValidCoordinate(lat, lon)) {
        return null;
    }

    // Format to 6 decimal places for high precision but clean look
    return `${lat.toFixed(6)}, ${lon.toFixed(6)}`;
}

export type CoordinatePrecision = 2 | 3 | 4;

/**
 * Rounds coordinate string to the specified decimal precision.
 * Precision: 2 = ~1 km city district, 3 = ~100 m neighborhood, 4 = ~10 m exact.
 */
export function roundCoordinates(input: string, decimals: CoordinatePrecision = 2): string | null {
    const normalized = normalizeCoordinates(input);
    if (!normalized) return null;
    const parts = normalized.split(',').map((s) => s.trim());
    if (parts.length < 2) return null;
    const lat = parseFloat(parts[0]);
    const lon = parseFloat(parts[1]);
    if (isNaN(lat) || isNaN(lon)) return null;
    return `${lat.toFixed(decimals)}, ${lon.toFixed(decimals)}`;
}

/**
 * Calculates the Haversine distance in kilometers between two "lat, lon" strings.
 * Returns null if either coordinate string is missing or invalid.
 */
export function calculateDistanceKm(coordsA?: string | null, coordsB?: string | null): number | null {
    if (!coordsA || !coordsB) return null;
    const normA = normalizeCoordinates(coordsA);
    const normB = normalizeCoordinates(coordsB);
    if (!normA || !normB) return null;

    const partsA = normA.split(',').map((s) => s.trim());
    const partsB = normB.split(',').map((s) => s.trim());
    if (partsA.length < 2 || partsB.length < 2) return null;

    const lat1 = parseFloat(partsA[0]);
    const lon1 = parseFloat(partsA[1]);
    const lat2 = parseFloat(partsB[0]);
    const lon2 = parseFloat(partsB[1]);

    if (isNaN(lat1) || isNaN(lon1) || isNaN(lat2) || isNaN(lon2)) return null;

    const R = 6371; // Earth radius in km
    const dLat = ((lat2 - lat1) * Math.PI) / 180;
    const dLon = ((lon2 - lon1) * Math.PI) / 180;
    const a =
        Math.sin(dLat / 2) * Math.sin(dLat / 2) +
        Math.cos((lat1 * Math.PI) / 180) *
            Math.cos((lat2 * Math.PI) / 180) *
            Math.sin(dLon / 2) *
            Math.sin(dLon / 2);
    const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
    return R * c;
}

/**
 * Checks if a string has a valid coordinate format without fully normalizing it yet.
 */
export function isValidCoordinateString(input: string): boolean {
    return normalizeCoordinates(input) !== null;
}

export interface GeocodeAddressOptions {
    coarseOnly?: boolean;
}

/**
 * Geocodes an Address object using OpenStreetMap Nominatim.
 * Returns coordinates as a "lat, lon" string or throws an error.
 * When `coarseOnly` is true (default for `decimals <= 2`), street and house number
 * are omitted so only city, zip code and country leave the device.
 */
export async function geocodeAddress(
    address: Address,
    decimals: number = 6,
    options?: GeocodeAddressOptions,
): Promise<string> {
    const coarseOnly = options?.coarseOnly ?? decimals <= 2;
    const parts = (coarseOnly
        ? [address.city, address.zipCode, address.country]
        : [address.street, address.houseNumber, address.city, address.zipCode, address.country]
    ).map(p => p?.trim()).filter(Boolean);

    if (parts.length === 0) {
        throw new Error("Address is empty");
    }

    const query = encodeURIComponent(parts.join(", "));
    const response = await fetch(`https://nominatim.openstreetmap.org/search?q=${query}&format=json&limit=1`, {
        headers: {
            "User-Agent": "HumanMoneyWalletApp/1.0"
        }
    });

    if (!response.ok) {
        throw new Error(`Geocoding server responded with status: ${response.status}`);
    }

    const data = await response.json();
    if (data && data.length > 0) {
        const lat = parseFloat(data[0].lat);
        const lon = parseFloat(data[0].lon);
        if (isValidCoordinate(lat, lon)) {
            return `${lat.toFixed(decimals)}, ${lon.toFixed(decimals)}`;
        }
    }

    throw new Error("No coordinates found for this address");
}

/**
 * Gets the current device GPS location using the HTML5 Geolocation API.
 * Returns coordinates as a "lat, lon" string or throws an error.
 */
export function getCurrentLocation(decimals: number = 6): Promise<string> {
    return new Promise((resolve, reject) => {
        if (!navigator.geolocation) {
            reject(new Error("GPS Geolocation is not supported by your device/browser"));
            return;
        }

        navigator.geolocation.getCurrentPosition(
            (position) => {
                const lat = position.coords.latitude;
                const lon = position.coords.longitude;
                if (isValidCoordinate(lat, lon)) {
                    resolve(`${lat.toFixed(decimals)}, ${lon.toFixed(decimals)}`);
                } else {
                    reject(new Error("Received invalid coordinates from GPS"));
                }
            },
            (error) => {
                let msg = "Could not retrieve GPS location";
                if (error.code === error.PERMISSION_DENIED) {
                    msg = "Location permission denied";
                } else if (error.code === error.POSITION_UNAVAILABLE) {
                    msg = "Location information is unavailable";
                } else if (error.code === error.TIMEOUT) {
                    msg = "Location request timed out";
                }
                reject(new Error(msg));
            },
            { enableHighAccuracy: true, timeout: 8000 }
        );
    });
}
