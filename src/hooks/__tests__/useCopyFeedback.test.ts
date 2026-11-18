import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { useCopyFeedback } from '../useCopyFeedback';
import * as clipboardUtils from '../../utils/clipboard';

vi.mock('../../utils/clipboard', () => ({
    copyToClipboard: vi.fn(),
}));

describe('useCopyFeedback', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
        vi.mocked(clipboardUtils.copyToClipboard).mockResolvedValue(true);
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it('sets copied state on success and resets after the configured delay', async () => {
        const { result } = renderHook(() => useCopyFeedback(1500));

        expect(result.current.copied).toBe(false);

        let success = false;
        await act(async () => {
            success = await result.current.copyText('humoco://pay?to=did:key:alice');
        });

        expect(success).toBe(true);
        expect(clipboardUtils.copyToClipboard).toHaveBeenCalledWith('humoco://pay?to=did:key:alice');
        expect(result.current.copied).toBe(true);

        act(() => {
            vi.advanceTimersByTime(1499);
        });
        expect(result.current.copied).toBe(true);

        act(() => {
            vi.advanceTimersByTime(1);
        });
        expect(result.current.copied).toBe(false);
    });

    it('returns false and stays uncopied when clipboard fails or value is empty', async () => {
        const { result } = renderHook(() => useCopyFeedback());

        let emptyRes = true;
        await act(async () => {
            emptyRes = await result.current.copyText('');
        });
        expect(emptyRes).toBe(false);
        expect(clipboardUtils.copyToClipboard).not.toHaveBeenCalled();
        expect(result.current.copied).toBe(false);

        vi.mocked(clipboardUtils.copyToClipboard).mockResolvedValueOnce(false);
        let failRes = true;
        await act(async () => {
            failRes = await result.current.copyText('abc');
        });
        expect(failRes).toBe(false);
        expect(result.current.copied).toBe(false);
    });

    it('clears pending reset timers cleanly on unmount and manual reset()', async () => {
        const { result, unmount } = renderHook(() => useCopyFeedback(2000));

        await act(async () => {
            await result.current.copyText('abc');
        });
        expect(result.current.copied).toBe(true);

        act(() => {
            result.current.reset();
        });
        expect(result.current.copied).toBe(false);

        await act(async () => {
            await result.current.copyText('def');
        });
        expect(result.current.copied).toBe(true);

        unmount();
        act(() => {
            vi.advanceTimersByTime(5000);
        });
    });
});
