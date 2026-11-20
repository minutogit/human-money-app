import { describe, it, expect, vi } from 'vitest';
import {
  estimateDecodedByteSize,
  isWebpCanvasSupported,
  compressAvatarToWebpDataUri,
  compressAdImageToWebpDataUri,
} from '../imageCompression';

describe('imageCompression utils', () => {
  it('correctly estimates decoded byte size from data URI', () => {
    // "Hello World" in base64: SGVsbG8gV29ybGQ= (11 bytes)
    const uriWithPadding1 = 'data:image/webp;base64,SGVsbG8gV29ybGQ=';
    expect(estimateDecodedByteSize(uriWithPadding1)).toBe(11);

    // "Hello" in base64: SGVsbG8= (5 bytes, 2 padding characters ==)
    const uriWithPadding2 = 'data:image/webp;base64,SGVsbG8=';
    expect(estimateDecodedByteSize(uriWithPadding2)).toBe(5);

    // "123456" in base64: MTIzNDU2 (6 bytes, 0 padding)
    const uriWithoutPadding = 'data:image/webp;base64,MTIzNDU2';
    expect(estimateDecodedByteSize(uriWithoutPadding)).toBe(6);
  });

  it('detects WebP canvas support accurately', () => {
    const mockToDataURL = vi.fn().mockReturnValue('data:image/webp;base64,dummy');
    const spy = vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
      if (tagName === 'canvas') {
        return {
          width: 0,
          height: 0,
          toDataURL: mockToDataURL,
        } as unknown as HTMLCanvasElement;
      }
      return document.createElement(tagName);
    });

    expect(isWebpCanvasSupported()).toBe(true);

    mockToDataURL.mockReturnValue('data:image/png;base64,dummy');
    expect(isWebpCanvasSupported()).toBe(false);

    spy.mockRestore();
  });

  it('compresses avatar to 96x96 WebP and limits length <= 4096', async () => {
    const mockDrawImage = vi.fn();
    const mockToDataURL = vi.fn().mockReturnValue('data:image/webp;base64,mockavataruri');
    const mockGetContext = vi.fn().mockReturnValue({
      drawImage: mockDrawImage,
    });

    vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
      if (tagName === 'canvas') {
        return {
          width: 0,
          height: 0,
          getContext: mockGetContext,
          toDataURL: mockToDataURL,
        } as unknown as HTMLCanvasElement;
      }
      return document.createElement(tagName);
    });

    // Mock FileReader & Image
    const originalFileReader = globalThis.FileReader;
    const originalImage = globalThis.Image;

    class MockFileReader {
      result: string = 'data:image/jpeg;base64,dummyrawdata';
      onload: (() => void) | null = null;
      readAsDataURL() {
        setTimeout(() => this.onload?.(), 0);
      }
    }
    class MockImage {
      width: number = 200;
      height: number = 100;
      naturalWidth: number = 200;
      naturalHeight: number = 100;
      onload: (() => void) | null = null;
      set src(_val: string) {
        setTimeout(() => this.onload?.(), 0);
      }
    }

    Object.assign(globalThis, { FileReader: MockFileReader, Image: MockImage });

    try {
      const file = new File(['mock'], 'test.png', { type: 'image/png' });
      const result = await compressAvatarToWebpDataUri(file);

      expect(result).toBe('data:image/webp;base64,mockavataruri');
      expect(mockGetContext).toHaveBeenCalledWith('2d');
      // minSide = 100, sx = (200 - 100)/2 = 50, sy = 0 -> center square crop
      expect(mockDrawImage).toHaveBeenCalledWith(
        expect.any(MockImage),
        50,
        0,
        100,
        100,
        0,
        0,
        96,
        96
      );
      expect(mockToDataURL).toHaveBeenCalledWith('image/webp', 0.65);
    } finally {
      globalThis.FileReader = originalFileReader;
      globalThis.Image = originalImage;
      vi.restoreAllMocks();
    }
  });

  it('compresses ad image with aspect ratio scaling and maxBytes limit', async () => {
    const mockDrawImage = vi.fn();
    const mockToDataURL = vi.fn().mockReturnValue('data:image/webp;base64,mockaduri');
    const mockGetContext = vi.fn().mockReturnValue({
      drawImage: mockDrawImage,
    });

    vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
      if (tagName === 'canvas') {
        return {
          width: 0,
          height: 0,
          getContext: mockGetContext,
          toDataURL: mockToDataURL,
        } as unknown as HTMLCanvasElement;
      }
      return document.createElement(tagName);
    });

    const originalFileReader = globalThis.FileReader;
    const originalImage = globalThis.Image;

    class MockFileReader {
      result: string = 'data:image/jpeg;base64,dummyrawdata';
      onload: (() => void) | null = null;
      readAsDataURL() {
        setTimeout(() => this.onload?.(), 0);
      }
    }
    class MockImage {
      width: number = 1600;
      height: number = 1200;
      naturalWidth: number = 1600;
      naturalHeight: number = 1200;
      onload: (() => void) | null = null;
      set src(_val: string) {
        setTimeout(() => this.onload?.(), 0);
      }
    }

    Object.assign(globalThis, { FileReader: MockFileReader, Image: MockImage });

    try {
      const file = new File(['mock'], 'ad.png', { type: 'image/png' });
      const result = await compressAdImageToWebpDataUri(file, 102400);

      expect(result).toBe('data:image/webp;base64,mockaduri');
      expect(mockGetContext).toHaveBeenCalledWith('2d');
      // Scaled down to max 800: ratio = 800/1600 = 0.5 -> 800 x 600
      expect(mockDrawImage).toHaveBeenCalledWith(
        expect.any(MockImage),
        0,
        0,
        800,
        600
      );
      expect(mockToDataURL).toHaveBeenCalledWith('image/webp', 0.78);
    } finally {
      globalThis.FileReader = originalFileReader;
      globalThis.Image = originalImage;
      vi.restoreAllMocks();
    }
  });

  it('falls back to JPEG for avatar when WebP canvas export is unsupported (e.g. WebKitGTK)', async () => {
    const mockDrawImage = vi.fn();
    // Simulate WebKitGTK: toDataURL('image/webp') returns PNG, but toDataURL('image/jpeg') works
    const mockToDataURL = vi.fn().mockImplementation((type: string) => {
      if (type === 'image/webp') {
        return 'data:image/png;base64,unsupported_png_fallback_string';
      }
      if (type === 'image/jpeg') {
        return 'data:image/jpeg;base64,valid_jpeg_avatar';
      }
      return 'data:image/png;base64,fallback';
    });
    const mockGetContext = vi.fn().mockReturnValue({
      drawImage: mockDrawImage,
    });

    vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
      if (tagName === 'canvas') {
        return {
          width: 0,
          height: 0,
          getContext: mockGetContext,
          toDataURL: mockToDataURL,
        } as unknown as HTMLCanvasElement;
      }
      return document.createElement(tagName);
    });

    const originalFileReader = globalThis.FileReader;
    const originalImage = globalThis.Image;

    class MockFileReader {
      result: string = 'data:image/jpeg;base64,dummyrawdata';
      onload: (() => void) | null = null;
      readAsDataURL() {
        setTimeout(() => this.onload?.(), 0);
      }
    }
    class MockImage {
      width: number = 200;
      height: number = 200;
      naturalWidth: number = 200;
      naturalHeight: number = 200;
      onload: (() => void) | null = null;
      set src(_val: string) {
        setTimeout(() => this.onload?.(), 0);
      }
    }

    Object.assign(globalThis, { FileReader: MockFileReader, Image: MockImage });

    try {
      const file = new File(['mock'], 'test.png', { type: 'image/png' });
      const result = await compressAvatarToWebpDataUri(file);

      expect(result).toBe('data:image/jpeg;base64,valid_jpeg_avatar');
      expect(result.startsWith('data:image/jpeg;base64,')).toBe(true);
      expect(result.length).toBeLessThanOrEqual(4096);
    } finally {
      globalThis.FileReader = originalFileReader;
      globalThis.Image = originalImage;
      vi.restoreAllMocks();
    }
  });

  it('downscales ad image dimensions when initial resolution exceeds maxBytes', async () => {
    const mockDrawImage = vi.fn();
    // Simulate oversized output on first scale, fits on second scale
    let callCount = 0;
    const mockToDataURL = vi.fn().mockImplementation(() => {
      callCount++;
      if (callCount <= 6) {
        // Return string that exceeds 102400 bytes (e.g. 150000 base64 chars)
        return 'data:image/webp;base64,' + 'A'.repeat(150000);
      }
      return 'data:image/webp;base64,fits_now';
    });
    const mockGetContext = vi.fn().mockReturnValue({
      drawImage: mockDrawImage,
    });

    vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
      if (tagName === 'canvas') {
        return {
          width: 0,
          height: 0,
          getContext: mockGetContext,
          toDataURL: mockToDataURL,
        } as unknown as HTMLCanvasElement;
      }
      return document.createElement(tagName);
    });

    const originalFileReader = globalThis.FileReader;
    const originalImage = globalThis.Image;

    class MockFileReader {
      result: string = 'data:image/jpeg;base64,dummyrawdata';
      onload: (() => void) | null = null;
      readAsDataURL() {
        setTimeout(() => this.onload?.(), 0);
      }
    }
    class MockImage {
      width: number = 800;
      height: number = 800;
      naturalWidth: number = 800;
      naturalHeight: number = 800;
      onload: (() => void) | null = null;
      set src(_val: string) {
        setTimeout(() => this.onload?.(), 0);
      }
    }

    Object.assign(globalThis, { FileReader: MockFileReader, Image: MockImage });

    try {
      const file = new File(['mock'], 'ad.png', { type: 'image/png' });
      const result = await compressAdImageToWebpDataUri(file, 102400);

      expect(result).toBe('data:image/webp;base64,fits_now');
      // mockDrawImage should be called multiple times with downscaled dimensions
      expect(mockDrawImage).toHaveBeenCalledTimes(2);
      expect(mockDrawImage).toHaveBeenNthCalledWith(
        1,
        expect.any(MockImage),
        0,
        0,
        800,
        800
      );
      expect(mockDrawImage).toHaveBeenNthCalledWith(
        2,
        expect.any(MockImage),
        0,
        0,
        640,
        640
      );
    } finally {
      globalThis.FileReader = originalFileReader;
      globalThis.Image = originalImage;
      vi.restoreAllMocks();
    }
  });
});
