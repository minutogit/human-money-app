// src/context/UiModeContext.tsx
import { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import { UiMode } from '../types';
import { useOptionalSession } from './SessionContext';
import { settingsService } from '../services/settingsService';
import { tourBridge } from '../utils/tourBridge';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';

export const UI_MODE_STORAGE_KEY = 'humoco_ui_mode';
const FALLBACK_MODE_EVENT = 'humoco:ui-mode-fallback-change';

export interface UiModeContextValue {
    mode: UiMode;
    isExpert: boolean;
    setMode(mode: UiMode): void;
    toggle(): void;
}

const UiModeContext = createContext<UiModeContextValue | null>(null);

function readStoredUiMode(): UiMode {
    try {
        return localStorage.getItem(UI_MODE_STORAGE_KEY) === 'expert' ? 'expert' : 'simple';
    } catch {
        return 'simple';
    }
}

function writeStoredUiMode(mode: UiMode): void {
    try {
        localStorage.setItem(UI_MODE_STORAGE_KEY, mode);
    } catch {
        // Ignore storage errors in restricted environments
    }
}

export function UiModeProvider({ children }: { children: ReactNode }) {
    const session = useOptionalSession();
    const isAuthenticated = session?.isAuthenticated ?? false;
    const hasCachedSession = session?.hasCachedSession ?? false;

    const [mode, setModeState] = useState<UiMode>(() => {
        const tourMode = tourBridge.getState().uiMode;
        if (tourMode === 'simple' || tourMode === 'expert') {
            return tourMode;
        }
        return readStoredUiMode();
    });

    useEffect(() => {
        tourBridge.registerUiModeSetter((nextMode) => {
            setModeState(nextMode);
            writeStoredUiMode(nextMode);
        });
        return () => {
            tourBridge.registerUiModeSetter(null);
        };
    }, []);

    useEffect(() => {
        if (!isAuthenticated) return;
        // Avoid overwriting tour mode when driven by tourBridge
        if (tourBridge.getState().uiMode) return;

        let active = true;
        settingsService.getSettings()
            .then((settings) => {
                if (!active || !settings) return;
                const persisted = settings.uiMode;
                if (persisted === 'simple' || persisted === 'expert') {
                    setModeState(persisted);
                    writeStoredUiMode(persisted);
                }
            })
            .catch((e) => {
                logger.warn(`Failed to load uiMode from settings: ${stringifyError(e)}`);
            });

        return () => {
            active = false;
        };
    }, [isAuthenticated]);

    const setMode = useCallback((nextMode: UiMode) => {
        setModeState(nextMode);
        writeStoredUiMode(nextMode);

        if (!isAuthenticated || !hasCachedSession || tourBridge.getState().uiMode) return;
        settingsService.getSettings()
            .then((current) => {
                return settingsService.saveSettings({ ...current, uiMode: nextMode });
            })
            .catch((e) => {
                logger.warn(`Failed to persist uiMode to settings: ${stringifyError(e)}`);
            });
    }, [isAuthenticated, hasCachedSession]);

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
    const [fallbackMode, setFallbackMode] = useState<UiMode>(readStoredUiMode);

    useEffect(() => {
        if (context) return;
        const handleSync = () => setFallbackMode(readStoredUiMode());
        window.addEventListener(FALLBACK_MODE_EVENT, handleSync);
        return () => window.removeEventListener(FALLBACK_MODE_EVENT, handleSync);
    }, [context]);

    const setFallback = useCallback((nextMode: UiMode) => {
        setFallbackMode(nextMode);
        writeStoredUiMode(nextMode);
        window.dispatchEvent(new CustomEvent(FALLBACK_MODE_EVENT));
    }, []);

    const toggleFallback = useCallback(() => {
        const next: UiMode = readStoredUiMode() === 'expert' ? 'simple' : 'expert';
        setFallback(next);
    }, [setFallback]);

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
