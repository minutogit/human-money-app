// src/utils/tourBridge.ts
import { AppState } from '../types';

export type SettingsTabId = 'profile' | 'security' | 'connection' | 'advanced';

export interface HumocoTourState {
    appState?: AppState;
    isAuthenticated?: boolean;
    isSessionActive?: boolean;
    isForkLocked?: boolean;
    isRecoveryRequired?: boolean;
    settingsTab?: SettingsTabId;
    profileName?: string;
}

export type TourListener = (state: HumocoTourState) => void;

const PUBLIC_VIEWS = new Set([
    'loading',
    'needs_profile',
    'needs_login',
    'recreate_profile',
    'restore_backup',
    'needs_recovery',
    'help_center'
]);

class TourBridge {
    private state: HumocoTourState = {};
    private listeners = new Set<TourListener>();
    private settingsTabSetter: ((tab: SettingsTabId) => void) | null = null;

    getState(): HumocoTourState {
        return { ...this.state };
    }

    setState(partial: HumocoTourState): void {
        const nextPartial = { ...partial };

        // Automatically align authentication state for non-public views if not explicitly specified
        if (nextPartial.appState && nextPartial.isAuthenticated === undefined) {
            const isPublic = PUBLIC_VIEWS.has(nextPartial.appState.view);
            nextPartial.isAuthenticated = !isPublic;
            if (nextPartial.isSessionActive === undefined) {
                nextPartial.isSessionActive = !isPublic;
            }
        }

        this.state = { ...this.state, ...nextPartial };

        if (nextPartial.settingsTab && this.settingsTabSetter) {
            this.settingsTabSetter(nextPartial.settingsTab);
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
}

export const tourBridge = new TourBridge();

if (typeof window !== 'undefined') {
    (window as unknown as { __HUMOCO_TOUR__: TourBridge }).__HUMOCO_TOUR__ = tourBridge;
}
