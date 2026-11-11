import { describe, it, expect, vi } from 'vitest';
import {
  estimateDecodedByteSize,
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

    (globalThis as any).FileReader = MockFileReader;
    (globalThis as any).Image = MockImage;

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

    (globalThis as any).FileReader = MockFileReader;
    (globalThis as any).Image = MockImage;

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
});
