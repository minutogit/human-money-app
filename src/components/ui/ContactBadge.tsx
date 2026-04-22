import React from "react";
import Avatar from "boring-avatars";
import { Contact } from "../../types";

interface ContactBadgeProps {
    did: string;
    contacts: Contact[];
    size?: 'sm' | 'md';
    onEdit?: () => void;
}

const truncateDid = (did: string) => {
    if (did.length <= 20) return did;
    return `${did.substring(0, 10)}...${did.substring(did.length - 6)}`;
};

export const ContactBadge: React.FC<ContactBadgeProps> = ({ did, contacts, size = 'md', onEdit }) => {
    const contact = contacts.find(c => c.did === did);
    const avatarSize = size === 'sm' ? 20 : 24;
    const textSize = size === 'sm' ? 'text-[10px]' : 'text-xs';
    const padding = size === 'sm' ? 'px-2 py-1' : 'px-3 py-2';

    if (contact) {
        return (
            <div className={`flex items-center gap-2 ${padding} bg-theme-accent/10 border border-theme-accent/30 rounded-lg`}>
                <div className={`w-${avatarSize/4} h-${avatarSize/4} rounded-full overflow-hidden flex-shrink-0`}>
                    <Avatar size={avatarSize} name={contact.did} variant="beam" colors={["#E63946", "#2B1B17", "#E76F51", "#F4A261", "#2A9D8F"]} />
                </div>
                <div className="flex-1 min-w-0 flex flex-wrap items-center gap-2">
                    <p className={`${size === 'sm' ? 'text-xs' : 'text-sm'} font-bold text-theme-secondary truncate`}>
                        {contact.profile.first_name || contact.profile.last_name
                            ? `${contact.profile.first_name || ''} ${contact.profile.last_name || ''}`.trim()
                            : contact.profile.organization || 'Anonymous'}
                    </p>
                    <p className={`${textSize} text-theme-light font-mono`}>{truncateDid(contact.did)}</p>
                </div>
                {onEdit && (
                    <button
                        type="button"
                        onClick={onEdit}
                        className="flex-shrink-0 p-1 rounded hover:bg-theme-accent/20 text-theme-light hover:text-theme-primary transition-colors"
                        title="Edit contact"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                        </svg>
                    </button>
                )}
            </div>
        );
    }
    return <span className={`font-mono ${textSize} break-all`}>{truncateDid(did)}</span>;
};
