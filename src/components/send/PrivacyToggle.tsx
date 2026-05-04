import { Shield, Lock, Eye, AlertTriangle, CheckCircle2, Info, ArrowRight } from 'lucide-react';
import { Card } from '../ui/Card';

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
  return (
    <Card header={
      <div id="privacyMode" className="flex items-center gap-2">
        <Shield size={18} className="text-theme-primary"/>
        <span className="font-black text-xs uppercase tracking-widest text-theme-primary">Security & Privacy</span>
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
                  {privacyRules.mode === 'Stealth' ? 'Stealth Execution' : 'Transparent Execution'}
                </h4>
                <p className="text-[10px] font-bold text-theme-light uppercase tracking-widest">Mandatory for selected standard</p>
              </div>
            </div>
            <CheckCircle2 className="text-theme-primary" size={24} />
          </div>
        ) : (
          <div className={`grid grid-cols-2 gap-4 p-2 bg-slate-50 border-2 rounded-[40px] transition-all ${privacyError ? 'border-rose-500 ring-2 ring-rose-100' : 'border-slate-100'}`}>
            <button 
              type="button" 
              onClick={() => onPrivacyChange('public')} 
              className={`flex flex-col items-center gap-2 p-6 rounded-[32px] transition-all ${privacyMode === 'public' ? 'bg-white shadow-premium-lg border border-theme-subtle' : 'opacity-50 hover:opacity-80'}`}
            >
              <div className="p-3 bg-blue-50 text-blue-500 rounded-2xl"><Eye size={24}/></div>
              <span className="text-xs font-black uppercase tracking-widest text-slate-900">Public</span>
            </button>
            <button 
              type="button" 
              onClick={() => onPrivacyChange('stealth')} 
              className={`flex flex-col items-center gap-2 p-6 rounded-[32px] transition-all ${privacyMode === 'stealth' ? 'bg-white shadow-premium-lg border border-theme-subtle' : 'opacity-50 hover:opacity-80'}`}
            >
              <div className="p-3 bg-purple-50 text-purple-500 rounded-2xl"><Lock size={24}/></div>
              <span className="text-xs font-black uppercase tracking-widest text-slate-900">Stealth</span>
            </button>
          </div>
        )}

        {privacyRules.mode === 'Incompatible' && (
          <div className="p-4 bg-rose-50 border border-rose-100 rounded-2xl flex items-center gap-3">
            <AlertTriangle size={18} className="text-rose-500" />
            <p className="text-xs font-bold text-rose-800">Privacy Conflict: Mixed privacy requirements in selection.</p>
          </div>
        )}

        <details className="group">
          <summary className="flex items-center gap-2 text-[10px] font-black uppercase tracking-widest text-theme-light cursor-pointer hover:text-theme-primary transition-colors list-none">
            <Info size={14}/> Privacy Details <ArrowRight size={10} className="group-open:rotate-90 transition-transform" />
          </summary>
          <div className="mt-4 p-5 bg-slate-50 rounded-3xl border border-slate-100 space-y-4 animate-in slide-in-from-top-2">
            <div className="space-y-1">
              <h5 className="text-xs font-black text-slate-900">🔒 Stealth Mode</h5>
              <p className="text-[11px] text-slate-600 leading-relaxed font-medium">Anonymizes your signature in the transaction chain. Prevents mass surveillance. Reveal occurs only upon double-spend detection.</p>
            </div>
            <div className="space-y-1">
              <h5 className="text-xs font-black text-slate-900">👁️ Public Mode</h5>
              <p className="text-[11px] text-slate-600 leading-relaxed font-medium">Standard transparency. Builds high trust in closed networks and community circles.</p>
            </div>
          </div>
        </details>
      </div>
    </Card>
  );
}
