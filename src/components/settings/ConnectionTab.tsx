// src/components/settings/ConnectionTab.tsx
import React from 'react';
import { AppConfigDto, UiPreferencesPatch } from '../../types';
import { AutoSyncSummaryCard } from './AutoSyncSummaryCard';
import { SyncServiceSettingsCard } from './SyncServiceSettingsCard';

export interface ConnectionTabProps {
    config: AppConfigDto | null;
    onPatchUiPreferences: (patch: UiPreferencesPatch) => Promise<void> | void;
    onConfigUpdated?: (newConfig: AppConfigDto) => void;
}

export const ConnectionTab: React.FC<ConnectionTabProps> = ({
    config,
    onPatchUiPreferences,
    onConfigUpdated,
}) => {
    return (
        <div
            id="settings-sync-section"
            className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500"
        >
            <AutoSyncSummaryCard
                config={config}
                onPatchUiPreferences={onPatchUiPreferences}
                onConfigUpdated={onConfigUpdated}
                showQuickConnect={false}
            />
            <div id="settings-connection-section">
                <SyncServiceSettingsCard
                    config={config}
                    onConfigUpdated={onConfigUpdated}
                />
            </div>
        </div>
    );
};

export default ConnectionTab;
