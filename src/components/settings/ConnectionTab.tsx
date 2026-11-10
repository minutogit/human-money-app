// src/components/settings/ConnectionTab.tsx
import React from 'react';
import { AppSettings } from '../../types';
import { SyncServiceSettingsCard } from './SyncServiceSettingsCard';

export interface ConnectionTabProps {
    settings: AppSettings | null;
    onSettingsUpdated?: (newSettings: AppSettings) => void;
}

export const ConnectionTab: React.FC<ConnectionTabProps> = ({ settings, onSettingsUpdated }) => {
    return (
        <div id="settings-sync-section" className="space-y-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
            <div id="settings-connection-section">
                <SyncServiceSettingsCard
                    settings={settings}
                    onSettingsUpdated={onSettingsUpdated}
                    mode="consumer"
                />
            </div>
        </div>
    );
};

export default ConnectionTab;
