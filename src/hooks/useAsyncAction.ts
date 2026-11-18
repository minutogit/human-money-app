// src/hooks/useAsyncAction.ts
import { useState, useRef, useEffect, useCallback } from 'react';
import { stringifyError } from '../utils/errorHelper';

export interface UseAsyncActionOptions {
    /** Custom error formatter; defaults to stringifyError. */
    formatError?: (error: unknown) => string;
    /** Allow overlapping executions while one is already running (defaults to false). */
    allowConcurrent?: boolean;
}

export interface UseAsyncActionResult<TArgs extends unknown[], TResult> {
    execute: (...args: TArgs) => Promise<TResult | undefined>;
    isRunning: boolean;
    error: string | null;
    clearError: () => void;
}

type AsyncActionFn = (...args: never[]) => Promise<unknown>;

export interface AdHocAsyncActionResult {
    execute: <T>(action: () => Promise<T>) => Promise<T | undefined>;
    isRunning: boolean;
    error: string | null;
    clearError: () => void;
}

/**
 * Wraps asynchronous actions with running state, formatted error tracking,
 * concurrency protection, and unmount-safe state updates.
 */
export function useAsyncAction<
    TFirst extends AsyncActionFn | UseAsyncActionOptions | undefined = undefined
>(
    actionOrOptions?: TFirst,
    maybeOptions?: UseAsyncActionOptions
): TFirst extends (...args: infer TArgs) => Promise<infer TResult>
    ? UseAsyncActionResult<TArgs, TResult>
    : AdHocAsyncActionResult {
    const boundAction: AsyncActionFn | undefined =
        typeof actionOrOptions === 'function' ? (actionOrOptions as AsyncActionFn) : undefined;
    const options: UseAsyncActionOptions | undefined =
        typeof actionOrOptions === 'function'
            ? maybeOptions
            : (actionOrOptions as UseAsyncActionOptions | undefined);

    const [isRunning, setIsRunning] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const isRunningRef = useRef(false);
    const isMountedRef = useRef(true);
    const boundActionRef = useRef(boundAction);
    const optionsRef = useRef(options);

    boundActionRef.current = boundAction;
    optionsRef.current = options;

    useEffect(() => {
        isMountedRef.current = true;
        return () => {
            isMountedRef.current = false;
        };
    }, []);

    const clearError = useCallback(() => {
        if (isMountedRef.current) {
            setError(null);
        }
    }, []);

    const execute = useCallback(async (...args: unknown[]): Promise<unknown> => {
        const currentOptions = optionsRef.current;
        if (!currentOptions?.allowConcurrent && isRunningRef.current) {
            return undefined;
        }

        const runner = boundActionRef.current
            ? () => boundActionRef.current!(...(args as never[]))
            : (args[0] as (() => Promise<unknown>) | undefined);

        if (typeof runner !== 'function') {
            return undefined;
        }

        isRunningRef.current = true;
        if (isMountedRef.current) {
            setIsRunning(true);
            setError(null);
        }

        try {
            return await runner();
        } catch (err) {
            const formatted = currentOptions?.formatError
                ? currentOptions.formatError(err)
                : stringifyError(err);
            if (isMountedRef.current) {
                setError(formatted || 'An unexpected error occurred.');
            }
            return undefined;
        } finally {
            isRunningRef.current = false;
            if (isMountedRef.current) {
                setIsRunning(false);
            }
        }
    }, []);

    return {
        execute,
        isRunning,
        error,
        clearError,
    } as unknown as TFirst extends (...args: infer TArgs) => Promise<infer TResult>
        ? UseAsyncActionResult<TArgs, TResult>
        : AdHocAsyncActionResult;
}
