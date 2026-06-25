// src/hooks/useWindowTitleSync.ts
import { useEffect } from 'react';
import { logger } from '../utils/log';
import { stringifyError } from '../utils/errorHelper';

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
                logger.warn(`Failed to update window title: ${stringifyError(e)}`);
            }
        };

        updateTitle();
    }, [profileName]);
}
