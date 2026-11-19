// src/utils/tourBridge.ts
import { AppState, UiMode, isPublicView } from '../types';

export type SettingsTabId = 'profile' | 'security' | 'connection' | 'advanced';

export interface HumocoTourState {
    appState?: AppState;
    isAuthenticated?: boolean;
    hasCachedSession?: boolean;
    isForkLocked?: boolean;
    isRecoveryRequired?: boolean;
    settingsTab?: SettingsTabId;
    profileName?: string;
    uiMode?: UiMode;
}

export type TourListener = (state: HumocoTourState) => void;

export interface HumocoTourBridge {
    setState: (partial: HumocoTourState) => void;
    getState: () => HumocoTourState;
    subscribe: (listener: TourListener) => () => void;
    registerSettingsTabSetter: (setter: ((tab: SettingsTabId) => void) | null) => void;
    registerUiModeSetter: (setter: ((mode: UiMode) => void) | null) => void;
    setUiMode: (mode: UiMode) => void;
}

class TourBridge {
    private state: HumocoTourState = {};
    private listeners = new Set<TourListener>();
    private settingsTabSetter: ((tab: SettingsTabId) => void) | null = null;
    private uiModeSetter: ((mode: UiMode) => void) | null = null;

    getState(): HumocoTourState {
        return { ...this.state };
    }

    setState(partial: HumocoTourState): void {
        const nextPartial = { ...partial };

        // Automatically align authentication state for non-public views if not explicitly specified
        if (nextPartial.appState && nextPartial.isAuthenticated === undefined) {
            const isPublic = isPublicView(nextPartial.appState.view);
            nextPartial.isAuthenticated = !isPublic;
            if (nextPartial.hasCachedSession === undefined) {
                nextPartial.hasCachedSession = !isPublic;
            }
        }

        // Reset locks by default on view transitions unless explicitly set
        if (nextPartial.appState) {
            if (nextPartial.isForkLocked === undefined) {
                nextPartial.isForkLocked = false;
            }
            if (nextPartial.isRecoveryRequired === undefined) {
                nextPartial.isRecoveryRequired = false;
            }
        }

        this.state = { ...this.state, ...nextPartial };

        if (nextPartial.settingsTab && this.settingsTabSetter) {
            this.settingsTabSetter(nextPartial.settingsTab);
        }

        if (nextPartial.uiMode && this.uiModeSetter) {
            this.uiModeSetter(nextPartial.uiMode);
        }

        for (const listener of this.listeners) {
            try {
                listener(nextPartial);
            } catch (err) {
                console.error('[TourBridge] Error in tour listener:', err);
            }
        }
    }

    subscribe(listener: TourListener): () => void {
        this.listeners.add(listener);
        return () => {
            this.listeners.delete(listener);
        };
    }

    registerSettingsTabSetter(setter: ((tab: SettingsTabId) => void) | null): void {
        this.settingsTabSetter = setter;
    }

    registerUiModeSetter(setter: ((mode: UiMode) => void) | null): void {
        this.uiModeSetter = setter;
    }

    setUiMode(mode: UiMode): void {
        this.setState({ uiMode: mode });
    }
}

export const tourBridge = new TourBridge();

if (typeof window !== 'undefined') {
    (window as unknown as { __HUMOCO_TOUR__: TourBridge }).__HUMOCO_TOUR__ = tourBridge;
}
