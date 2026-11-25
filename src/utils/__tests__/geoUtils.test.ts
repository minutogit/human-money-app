import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
    normalizeCoordinates,
    isValidCoordinateString,
    extractCoordinatesFromUrl,
    geocodeAddress,
    getCurrentLocation,
    roundCoordinates,
    calculateDistanceKm,
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

        it("should omit street and houseNumber in coarse mode (decimals <= 2)", async () => {
            vi.mocked(fetch).mockResolvedValueOnce({
                ok: true,
                json: async () => [{ lat: "52.520008", lon: "13.404954" }],
            } as Response);

            const result = await geocodeAddress(
                {
                    street: "Geheimweg",
                    houseNumber: "99",
                    zipCode: "10117",
                    city: "Berlin",
                    country: "Germany",
                },
                2,
            );
            expect(result).toBe("52.52, 13.40");
            const calledUrl = String(vi.mocked(fetch).mock.calls[0][0]);
            expect(calledUrl).not.toContain("Geheimweg");
            expect(calledUrl).not.toContain("99");
            expect(calledUrl).toContain("Berlin");
            expect(calledUrl).toContain("10117");
        });

        it("should include street and houseNumber in precise mode (decimals = 6)", async () => {
            vi.mocked(fetch).mockResolvedValueOnce({
                ok: true,
                json: async () => [{ lat: "52.520008", lon: "13.404954" }],
            } as Response);

            const result = await geocodeAddress(
                {
                    street: "Unter den Linden",
                    houseNumber: "1",
                    zipCode: "10117",
                    city: "Berlin",
                    country: "Germany",
                },
                6,
            );
            expect(result).toBe("52.520008, 13.404954");
            const calledUrl = String(vi.mocked(fetch).mock.calls[0][0]);
            expect(calledUrl).toContain("Unter%20den%20Linden");
            expect(calledUrl).toContain("10117");
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

        it("should reject non-OK HTTP responses and honor explicit coarseOnly option", async () => {
            vi.mocked(fetch).mockResolvedValueOnce({
                ok: false,
                status: 503,
            } as Response);

            await expect(geocodeAddress({ city: "Berlin" })).rejects.toThrow(
                "Geocoding server responded with status: 503"
            );

            vi.mocked(fetch).mockResolvedValueOnce({
                ok: true,
                json: async () => [{ lat: "52.520008", lon: "13.404954" }],
            } as Response);

            const result = await geocodeAddress(
                {
                    street: "Geheimweg",
                    houseNumber: "99",
                    zipCode: "10117",
                    city: "Berlin",
                    country: "Germany",
                },
                6,
                { coarseOnly: true },
            );
            expect(result).toBe("52.520008, 13.404954");
            const calledUrl = String(vi.mocked(fetch).mock.calls[1][0]);
            expect(calledUrl).not.toContain("Geheimweg");
            expect(calledUrl).not.toContain("99");
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

        it("should surface unavailable, timeout, and out-of-range GPS errors", async () => {
            vi.mocked(navigator.geolocation.getCurrentPosition).mockImplementationOnce((_success, error) => {
                error?.({
                    code: 2,
                    message: "unavailable",
                    PERMISSION_DENIED: 1,
                    POSITION_UNAVAILABLE: 2,
                    TIMEOUT: 3,
                });
            });
            await expect(getCurrentLocation()).rejects.toThrow("Location information is unavailable");

            vi.mocked(navigator.geolocation.getCurrentPosition).mockImplementationOnce((_success, error) => {
                error?.({
                    code: 3,
                    message: "timeout",
                    PERMISSION_DENIED: 1,
                    POSITION_UNAVAILABLE: 2,
                    TIMEOUT: 3,
                });
            });
            await expect(getCurrentLocation()).rejects.toThrow("Location request timed out");

            vi.mocked(navigator.geolocation.getCurrentPosition).mockImplementationOnce((success) => {
                success({
                    coords: {
                        latitude: 999,
                        longitude: 13.4,
                        accuracy: 0,
                        altitude: null,
                        altitudeAccuracy: null,
                        heading: null,
                        speed: null,
                    } as unknown as GeolocationCoordinates,
                    timestamp: Date.now(),
                } as unknown as GeolocationPosition);
            });
            await expect(getCurrentLocation()).rejects.toThrow("Received invalid coordinates from GPS");
        });
    });

    describe("roundCoordinates", () => {
        it("should round to 2 decimals by default (~1 km precision)", () => {
            expect(roundCoordinates("52.520008, 13.404954")).toBe("52.52, 13.40");
            expect(roundCoordinates("52.520008, 13.404954", 2)).toBe("52.52, 13.40");
        });

        it("should round to 3 decimals (~100 m neighborhood precision)", () => {
            expect(roundCoordinates("52.520008, 13.404954", 3)).toBe("52.520, 13.405");
        });

        it("should round to 4 decimals (~10 m exact precision)", () => {
            expect(roundCoordinates("52.520008, 13.404954", 4)).toBe("52.5200, 13.4050");
        });

        it("should return null for invalid coordinates", () => {
            expect(roundCoordinates("")).toBeNull();
            expect(roundCoordinates("invalid coords")).toBeNull();
            expect(roundCoordinates("100.0, 200.0")).toBeNull();
        });
    });

    describe("calculateDistanceKm", () => {
        it("should calculate distance correctly between two known locations (Berlin to Munich)", () => {
            // Berlin: 52.5200, 13.4050; Munich: 48.1371, 11.5761
            const dist = calculateDistanceKm("52.5200, 13.4050", "48.1371, 11.5761");
            expect(typeof dist).toBe("number");
            const distanceKm = dist as number;
            // Distance is approx 504 km
            expect(distanceKm).toBeGreaterThan(500);
            expect(distanceKm).toBeLessThan(510);
        });

        it("should return 0 for identical coordinates", () => {
            const dist = calculateDistanceKm("52.52, 13.40", "52.52, 13.40");
            expect(dist).toBeCloseTo(0, 1);
        });

        it("should return null if either coordinate string is missing or invalid", () => {
            expect(calculateDistanceKm(null, "52.52, 13.40")).toBeNull();
            expect(calculateDistanceKm("52.52, 13.40", undefined)).toBeNull();
            expect(calculateDistanceKm("invalid", "52.52, 13.40")).toBeNull();
            expect(calculateDistanceKm("52.52, 13.40", "invalid")).toBeNull();
        });
    });
});
