// src/components/SettingsView.tsx
import { useState, useEffect, FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { AppSettings } from '../types';
import { useSession } from '../context/SessionContext';

import { ProfileSettings } from './ProfileSettings';

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
    const [activeTab, setActiveTab] = useState<'app' | 'profile'>('profile');

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
        <div className="max-w-3xl mx-auto pb-12">
            <header className="flex-shrink-0 mb-8">
                <div className="flex items-center gap-4 mb-4">
                    <button
                        onClick={onBack}
                        className="p-2.5 rounded-full bg-white border border-theme-subtle hover:bg-bg-input-readonly transition-all text-theme-light hover:text-theme-primary shadow-sm active:scale-95"
                        title="Back to Dashboard"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 19l-7-7m0 0l7-7m-7 7h18" />
                        </svg>
                    </button>
                    <h1 className="text-3xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-theme-primary to-theme-secondary">Settings</h1>
                </div>
                
                <div className="flex border-b border-theme-subtle gap-8">
                    <button 
                        onClick={() => setActiveTab('profile')}
                        className={`pb-2 text-sm font-medium transition-colors relative ${activeTab === 'profile' ? 'text-theme-primary' : 'text-theme-light hover:text-theme-main'}`}
                    >
                        User Profile
                        {activeTab === 'profile' && <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-theme-primary animate-in fade-in zoom-in duration-300"></div>}
                    </button>
                    <button 
                        onClick={() => setActiveTab('app')}
                        className={`pb-2 text-sm font-medium transition-colors relative ${activeTab === 'app' ? 'text-theme-primary' : 'text-theme-light hover:text-theme-main'}`}
                    >
                        App Settings
                        {activeTab === 'app' && <div className="absolute bottom-0 left-0 right-0 h-0.5 bg-theme-primary animate-in fade-in zoom-in duration-300"></div>}
                    </button>
                </div>
            </header>

            {activeTab === 'profile' ? (
                <ProfileSettings />
            ) : (
                <form onSubmit={handleSave} className="space-y-6 bg-bg-card border border-theme-subtle p-8 rounded-xl shadow-xl animate-in fade-in slide-in-from-bottom-4 duration-500">
                    <div>
                        <h2 className="text-lg font-semibold text-theme-main mb-4">Storage & Security</h2>
                        
                        <div className="space-y-6">
                            <div>
                                <label htmlFor="retention" className="block text-sm font-medium text-theme-light mb-1.5">
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
                                <p className="text-xs text-theme-light mt-2 bg-theme-primary/5 p-2 rounded border-l-2 border-theme-primary">
                                    How long the full data for sent transfers should be stored. After this period, only the metadata is kept.
                                </p>
                            </div>

                            <div>
                                <label htmlFor="sessionTimeout" className="block text-sm font-medium text-theme-light mb-1.5">
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
                                <p className="text-xs text-theme-light mt-2 bg-theme-secondary/5 p-2 rounded border-l-2 border-theme-secondary">
                                    Duration to keep the wallet unlocked after activity. Set to 0 to always ask for password (High Security).
                                </p>
                            </div>
                        </div>
                    </div>

                    <div className="pt-6 border-t border-theme-subtle flex items-center justify-between">
                        <Button type="submit" disabled={isSaving} className="min-w-[150px]">
                            {isSaving ? 'Saving...' : 'Save Settings'}
                        </Button>
                        <div className="flex-1 text-right">
                            {error && <p className="text-sm text-red-500">{error}</p>}
                            {success && <p className="text-sm text-green-500 font-medium">{success}</p>}
                        </div>
                    </div>
                </form>
            )}
        </div>
    );
}