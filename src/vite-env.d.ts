/// <reference types="vite/client" />

import type { HumocoTourBridge } from './utils/tourBridge';

export type { HumocoTourState, HumocoTourBridge } from './utils/tourBridge';

declare global {
    interface Window {
        __HUMOCO_TOUR__?: HumocoTourBridge;
    }
}
