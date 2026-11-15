// src/components/ui/ExpertTabsLayout.tsx
import React from 'react';
import { useUiMode } from '../../context/UiModeContext';

export interface ExpertTabsLayoutProps {
    simpleContent: React.ReactNode;
    expertContent: React.ReactNode;
    className?: string;
}

/**
 * Renders a unified single-page layout in simple mode and the full tabbed
 * layout in expert mode without exposing `useUiMode()` to view components.
 */
export const ExpertTabsLayout: React.FC<ExpertTabsLayoutProps> = ({
    simpleContent,
    expertContent,
    className,
}) => {
    const { isExpert } = useUiMode();

    return (
        <div
            className={className}
            data-testid={isExpert ? 'expert-tabs-layout' : 'simple-blocks-layout'}
        >
            {isExpert ? expertContent : simpleContent}
        </div>
    );
};

export default ExpertTabsLayout;
