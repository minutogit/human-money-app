// src/context/UiModeContext.tsx
import { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import { UiMode } from '../types';
import { useOptionalSession } from './SessionContext';
import { settingsService } from '../services/settingsService';
import { tourBridge } from '../utils/tourBridge';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';

export interface UiModeContextValue {
    mode: UiMode;
    isExpert: boolean;
    setMode(mode: UiMode): void;
    toggle(): void;
}

const UiModeContext = createContext<UiModeContextValue | null>(null);

function resolveDefaultUiMode(initialMode: UiMode): UiMode {
    const tourMode = tourBridge.getState().uiMode;
    if (tourMode === 'simple' || tourMode === 'expert') {
        return tourMode;
    }
    return initialMode;
}

export function UiModeProvider({
    children,
    initialMode = 'simple',
}: {
    children: ReactNode;
    initialMode?: UiMode;
}) {
    const session = useOptionalSession();
    const isAuthenticated = session?.isAuthenticated ?? false;
    const hasCachedSession = session?.hasCachedSession ?? false;

    const [mode, setModeState] = useState<UiMode>(() => resolveDefaultUiMode(initialMode));

    useEffect(() => {
        tourBridge.registerUiModeSetter((nextMode) => {
            setModeState(nextMode);
        });
        return () => {
            tourBridge.registerUiModeSetter(null);
        };
    }, []);

    useEffect(() => {
        if (!isAuthenticated) {
            setModeState(resolveDefaultUiMode(initialMode));
            return;
        }
        // Avoid overwriting tour mode when driven by tourBridge
        if (tourBridge.getState().uiMode) return;

        let active = true;
        settingsService
            .getConfig()
            .then((config) => {
                if (!active) return;
                const persisted = config?.uiPreferences?.uiMode;
                if (persisted === 'simple' || persisted === 'expert') {
                    setModeState(persisted);
                }
            })
            .catch((e) => {
                logger.warn(`Failed to load uiMode from config: ${stringifyError(e)}`);
            });

        return () => {
            active = false;
        };
    }, [isAuthenticated, initialMode]);

    const setMode = useCallback(
        (nextMode: UiMode) => {
            setModeState(nextMode);

            if (!isAuthenticated || !hasCachedSession || tourBridge.getState().uiMode) return;
            settingsService.patchUiPreferences({ uiMode: nextMode }).catch((e) => {
                logger.warn(`Failed to persist uiMode to preferences: ${stringifyError(e)}`);
            });
        },
        [isAuthenticated, hasCachedSession],
    );

    const toggle = useCallback(() => {
        setMode(mode === 'expert' ? 'simple' : 'expert');
    }, [mode, setMode]);

    return (
        <UiModeContext.Provider
            value={{
                mode,
                isExpert: mode === 'expert',
                setMode,
                toggle,
            }}
        >
            {children}
        </UiModeContext.Provider>
    );
}

export function useUiMode(): UiModeContextValue {
    const context = useContext(UiModeContext);
    const [fallbackMode, setFallbackMode] = useState<UiMode>('simple');

    const setFallback = useCallback((nextMode: UiMode) => {
        setFallbackMode(nextMode);
    }, []);

    const toggleFallback = useCallback(() => {
        setFallbackMode((prev) => (prev === 'expert' ? 'simple' : 'expert'));
    }, []);

    if (context) {
        return context;
    }

    return {
        mode: fallbackMode,
        isExpert: fallbackMode === 'expert',
        setMode: setFallback,
        toggle: toggleFallback,
    };
}
