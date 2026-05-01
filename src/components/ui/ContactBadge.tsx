// src/components/ui/ContactBadge.tsx
import React from "react";
import Avatar from "boring-avatars";
import { Contact } from "../../types";

interface ContactBadgeProps {
    did: string;
    contacts: Contact[];
    size?: 'sm' | 'md' | 'lg';
    onEdit?: () => void;
}

const truncateDid = (did: string | null | undefined) => {
    if (!did) return "Anonymous";
    if (did.length <= 20) return did;
    return `${did.substring(0, 10)}...${did.substring(did.length - 6)}`;
};

export const ContactBadge: React.FC<ContactBadgeProps> = ({ did, contacts, size = 'md', onEdit }) => {
    if (!did) {
        return <span className={`font-mono ${size === 'sm' ? 'text-[10px]' : 'text-xs'} text-theme-light italic`}>Anonymous</span>;
    }
    
    const contact = contacts.find(c => c.did === did);
    
    const sizeConfig = {
        sm: {
            avatar: 20,
            text: 'text-[10px]',
            name: 'text-xs',
            padding: 'px-2 py-1',
            gap: 'gap-1.5'
        },
        md: {
            avatar: 24,
            text: 'text-xs',
            name: 'text-sm',
            padding: 'px-3 py-2',
            gap: 'gap-2'
        },
        lg: {
            avatar: 32,
            text: 'text-[11px]',
            name: 'text-base',
            padding: 'px-4 py-3',
            gap: 'gap-3'
        }
    };

    const config = sizeConfig[size];

    if (contact) {
        return (
            <div className={`flex items-center ${config.gap} ${config.padding} bg-theme-primary/5 border border-theme-primary/10 rounded-2xl shadow-inner-soft`}>
                <div className="rounded-xl overflow-hidden flex-shrink-0 border border-theme-primary/20 bg-white">
                    <Avatar size={config.avatar} name={contact.did} variant="beam" colors={["#E63946", "#2B1B17", "#E76F51", "#F4A261", "#2A9D8F"]} />
                </div>
                <div className="flex-1 min-w-0 flex flex-col">
                    <p className={`${config.name} font-black text-theme-secondary truncate uppercase tracking-tight`}>
                        {contact.profile.first_name || contact.profile.last_name
                            ? `${contact.profile.first_name || ''} ${contact.profile.last_name || ''}`.trim()
                            : contact.profile.organization || 'Anonymous'}
                    </p>
                    <p className={`${config.text} text-theme-light font-mono opacity-60`}>{truncateDid(contact.did)}</p>
                </div>
                {onEdit && (
                    <button
                        type="button"
                        onClick={onEdit}
                        className="flex-shrink-0 p-1.5 rounded-xl hover:bg-theme-primary/10 text-theme-light hover:text-theme-primary transition-all"
                        title="Edit contact"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                        </svg>
                    </button>
                )}
            </div>
        );
    }
    
    return (
        <div className={`flex items-center ${config.gap} ${config.padding} bg-slate-50 border border-slate-200 rounded-2xl`}>
            <div className="rounded-xl overflow-hidden flex-shrink-0 border border-slate-300 bg-white opacity-40">
                <Avatar size={config.avatar} name={did} variant="beam" />
            </div>
            <p className={`${config.text} text-theme-light font-mono font-bold break-all`}>{truncateDid(did)}</p>
        </div>
    );
};
