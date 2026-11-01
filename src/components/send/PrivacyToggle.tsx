import { useTranslation } from 'react-i18next';
import { Shield, Lock, Eye, AlertTriangle } from 'lucide-react';
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
  privacyError,
}: PrivacyToggleProps) {
  const { t } = useTranslation();

  return (
    <div
      id="privacyMode"
      className={`px-5 py-3.5 bg-white shadow-premium rounded-3xl border transition-all ${
        privacyError ? 'border-rose-500 ring-2 ring-rose-100' : 'border-theme-subtle/50'
      }`}
    >
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <Shield size={16} className="text-theme-primary shrink-0" />
            <span className="font-black text-xs uppercase tracking-widest text-theme-primary">
              {t('transfer.securityPrivacy')}
            </span>
            <HelpIcon topic="privacy" size={14} />
          </div>
          <p className="text-[11px] text-theme-light font-medium mt-0.5">
            {privacyRules.forced ? (
              t('transfer.mandatoryStandard')
            ) : privacyMode === 'stealth' ? (
              t('transfer.privacyStealthSummary')
            ) : privacyMode === 'public' ? (
              t('transfer.privacyPublicSummary')
            ) : (
              <span className={privacyError ? 'text-rose-600 font-semibold' : ''}>
                {t('transfer.privacySelectPrompt')}
              </span>
            )}
          </p>
        </div>

        <div className="shrink-0">
          {privacyRules.forced ? (
            <div
              className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-bold ${
                privacyRules.mode === 'Stealth'
                  ? 'bg-purple-100 text-purple-700 border border-purple-200/60'
                  : 'bg-blue-100 text-blue-700 border border-blue-200/60'
              }`}
            >
              {privacyRules.mode === 'Stealth' ? <Lock size={14} /> : <Eye size={14} />}
              <span>{privacyRules.mode === 'Stealth' ? t('transfer.stealth') : t('transfer.public')}</span>
            </div>
          ) : (
            <div className="inline-flex p-1 bg-slate-100 rounded-xl border border-slate-200/70 gap-1">
              <button
                type="button"
                onClick={() => onPrivacyChange('public')}
                className={`px-3 py-1.5 rounded-lg text-xs font-bold flex items-center gap-1.5 transition-all cursor-pointer ${
                  privacyMode === 'public'
                    ? 'bg-white text-blue-700 shadow-xs border border-blue-200/60'
                    : 'text-slate-600 hover:text-slate-900'
                }`}
              >
                <Eye size={14} />
                <span>{t('transfer.public')}</span>
              </button>
              <button
                type="button"
                onClick={() => onPrivacyChange('stealth')}
                className={`px-3 py-1.5 rounded-lg text-xs font-bold flex items-center gap-1.5 transition-all cursor-pointer ${
                  privacyMode === 'stealth'
                    ? 'bg-white text-purple-700 shadow-xs border border-purple-200/60'
                    : 'text-slate-600 hover:text-slate-900'
                }`}
              >
                <Lock size={14} />
                <span>{t('transfer.stealth')}</span>
              </button>
            </div>
          )}
        </div>
      </div>

      {privacyRules.mode === 'Incompatible' && (
        <div className="mt-3 p-2.5 bg-rose-50 border border-rose-100 rounded-xl flex items-center gap-2">
          <AlertTriangle size={16} className="text-rose-500 shrink-0" />
          <p className="text-xs font-bold text-rose-800">{t('transfer.privacyConflict')}</p>
        </div>
      )}
    </div>
  );
}
