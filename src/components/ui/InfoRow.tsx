import React from 'react';

interface InfoRowProps {
  label: string;
  children: React.ReactNode;
  isMono?: boolean;
  icon?: any;
}

export const InfoRow: React.FC<InfoRowProps> = ({ label, children, isMono = false, icon: Icon }) => {
  if (!children) return null;
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-1.5 opacity-60">
        {Icon && <Icon size={12} className="text-theme-light" />}
        <p className="text-[10px] font-black text-theme-light uppercase tracking-widest">{label}</p>
      </div>
      <p className={`${isMono ? 'font-mono text-xs' : 'text-sm font-bold'} text-theme-secondary break-words`}>
        {children}
      </p>
    </div>
  );
};
