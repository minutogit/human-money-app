/// <reference types="vite/client" />

export interface HumocoTourState {
    appState?: import('./types').AppState;
    isAuthenticated?: boolean;
    isSessionActive?: boolean;
    isForkLocked?: boolean;
    isRecoveryRequired?: boolean;
    settingsTab?: 'profile' | 'security_backup' | 'sync_network' | 'standards_general';
    profileName?: string;
}

export interface HumocoTourBridge {
    setState: (partial: HumocoTourState) => void;
    getState: () => HumocoTourState;
    subscribe: (listener: (state: HumocoTourState) => void) => () => void;
    registerSettingsTabSetter: (setter: ((tab: 'profile' | 'security_backup' | 'sync_network' | 'standards_general') => void) | null) => void;
}

declare global {
    interface Window {
        __HUMOCO_TOUR__?: HumocoTourBridge;
    }
}
