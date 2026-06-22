// src/hooks/useWindowTitleSync.ts
import { useEffect } from 'react';
import { logger } from '../utils/log';

export function useWindowTitleSync(profileName: string) {
    useEffect(() => {
        const updateTitle = async () => {
            try {
                const { getCurrentWindow } = await import("@tauri-apps/api/window");
                const win = getCurrentWindow();
                if (profileName) {
                    await win.setTitle(`Human Money App - ${profileName}`);
                } else {
                    await win.setTitle('Human Money App');
                }
            } catch (e) {
                logger.warn(`Failed to update window title: ${e}`);
            }
        };

        updateTitle();
    }, [profileName]);
}
