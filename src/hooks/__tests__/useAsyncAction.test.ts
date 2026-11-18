import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { useAsyncAction } from '../useAsyncAction';

describe('useAsyncAction', () => {
    it('tracks running state and returns result for ad-hoc actions', async () => {
        const { result } = renderHook(() => useAsyncAction());

        expect(result.current.isRunning).toBe(false);
        expect(result.current.error).toBeNull();

        let resolveAction!: (val: string) => void;
        let promise!: Promise<string | undefined>;

        act(() => {
            promise = result.current.execute(
                () =>
                    new Promise<string>((resolve) => {
                        resolveAction = resolve;
                    })
            );
        });

        expect(result.current.isRunning).toBe(true);

        let returned: string | undefined;
        await act(async () => {
            resolveAction('ok');
            returned = await promise;
        });

        expect(returned).toBe('ok');
        expect(result.current.isRunning).toBe(false);
        expect(result.current.error).toBeNull();
    });

    it('captures formatted errors and supports clearError()', async () => {
        const { result } = renderHook(() =>
            useAsyncAction(
                async (shouldFail: boolean) => {
                    if (shouldFail) {
                        throw new Error('boom');
                    }
                    return 42;
                },
                { formatError: (err) => `Formatted: ${(err as Error).message}` }
            )
        );

        let out: number | undefined;
        await act(async () => {
            out = await result.current.execute(true);
        });

        expect(out).toBeUndefined();
        expect(result.current.isRunning).toBe(false);
        expect(result.current.error).toBe('Formatted: boom');

        act(() => {
            result.current.clearError();
        });
        expect(result.current.error).toBeNull();

        await act(async () => {
            out = await result.current.execute(false);
        });
        expect(out).toBe(42);
        expect(result.current.error).toBeNull();
    });

    it('prevents concurrent overlapping runs by default and skips updates after unmount', async () => {
        const action = vi.fn(
            () =>
                new Promise<string>((resolve) => {
                    setTimeout(() => resolve('done'), 10);
                })
        );

        const { result, unmount } = renderHook(() => useAsyncAction());

        let first!: Promise<string | undefined>;
        let second!: Promise<string | undefined>;

        act(() => {
            first = result.current.execute(action);
            second = result.current.execute(action);
        });

        expect(action).toHaveBeenCalledTimes(1);
        await expect(second).resolves.toBeUndefined();

        unmount();
        await expect(first).resolves.toBe('done');
    });
});
