import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
    normalizeCoordinates,
    isValidCoordinateString,
    extractCoordinatesFromUrl,
    geocodeAddress,
    getCurrentLocation
} from "../geoUtils";

describe("geoUtils", () => {
    describe("extractCoordinatesFromUrl", () => {
        it("should parse Google Maps @ style URL", () => {
            const url = "https://www.google.com/maps/@51.165600,10.451500,14z";
            expect(extractCoordinatesFromUrl(url)).toBe("51.165600, 10.451500");
        });

        it("should parse Google Maps query style URL", () => {
            const url = "https://google.com/maps?q=51.165600,10.451500";
            expect(extractCoordinatesFromUrl(url)).toBe("51.165600, 10.451500");
        });

        it("should parse OpenStreetMap #map style URL", () => {
            const url = "https://www.openstreetmap.org/#map=16/51.160000/10.450000";
            expect(extractCoordinatesFromUrl(url)).toBe("51.160000, 10.450000");
        });

        it("should parse URL with lat/lon query parameters", () => {
            const url = "https://example.com/map?lat=51.165600&lon=10.451500";
            expect(extractCoordinatesFromUrl(url)).toBe("51.165600, 10.451500");
        });

        it("should return null for URLs without coordinates", () => {
            const url = "https://google.com/maps/search/restaurants";
            expect(extractCoordinatesFromUrl(url)).toBeNull();
        });
    });

    describe("normalizeCoordinates", () => {
        it("should normalize valid formats", () => {
            expect(normalizeCoordinates("51.16, 10.45")).toBe("51.160000, 10.450000");
            expect(normalizeCoordinates("51.16;10.45")).toBe("51.160000, 10.450000");
            expect(normalizeCoordinates("51.16 10.45")).toBe("51.160000, 10.450000");
        });

        it("should normalize URLs directly", () => {
            const url = "https://www.google.com/maps/@51.165600,10.451500,14z";
            expect(normalizeCoordinates(url)).toBe("51.165600, 10.451500");
        });

        it("should return null for invalid formats or ranges", () => {
            expect(normalizeCoordinates("")).toBeNull();
            expect(normalizeCoordinates("abc, def")).toBeNull();
            expect(normalizeCoordinates("95.0, 10.0")).toBeNull(); // lat > 90
            expect(normalizeCoordinates("50.0, 190.0")).toBeNull(); // lon > 180
            expect(normalizeCoordinates("50.0")).toBeNull(); // single value
        });
    });

    describe("isValidCoordinateString", () => {
        it("should validate correctly", () => {
            expect(isValidCoordinateString("51.16, 10.45")).toBe(true);
            expect(isValidCoordinateString("95.0, 10.0")).toBe(false);
        });
    });

    describe("geocodeAddress", () => {
        beforeEach(() => {
            vi.stubGlobal("fetch", vi.fn());
        });

        afterEach(() => {
            vi.restoreAllMocks();
        });

        it("should throw if address is empty", async () => {
            await expect(geocodeAddress({})).rejects.toThrow("Address is empty");
        });

        it("should geocode address successfully", async () => {
            const mockResponse = [
                {
                    lat: "52.520008",
                    lon: "13.404954"
                }
            ];

            vi.mocked(fetch).mockResolvedValueOnce({
                ok: true,
                json: async () => mockResponse
            } as Response);

            const result = await geocodeAddress({ city: "Berlin", country: "Germany" });
            expect(result).toBe("52.520008, 13.404954");
            expect(fetch).toHaveBeenCalledWith(
                expect.stringContaining("https://nominatim.openstreetmap.org/search?q=Berlin%2C%20Germany"),
                expect.any(Object)
            );
        });

        it("should throw if no coordinates found", async () => {
            vi.mocked(fetch).mockResolvedValueOnce({
                ok: true,
                json: async () => []
            } as Response);

            await expect(geocodeAddress({ city: "NonexistentCity" })).rejects.toThrow(
                "No coordinates found for this address"
            );
        });
    });

    describe("getCurrentLocation", () => {
        beforeEach(() => {
            vi.stubGlobal("navigator", {
                geolocation: {
                    getCurrentPosition: vi.fn()
                }
            });
        });

        afterEach(() => {
            vi.unstubAllGlobals();
        });

        it("should retrieve GPS coordinates", async () => {
            vi.mocked(navigator.geolocation.getCurrentPosition).mockImplementation((success) => {
                success({
                    coords: {
                        latitude: 52.52,
                        longitude: 13.40,
                        accuracy: 0,
                        altitude: null,
                        altitudeAccuracy: null,
                        heading: null,
                        speed: null
                    } as unknown as GeolocationCoordinates,
                    timestamp: Date.now()
                } as unknown as GeolocationPosition);
            });

            const result = await getCurrentLocation();
            expect(result).toBe("52.520000, 13.400000");
        });

        it("should throw if geolocation is not supported", async () => {
            vi.stubGlobal("navigator", {});

            await expect(getCurrentLocation()).rejects.toThrow(
                "GPS Geolocation is not supported by your device/browser"
            );
        });

        it("should handle permission denied error", async () => {
            vi.mocked(navigator.geolocation.getCurrentPosition).mockImplementation((_success, error) => {
                if (error) {
                    error({
                        code: 1, // PERMISSION_DENIED
                        message: "Location permission denied",
                        PERMISSION_DENIED: 1,
                        POSITION_UNAVAILABLE: 2,
                        TIMEOUT: 3
                    });
                }
            });

            await expect(getCurrentLocation()).rejects.toThrow("Location permission denied");
        });
    });
});
