import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Shield, Lock, Eye, AlertTriangle, CheckCircle2 } from 'lucide-react';
import { Card } from '../ui/Card';
import { HelpIcon } from '../ui/HelpIcon';

interface PrivacyToggleProps {
  privacyMode: 'public' | 'stealth' | null;
  onPrivacyChange: (mode: 'public' | 'stealth') => void;
  privacyRules: { mode: string; forced: boolean };
  privacyError: boolean;
}

export function PrivacyToggle({
  privacyMode,
  onPrivacyChange,
  privacyRules,
  privacyError
}: PrivacyToggleProps) {
  const { t } = useTranslation();
  const [isEditing, setIsEditing] = useState(false);

  const showSelector = isEditing || privacyMode === null || privacyError;

  return (
    <Card header={
      <div id="privacyMode" className="flex items-center gap-2">
        <Shield size={18} className="text-theme-primary"/>
        <span className="font-black text-xs uppercase tracking-widest text-theme-primary">{t('transfer.securityPrivacy')}</span>
        <HelpIcon topic="privacy" size={14} />
      </div>
    }>
      <div className="space-y-6">
        {privacyRules.forced ? (
          <div className="p-5 bg-theme-primary/5 border border-theme-primary/10 rounded-[32px] flex items-center justify-between shadow-inner-soft">
            <div className="flex items-center gap-4">
              <div className={`p-3 rounded-2xl ${privacyRules.mode === 'Stealth' ? 'bg-purple-500 text-white' : 'bg-blue-500 text-white'}`}>
                {privacyRules.mode === 'Stealth' ? <Lock size={20}/> : <Eye size={20}/>}
              </div>
              <div>
                <h4 className="text-sm font-black text-theme-secondary uppercase tracking-widest">
                  {privacyRules.mode === 'Stealth' ? t('transfer.stealthExecution') : t('transfer.transparentExecution')}
                </h4>
                <p className="text-[10px] font-bold text-theme-light uppercase tracking-widest">{t('transfer.mandatoryStandard')}</p>
              </div>
            </div>
            <CheckCircle2 className="text-theme-primary" size={24} />
          </div>
        ) : showSelector ? (
          <div className="space-y-3">
            <div className={`grid grid-cols-2 gap-4 p-2 bg-slate-50 border-2 rounded-[40px] transition-all ${privacyError ? 'border-rose-500 ring-2 ring-rose-100' : 'border-slate-100'}`}>
              <button 
                type="button" 
                onClick={() => {
                  onPrivacyChange('public');
                  setIsEditing(false);
                }} 
                className={`flex flex-col items-center gap-2 p-6 rounded-[32px] transition-all ${privacyMode === 'public' ? 'bg-white shadow-premium-lg border border-theme-subtle' : 'opacity-50 hover:opacity-80'}`}
              >
                <div className="p-3 bg-blue-50 text-blue-500 rounded-2xl"><Eye size={24}/></div>
                <span className="text-xs font-black uppercase tracking-widest text-slate-900">{t('transfer.public')}</span>
              </button>
              <button 
                type="button" 
                onClick={() => {
                  onPrivacyChange('stealth');
                  setIsEditing(false);
                }} 
                className={`flex flex-col items-center gap-2 p-6 rounded-[32px] transition-all ${privacyMode === 'stealth' ? 'bg-white shadow-premium-lg border border-theme-subtle' : 'opacity-50 hover:opacity-80'}`}
              >
                <div className="p-3 bg-purple-50 text-purple-500 rounded-2xl"><Lock size={24}/></div>
                <span className="text-xs font-black uppercase tracking-widest text-slate-900">{t('transfer.stealth')}</span>
              </button>
            </div>
            {privacyMode !== null && (
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={() => setIsEditing(false)}
                  className="text-xs font-bold text-theme-light hover:text-theme-secondary transition-colors px-2 py-1"
                >
                  {t('common.done')}
                </button>
              </div>
            )}
          </div>
        ) : (
          <div className="p-4 bg-slate-50/80 border border-theme-subtle/50 rounded-2xl flex items-center justify-between gap-4">
            <div className="flex items-center gap-3">
              <div className={`p-2 rounded-xl ${privacyMode === 'stealth' ? 'bg-purple-100 text-purple-700' : 'bg-blue-100 text-blue-700'}`}>
                {privacyMode === 'stealth' ? <Lock size={18} /> : <Eye size={18} />}
              </div>
              <div>
                <p className="text-xs font-black text-theme-secondary">
                  {privacyMode === 'stealth' ? t('transfer.stealth') : t('transfer.public')}
                </p>
                <p className="text-[11px] text-theme-light font-medium">
                  {privacyMode === 'stealth' ? t('transfer.privacyStealthSummary') : t('transfer.privacyPublicSummary')}
                </p>
              </div>
            </div>
            <button
              type="button"
              onClick={() => setIsEditing(true)}
              className="px-3 py-1.5 rounded-xl border border-theme-subtle bg-white text-xs font-bold text-theme-secondary hover:bg-bg-app transition-colors shadow-sm"
            >
              {t('common.change')}
            </button>
          </div>
        )}

        {privacyRules.mode === 'Incompatible' && (
          <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3">
            <AlertTriangle size={18} className="text-rose-500" />
            <p className="text-xs font-bold text-rose-800">{t('transfer.privacyConflict')}</p>
          </div>
        )}
      </div>
    </Card>
  );
}
