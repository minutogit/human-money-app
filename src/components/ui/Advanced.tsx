// src/components/ui/Advanced.tsx
import React from 'react';
import { useUiMode } from '../../context/UiModeContext';

export interface AdvancedProps {
    children: React.ReactNode;
}

/**
 * Renders technical/expert UI elements only when Expert mode is active.
 */
export const Advanced: React.FC<AdvancedProps> = ({ children }) => {
    const { isExpert } = useUiMode();

    if (!isExpert) {
        return null;
    }

    return <>{children}</>;
};
