// src/components/SettingsView.tsx
import { useState, useEffect, FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { AppSettings } from '../types';
import { useSession } from '../context/SessionContext';

interface SettingsViewProps {
    onBack: () => void;
}

export function SettingsView({ onBack }: SettingsViewProps) {
    const { protectAction } = useSession();
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [isLoading, setIsLoading] = useState(true);
    const [isSaving, setIsSaving] = useState(false);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');

    useEffect(() => {
        async function fetchSettings() {
            try {
                logger.info("SettingsView: Fetching current settings.");
                const currentSettings = await invoke<AppSettings>('get_app_settings');
                setSettings(currentSettings);
            } catch (e) {
                const msg = `Failed to load settings: ${e}`;
                logger.error(msg);
                setError(msg);
            } finally {
                setIsLoading(false);
            }
        }
        fetchSettings();
    }, []);

    const handleSave = async (event: FormEvent) => {
        event.preventDefault();
        if (!settings) {
            setError("Please fill in all fields.");
            return;
        }

        setIsSaving(true);
        setError('');
        setSuccess('');

        try {
            logger.info("Attempting to save settings...");
            await protectAction(async (password) => { await invoke('save_app_settings', { settings, password }); });
            setSuccess("Settings saved successfully!");
            logger.info("Settings saved successfully.");
            setTimeout(() => setSuccess(''), 3000); // Clear success message after 3s
        } catch (e) {
            const msg = `Failed to save settings: ${e}`;
            logger.error(msg);
            setError(msg);
        } finally {
            setIsSaving(false);
        }
    };

    if (isLoading) {
        return <p className="text-center">Loading settings...</p>;
    }

    return (
        <div className="max-w-2xl mx-auto">
            <header className="flex-shrink-0 mb-6">
                <div className="flex items-center justify-between">
                    <h1 className="text-2xl font-bold text-theme-primary">Settings</h1>
                    <Button variant="secondary" onClick={onBack}>Back to Dashboard</Button>
                </div>
                <p className="text-theme-light mt-1">Manage application settings.</p>
            </header>

            <form onSubmit={handleSave} className="space-y-6 bg-bg-card border border-theme-subtle p-6 rounded-lg">
                <div>
                    <label htmlFor="retention" className="block text-sm font-medium text-theme-light mb-1">
                        Transaction Bundle Retention (Days)
                    </label>
                    <Input
                        id="retention"
                        type="number"
                        value={settings?.bundle_retention_days ?? 30}
                        onChange={(e) => setSettings(s => s ? { ...s, bundle_retention_days: parseInt(e.target.value, 10) } : null)}
                        min="1"
                        required
                    />
                    <p className="text-xs text-theme-light mt-2">
                        How long the full data for sent transfers should be stored. After this period, only the metadata is kept.
                    </p>
                </div>

                <div>
                    <label htmlFor="sessionTimeout" className="block text-sm font-medium text-theme-light mb-1">
                        Session Timeout (Minutes)
                    </label>
                    <Input
                        id="sessionTimeout"
                        type="number"
                        value={settings ? Math.floor(settings.session_timeout_seconds / 60) : 10}
                        onChange={(e) => {
                            const minutes = parseInt(e.target.value, 10);
                            setSettings(s => s ? { ...s, session_timeout_seconds: isNaN(minutes) ? 0 : minutes * 60 } : null);
                        }}
                        min="0"
                        required
                    />
                    <p className="text-xs text-theme-light mt-2">
                        Duration to keep the wallet unlocked after activity. Set to 0 to always ask for password (High Security).
                    </p>
                </div>



                <div className="flex items-center justify-between">
                    <Button type="submit" disabled={isSaving}>
                        {isSaving ? 'Saving...' : 'Save Settings'}
                    </Button>
                    {error && <p className="text-sm text-red-500">{error}</p>}
                    {success && <p className="text-sm text-green-500">{success}</p>}
                </div>
            </form>
        </div>
    );
}