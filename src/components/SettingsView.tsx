// src/components/SettingsView.tsx
import { useState, useEffect, FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/log';
import { Button } from './ui/Button';
import { Card } from './ui/Card';
import { Input } from './ui/Input';
import { AppSettings, PrivacyDefault } from '../types';
import { useSession } from '../context/SessionContext';

import { ProfileSettings } from './ProfileSettings';
import { PageLayout } from './ui/PageLayout';
import { 
    User, 
    Settings as SettingsIcon, 
    Shield, 
    Eye, 
    EyeOff, 
    Clock, 
    Database,
    Save,
    CheckCircle2,
    Fingerprint
} from 'lucide-react';

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
            setSuccess("Configuration synchronized!");
            logger.info("Settings saved successfully.");
            setTimeout(() => setSuccess(''), 3000);
        } catch (e) {
            const msg = `Failed to save settings: ${e}`;
            logger.error(msg);
            setError(msg);
        } finally {
            setIsSaving(false);
        }
    };

    if (isLoading) {
        return <div className="py-20 text-center animate-pulse text-theme-light font-black uppercase tracking-[0.2em]">Configuring Environment...</div>;
    }

    return (
        <PageLayout 
            title="System Preferences" 
            description="Manage your identity, security protocols and network defaults."
            onBack={onBack}
        >
            <div className="max-w-4xl mx-auto">
                {/* Tab Navigation */}
                <div className="flex bg-white/30 backdrop-blur-md p-1.5 rounded-2xl border border-theme-subtle mb-10 shadow-inner-soft">
                    <button 
                        onClick={() => setActiveTab('profile')}
                        className={`flex-1 flex items-center justify-center gap-2 py-3 rounded-xl text-xs font-black uppercase tracking-widest transition-all ${activeTab === 'profile' ? 'bg-white text-theme-primary shadow-premium' : 'text-theme-light hover:text-theme-secondary hover:bg-white/40'}`}
                    >
                        <User size={14} />
                        Identity Profile
                    </button>
                    <button 
                        onClick={() => setActiveTab('app')}
                        className={`flex-1 flex items-center justify-center gap-2 py-3 rounded-xl text-xs font-black uppercase tracking-widest transition-all ${activeTab === 'app' ? 'bg-white text-theme-primary shadow-premium' : 'text-theme-light hover:text-theme-secondary hover:bg-white/40'}`}
                    >
                        <SettingsIcon size={14} />
                        Security & Ops
                    </button>
                </div>

                {activeTab === 'profile' ? (
                    <ProfileSettings />
                ) : (
                    <form onSubmit={handleSave} className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
                        <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
                            {/* Storage & Data Retention */}
                            <Card header={
                                <div className="flex items-center gap-2">
                                    <Database size={18} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">Data Stewardship</span>
                                </div>
                            }>
                                <div className="space-y-6">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Bundle Retention (Days)</label>
                                        <Input
                                            type="number"
                                            value={settings?.bundleRetentionDays ?? 30}
                                            onChange={(e) => setSettings(s => s ? { ...s, bundleRetentionDays: parseInt(e.target.value, 10) } : null)}
                                            min="1"
                                            className="font-bold"
                                        />
                                        <p className="text-[10px] text-theme-light/60 font-medium leading-relaxed">
                                            Sets the archival threshold for cryptographic transfer bundles.
                                        </p>
                                    </div>
                                </div>
                            </Card>

                            {/* Session Control */}
                            <Card header={
                                <div className="flex items-center gap-2">
                                    <Clock size={18} className="text-theme-primary" />
                                    <span className="font-black text-xs uppercase tracking-widest">Vault Session</span>
                                </div>
                            }>
                                <div className="space-y-6">
                                    <div className="space-y-2">
                                        <label className="text-[10px] font-black text-theme-light uppercase tracking-widest">Timeout Interval (Minutes)</label>
                                        <Input
                                            type="number"
                                            value={settings ? Math.floor(settings.sessionTimeoutSeconds / 60) : 10}
                                            onChange={(e) => {
                                                const minutes = parseInt(e.target.value, 10);
                                                setSettings(s => s ? { ...s, sessionTimeoutSeconds: isNaN(minutes) ? 0 : minutes * 60 } : null);
                                            }}
                                            min="0"
                                            className="font-bold"
                                        />
                                        <p className="text-[10px] text-theme-light/60 font-medium leading-relaxed">
                                            Inactivity period before requiring re-authentication.
                                        </p>
                                    </div>
                                </div>
                            </Card>
                        </div>

                        {/* Privacy Protocols */}
                        <Card header={
                            <div className="flex items-center gap-2">
                                <Shield size={18} className="text-theme-primary" />
                                <span className="font-black text-xs uppercase tracking-widest">Privacy Protocols</span>
                            </div>
                        }>
                            <div className="space-y-4">
                                <p className="text-xs text-theme-light font-medium mb-4">Default visibility for flexible standard transactions:</p>
                                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                                    {[
                                        { id: 'ask', label: 'Interactive', icon: Fingerprint, desc: 'Always prompt' },
                                        { id: 'stealth', label: 'Stealth', icon: EyeOff, desc: 'Maximum privacy' },
                                        { id: 'public', label: 'Public', icon: Eye, desc: 'Identity transparency' }
                                    ].map((mode) => (
                                        <label 
                                            key={mode.id}
                                            className={`relative flex flex-col p-4 rounded-2xl border-2 transition-all cursor-pointer group ${settings?.privacyDefault === mode.id ? 'bg-theme-primary/5 border-theme-primary shadow-sm' : 'bg-white border-theme-subtle hover:border-theme-primary/30'}`}
                                        >
                                            <input
                                                type="radio"
                                                name="privacyDefault"
                                                value={mode.id}
                                                checked={settings?.privacyDefault === mode.id}
                                                onChange={(e) => setSettings(s => s ? { ...s, privacyDefault: e.target.value as PrivacyDefault } : null)}
                                                className="absolute opacity-0"
                                            />
                                            <mode.icon className={`mb-3 transition-colors ${settings?.privacyDefault === mode.id ? 'text-theme-primary' : 'text-theme-light group-hover:text-theme-secondary'}`} size={20} />
                                            <span className={`text-sm font-black tracking-tight ${settings?.privacyDefault === mode.id ? 'text-theme-primary' : 'text-theme-secondary'}`}>{mode.label}</span>
                                            <span className="text-[10px] font-medium text-theme-light mt-1">{mode.desc}</span>
                                            {settings?.privacyDefault === mode.id && (
                                                <div className="absolute top-3 right-3 text-theme-primary">
                                                    <CheckCircle2 size={16} />
                                                </div>
                                            )}
                                        </label>
                                    ))}
                                </div>
                            </div>
                        </Card>

                        <div className="pt-6 flex flex-col items-center gap-4">
                            <Button type="submit" disabled={isSaving} className="min-w-[240px] gap-2 rounded-2xl py-4 shadow-lg shadow-theme-primary/20">
                                {isSaving ? <Clock className="animate-spin" size={18} /> : <Save size={18} />}
                                {isSaving ? 'Synchronizing...' : 'Save System Changes'}
                            </Button>
                            {error && <p className="text-sm font-bold text-rose-500 animate-bounce">{error}</p>}
                            {success && <p className="text-sm font-bold text-emerald-500 flex items-center gap-2"><CheckCircle2 size={16}/> {success}</p>}
                        </div>
                    </form>
                )}
            </div>
        </PageLayout>
    );
}
