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

/**
 * Checks if a string has a valid coordinate format without fully normalizing it yet.
 */
export function isValidCoordinateString(input: string): boolean {
    return normalizeCoordinates(input) !== null;
}

/**
 * Geocodes an Address object using OpenStreetMap Nominatim.
 * Returns coordinates as a "lat, lon" string or throws an error.
 */
export async function geocodeAddress(address: Address): Promise<string> {
    const parts = [
        address.street,
        address.houseNumber,
        address.city,
        address.zipCode,
        address.country
    ].map(p => p?.trim()).filter(Boolean);

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
            return `${lat.toFixed(6)}, ${lon.toFixed(6)}`;
        }
    }

    throw new Error("No coordinates found for this address");
}

/**
 * Gets the current device GPS location using the HTML5 Geolocation API.
 * Returns coordinates as a "lat, lon" string or throws an error.
 */
export function getCurrentLocation(): Promise<string> {
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
                    resolve(`${lat.toFixed(6)}, ${lon.toFixed(6)}`);
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
