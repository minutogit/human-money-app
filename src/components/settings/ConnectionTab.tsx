// src/components/settings/ConnectionTab.tsx
import React from 'react';
import { AppSettings } from '../../types';
import { AutoSyncSummaryCard } from './AutoSyncSummaryCard';
import { SyncServiceSettingsCard } from './SyncServiceSettingsCard';

export interface ConnectionTabProps {
    settings: AppSettings | null;
    onPersistSettings: (updater: (prev: AppSettings | null) => AppSettings | null) => void;
    onSettingsSynced?: (newSettings: AppSettings) => void;
}

export const ConnectionTab: React.FC<ConnectionTabProps> = ({
    settings,
    onPersistSettings,
    onSettingsSynced,
}) => {
    return (
        <div
            id="settings-sync-section"
            className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500"
        >
            <AutoSyncSummaryCard
                settings={settings}
                onPersistSettings={onPersistSettings}
                onSettingsSynced={onSettingsSynced}
                showQuickConnect={false}
            />
            <div id="settings-connection-section">
                <SyncServiceSettingsCard
                    settings={settings}
                    onSettingsUpdated={onSettingsSynced}
                />
            </div>
        </div>
    );
};

export default ConnectionTab;
