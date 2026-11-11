// src/utils/imageCompression.ts

/**
 * Pure HTML5 Image + HTMLCanvasElement image compression utilities (zero external dependencies).
 * Enforces WebP (or JPEG fallback on platforms without canvas WebP support, e.g. WebKitGTK / Safari)
 * with size and dimension limits according to the media sync plan.
 */

export function isWebpCanvasSupported(): boolean {
    if (typeof document === 'undefined') return false;
    try {
        const c = document.createElement('canvas');
        c.width = 1;
        c.height = 1;
        return c.toDataURL('image/webp').startsWith('data:image/webp');
    } catch {
        return false;
    }
}

export function estimateDecodedByteSize(dataUri: string): number {
    const commaIdx = dataUri.indexOf(',');
    const base64 = commaIdx >= 0 ? dataUri.slice(commaIdx + 1) : dataUri;
    const len = base64.length;
    let padding = 0;
    if (len > 0 && base64[len - 1] === '=') padding++;
    if (len > 1 && base64[len - 2] === '=') padding++;
    return Math.floor((len * 3) / 4) - padding;
}

function loadImageFromFile(file: File): Promise<HTMLImageElement> {
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => {
            const img = new Image();
            img.onload = () => resolve(img);
            img.onerror = () => reject(new Error('Failed to load image'));
            img.src = reader.result as string;
        };
        reader.onerror = () => reject(new Error('Failed to read file'));
        reader.readAsDataURL(file);
    });
}

/**
 * Compresses an avatar or logo image to a 96x96 center-cropped WebP (or JPEG) data URI.
 * First tries WebP (if supported) at qualities [0.7, 0.5, 0.4, 0.3, 0.2].
 * If WebP is unsupported (WebKitGTK, Safari) or output exceeds 4096 chars, falls back to JPEG at qualities [0.7, 0.5, 0.4, 0.3, 0.2, 0.1].
 */
export async function compressAvatarToWebpDataUri(file: File): Promise<string> {
    const img = await loadImageFromFile(file);
    const width = img.naturalWidth || img.width || 96;
    const height = img.naturalHeight || img.height || 96;
    const minSide = Math.min(width, height);
    const sx = (width - minSide) / 2;
    const sy = (height - minSide) / 2;

    const canvas = document.createElement('canvas');
    canvas.width = 96;
    canvas.height = 96;
    const ctx = canvas.getContext('2d');
    if (!ctx) {
        throw new Error('Canvas 2D context not available');
    }

    ctx.drawImage(img, sx, sy, minSide, minSide, 0, 0, 96, 96);

    const hasWebp = isWebpCanvasSupported();
    if (hasWebp) {
        const webpQualities = [0.65, 0.5, 0.4, 0.3, 0.2];
        for (const q of webpQualities) {
            const res = canvas.toDataURL('image/webp', q);
            if (res.startsWith('data:image/webp') && res.length <= 4096) {
                return res;
            }
        }
    }

    // Fallback to JPEG (supported everywhere including WebKitGTK / Safari; 96x96 fits easily under 4096)
    const jpegQualities = [0.65, 0.5, 0.4, 0.3, 0.2, 0.1];
    let result = '';
    for (const q of jpegQualities) {
        result = canvas.toDataURL('image/jpeg', q);
        if (result.length <= 4096) {
            return result;
        }
    }
    return result;
}

/**
 * Compresses an ad listing image to a max 800x800 WebP (or JPEG) data URI.
 * Uses WebP if supported, otherwise JPEG.
 * Tries qualities [0.78, 0.65, 0.52, 0.4, 0.3] until decoded byte size <= maxBytes - 64.
 * If size still exceeds threshold, iteratively downscales canvas dimensions by 0.8.
 */
export async function compressAdImageToWebpDataUri(
    file: File,
    maxBytes: number = 102400
): Promise<string> {
    const img = await loadImageFromFile(file);
    let width = img.naturalWidth || img.width || 800;
    let height = img.naturalHeight || img.height || 800;

    if (width > 800 || height > 800) {
        const ratio = Math.min(800 / width, 800 / height);
        width = Math.round(width * ratio);
        height = Math.round(height * ratio);
    }

    const hasWebp = isWebpCanvasSupported();
    const mimeType = hasWebp ? 'image/webp' : 'image/jpeg';
    const threshold = maxBytes - 64;
    const qualities = [0.78, 0.65, 0.52, 0.4, 0.3];

    let currentWidth = width;
    let currentHeight = height;
    let result = '';

    for (let scaleAttempt = 0; scaleAttempt < 5; scaleAttempt++) {
        const canvas = document.createElement('canvas');
        canvas.width = currentWidth;
        canvas.height = currentHeight;
        const ctx = canvas.getContext('2d');
        if (!ctx) {
            throw new Error('Canvas 2D context not available');
        }

        ctx.drawImage(img, 0, 0, currentWidth, currentHeight);

        for (const q of qualities) {
            result = canvas.toDataURL(mimeType, q);
            if (estimateDecodedByteSize(result) <= threshold) {
                return result;
            }
        }

        // If even lowest quality is too big, scale down dimensions and retry
        currentWidth = Math.max(96, Math.round(currentWidth * 0.8));
        currentHeight = Math.max(96, Math.round(currentHeight * 0.8));
    }

    return result;
}
