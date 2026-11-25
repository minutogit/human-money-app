import { describe, it, expect, vi } from 'vitest';
import {
  estimateDecodedByteSize,
  isWebpCanvasSupported,
  compressAvatarToWebpDataUri,
  compressAdImageToWebpDataUri,
} from '../imageCompression';

describe('imageCompression utils', () => {
  interface CompressionMockOptions {
    imgWidth?: number;
    imgHeight?: number;
    naturalWidth?: number;
    naturalHeight?: number;
    toDataURL: (type?: string, quality?: number) => string;
    fileReaderError?: boolean;
    imageError?: boolean;
    nullContext?: boolean;
  }

  async function withCompressionMocks<T>(
    opts: CompressionMockOptions,
    fn: (spies: {
      drawImage: ReturnType<typeof vi.fn>;
      getContext: ReturnType<typeof vi.fn>;
      toDataURL: ReturnType<typeof vi.fn>;
      MockImageCtor: unknown;
    }) => Promise<T>
  ): Promise<T> {
    const drawImage = vi.fn();
    const toDataURL = vi.fn(opts.toDataURL);
    const getContext = vi.fn().mockReturnValue(opts.nullContext ? null : { drawImage });
    const origCreateElement = document.createElement.bind(document);

    vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
      if (tagName === 'canvas') {
        return {
          width: 0,
          height: 0,
          getContext,
          toDataURL,
        } as unknown as HTMLCanvasElement;
      }
      return origCreateElement(tagName);
    });

    const originalFileReader = globalThis.FileReader;
    const originalImage = globalThis.Image;

    class MockFileReader {
      result: string = 'data:image/jpeg;base64,dummyrawdata';
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      readAsDataURL() {
        void Promise.resolve().then(() => {
          if (opts.fileReaderError) {
            this.onerror?.();
          } else {
            this.onload?.();
          }
        });
      }
    }

    class MockImage {
      width: number = opts.imgWidth ?? 200;
      height: number = opts.imgHeight ?? 100;
      naturalWidth: number = opts.naturalWidth ?? opts.imgWidth ?? 200;
      naturalHeight: number = opts.naturalHeight ?? opts.imgHeight ?? 100;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      set src(_val: string) {
        void Promise.resolve().then(() => {
          if (opts.imageError) {
            this.onerror?.();
          } else {
            this.onload?.();
          }
        });
      }
    }

    Object.assign(globalThis, { FileReader: MockFileReader, Image: MockImage });

    try {
      return await fn({ drawImage, getContext, toDataURL, MockImageCtor: MockImage });
    } finally {
      globalThis.FileReader = originalFileReader;
      globalThis.Image = originalImage;
      vi.restoreAllMocks();
    }
  }

  it('correctly estimates decoded byte size from data URI and raw base64', () => {
    // "Hello World" in base64: SGVsbG8gV29ybGQ= (11 bytes)
    const uriWithPadding1 = 'data:image/webp;base64,SGVsbG8gV29ybGQ=';
    expect(estimateDecodedByteSize(uriWithPadding1)).toBe(11);

    // "Hello" in base64: SGVsbG8= (5 bytes, 2 padding characters ==)
    const uriWithPadding2 = 'data:image/webp;base64,SGVsbG8=';
    expect(estimateDecodedByteSize(uriWithPadding2)).toBe(5);

    // "123456" in base64: MTIzNDU2 (6 bytes, 0 padding)
    const uriWithoutPadding = 'data:image/webp;base64,MTIzNDU2';
    expect(estimateDecodedByteSize(uriWithoutPadding)).toBe(6);

    // Raw base64 without comma prefix & empty input
    expect(estimateDecodedByteSize('MTIzNDU2')).toBe(6);
    expect(estimateDecodedByteSize('')).toBe(0);
  });

  it('detects WebP canvas support accurately including toDataURL exceptions', () => {
    const mockToDataURL = vi.fn().mockReturnValue('data:image/webp;base64,dummy');
    const origCreateElement = document.createElement.bind(document);
    vi.spyOn(document, 'createElement').mockImplementation((tagName: string) => {
      if (tagName === 'canvas') {
        return {
          width: 0,
          height: 0,
          toDataURL: mockToDataURL,
        } as unknown as HTMLCanvasElement;
      }
      return origCreateElement(tagName);
    });

    try {
      expect(isWebpCanvasSupported()).toBe(true);

      mockToDataURL.mockReturnValue('data:image/png;base64,dummy');
      expect(isWebpCanvasSupported()).toBe(false);

      mockToDataURL.mockImplementation(() => {
        throw new Error('canvas blocked');
      });
      expect(isWebpCanvasSupported()).toBe(false);
    } finally {
      vi.restoreAllMocks();
    }
  });

  it('compresses avatar to 96x96 WebP and limits length <= 4096', async () => {
    await withCompressionMocks(
      {
        imgWidth: 200,
        imgHeight: 100,
        toDataURL: () => 'data:image/webp;base64,mockavataruri',
      },
      async ({ drawImage, getContext, toDataURL, MockImageCtor }) => {
        const file = new File(['mock'], 'test.png', { type: 'image/png' });
        const result = await compressAvatarToWebpDataUri(file);

        expect(result).toBe('data:image/webp;base64,mockavataruri');
        expect(getContext).toHaveBeenCalledWith('2d');
        // minSide = 100, sx = (200 - 100)/2 = 50, sy = 0 -> center square crop
        expect(drawImage).toHaveBeenCalledWith(
          expect.any(MockImageCtor),
          50,
          0,
          100,
          100,
          0,
          0,
          96,
          96
        );
        expect(toDataURL).toHaveBeenCalledWith('image/webp', 0.65);
      }
    );
  });

  it('compresses ad image with aspect ratio scaling and maxBytes limit', async () => {
    await withCompressionMocks(
      {
        imgWidth: 1600,
        imgHeight: 1200,
        toDataURL: () => 'data:image/webp;base64,mockaduri',
      },
      async ({ drawImage, getContext, toDataURL, MockImageCtor }) => {
        const file = new File(['mock'], 'ad.png', { type: 'image/png' });
        const result = await compressAdImageToWebpDataUri(file, 102400);

        expect(result).toBe('data:image/webp;base64,mockaduri');
        expect(getContext).toHaveBeenCalledWith('2d');
        // Scaled down to max 800: ratio = 800/1600 = 0.5 -> 800 x 600
        expect(drawImage).toHaveBeenCalledWith(
          expect.any(MockImageCtor),
          0,
          0,
          800,
          600
        );
        expect(toDataURL).toHaveBeenCalledWith('image/webp', 0.78);
      }
    );
  });

  it('falls back to JPEG for avatar when WebP canvas export is unsupported (e.g. WebKitGTK)', async () => {
    await withCompressionMocks(
      {
        imgWidth: 200,
        imgHeight: 200,
        toDataURL: (type?: string) => {
          if (type === 'image/webp') {
            return 'data:image/png;base64,unsupported_png_fallback_string';
          }
          if (type === 'image/jpeg') {
            return 'data:image/jpeg;base64,valid_jpeg_avatar';
          }
          return 'data:image/png;base64,fallback';
        },
      },
      async () => {
        const file = new File(['mock'], 'test.png', { type: 'image/png' });
        const result = await compressAvatarToWebpDataUri(file);

        expect(result).toBe('data:image/jpeg;base64,valid_jpeg_avatar');
        expect(result.startsWith('data:image/jpeg;base64,')).toBe(true);
        expect(result.length).toBeLessThanOrEqual(4096);
      }
    );
  });

  it('downscales ad image dimensions when initial resolution exceeds maxBytes', async () => {
    let callCount = 0;
    await withCompressionMocks(
      {
        imgWidth: 800,
        imgHeight: 800,
        toDataURL: () => {
          callCount++;
          if (callCount <= 6) {
            return 'data:image/webp;base64,' + 'A'.repeat(150000);
          }
          return 'data:image/webp;base64,fits_now';
        },
      },
      async ({ drawImage, MockImageCtor }) => {
        const file = new File(['mock'], 'ad.png', { type: 'image/png' });
        const result = await compressAdImageToWebpDataUri(file, 102400);

        expect(result).toBe('data:image/webp;base64,fits_now');
        expect(drawImage).toHaveBeenCalledTimes(2);
        expect(drawImage).toHaveBeenNthCalledWith(1, expect.any(MockImageCtor), 0, 0, 800, 800);
        expect(drawImage).toHaveBeenNthCalledWith(2, expect.any(MockImageCtor), 0, 0, 640, 640);
      }
    );
  });

  it('rejects cleanly on FileReader error, Image error, and unavailable 2D context', async () => {
    const file = new File(['mock'], 'broken.png', { type: 'image/png' });

    await expect(
      withCompressionMocks(
        { fileReaderError: true, toDataURL: () => 'data:image/webp;base64,ok' },
        () => compressAvatarToWebpDataUri(file)
      )
    ).rejects.toThrow('Failed to read file');

    await expect(
      withCompressionMocks(
        { imageError: true, toDataURL: () => 'data:image/webp;base64,ok' },
        () => compressAdImageToWebpDataUri(file)
      )
    ).rejects.toThrow('Failed to load image');

    await expect(
      withCompressionMocks(
        { nullContext: true, toDataURL: () => 'data:image/webp;base64,ok' },
        () => compressAvatarToWebpDataUri(file)
      )
    ).rejects.toThrow('Canvas 2D context not available');

    await expect(
      withCompressionMocks(
        { nullContext: true, toDataURL: () => 'data:image/webp;base64,ok' },
        () => compressAdImageToWebpDataUri(file)
      )
    ).rejects.toThrow('Canvas 2D context not available');
  });

  it('handles portrait avatar crop, WebP quality retries, and JPEG fallback when WebP exceeds 4096 chars', async () => {
    await withCompressionMocks(
      {
        imgWidth: 100,
        imgHeight: 200,
        toDataURL: (type?: string, quality?: number) => {
          if (type === 'image/webp' && quality === undefined) return 'data:image/webp;base64,ok';
          if (type === 'image/webp' && quality === 0.65) {
            return 'data:image/webp;base64,' + 'A'.repeat(4100);
          }
          if (type === 'image/webp' && quality === 0.5) {
            return 'data:image/webp;base64,small_enough';
          }
          return 'data:image/webp;base64,' + 'B'.repeat(4100);
        },
      },
      async ({ drawImage, MockImageCtor }) => {
        const res = await compressAvatarToWebpDataUri(new File(['mock'], 'p.png', { type: 'image/png' }));
        expect(res).toBe('data:image/webp;base64,small_enough');
        expect(drawImage).toHaveBeenCalledWith(expect.any(MockImageCtor), 0, 50, 100, 100, 0, 0, 96, 96);
      }
    );

    await withCompressionMocks(
      {
        imgWidth: 100,
        imgHeight: 100,
        toDataURL: (type?: string, quality?: number) => {
          if (type === 'image/webp' && quality === undefined) return 'data:image/webp;base64,ok';
          if (type === 'image/webp') return 'data:image/webp;base64,' + 'W'.repeat(4100);
          return 'data:image/jpeg;base64,jpeg_fallback_ok';
        },
      },
      async () => {
        const res = await compressAvatarToWebpDataUri(new File(['mock'], 'big.png', { type: 'image/png' }));
        expect(res).toBe('data:image/jpeg;base64,jpeg_fallback_ok');
      }
    );
  });

  it('enforces exact maxBytes - 64 threshold, JPEG ad fallback without upscaling, and 96px floor after 5 attempts', async () => {
    // 1. Exact threshold boundary (maxBytes = 100 -> threshold = 36 bytes = 48 base64 chars; 48 -> pass, 52 chars = 39B -> retry)
    const fortyBytesB64 = 'A'.repeat(56); // 42 bytes (> 36 threshold)
    const twentyFourBytesB64 = 'B'.repeat(32); // 24 bytes (<= 36 threshold)
    await withCompressionMocks(
      {
        imgWidth: 400,
        imgHeight: 300,
        toDataURL: (type?: string, quality?: number) => {
          if (type === 'image/webp') return 'data:image/png;base64,unsupported';
          if (quality === 0.78) return `data:image/jpeg;base64,${fortyBytesB64}`;
          return `data:image/jpeg;base64,${twentyFourBytesB64}`;
        },
      },
      async ({ drawImage, toDataURL, MockImageCtor }) => {
        const res = await compressAdImageToWebpDataUri(new File(['mock'], 'small.png', { type: 'image/png' }), 100);
        expect(res).toBe(`data:image/jpeg;base64,${twentyFourBytesB64}`);
        // Does not upscale 400x300
        expect(drawImage).toHaveBeenCalledTimes(1);
        expect(drawImage).toHaveBeenCalledWith(expect.any(MockImageCtor), 0, 0, 400, 300);
        expect(toDataURL).toHaveBeenNthCalledWith(2, 'image/jpeg', 0.78);
        expect(toDataURL).toHaveBeenNthCalledWith(3, 'image/jpeg', 0.65);
      }
    );

    // 2. Always oversized -> caps at 5 scale attempts and floors dimension to 96px
    await withCompressionMocks(
      {
        imgWidth: 150,
        imgHeight: 150,
        toDataURL: () => 'data:image/webp;base64,' + 'Z'.repeat(500),
      },
      async ({ drawImage, MockImageCtor }) => {
        await compressAdImageToWebpDataUri(new File(['mock'], 'giant.png', { type: 'image/png' }), 100);
        expect(drawImage).toHaveBeenCalledTimes(5);
        expect(drawImage).toHaveBeenNthCalledWith(1, expect.any(MockImageCtor), 0, 0, 150, 150);
        expect(drawImage).toHaveBeenNthCalledWith(2, expect.any(MockImageCtor), 0, 0, 120, 120);
        expect(drawImage).toHaveBeenNthCalledWith(3, expect.any(MockImageCtor), 0, 0, 96, 96);
        expect(drawImage).toHaveBeenNthCalledWith(4, expect.any(MockImageCtor), 0, 0, 96, 96);
        expect(drawImage).toHaveBeenNthCalledWith(5, expect.any(MockImageCtor), 0, 0, 96, 96);
      }
    );
  });
});
