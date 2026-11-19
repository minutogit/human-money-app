// src/components/ui/ContactBadge.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { UserPlus, IdCard } from "lucide-react";
import { Contact } from "../../types";
import { isExactSameUserId, isSameRootIdentity } from "../../utils/userIdHelper";
import { UserAvatar } from "./UserAvatar";

interface ContactBadgeProps {
    did: string;
    contacts: Contact[];
    size?: "sm" | "md" | "lg";
    onEdit?: () => void;
    fallbackName?: string | null;
    fallbackPictureUrl?: string | null;
    onAddContact?: (did: string, suggestedName?: string | null) => void;
    onViewCard?: (did: string) => void;
}

const truncateDid = (did: string | null | undefined) => {
    if (!did) return "Anonymous";
    if (did.length <= 20) return did;
    return `${did.substring(0, 10)}...${did.substring(did.length - 6)}`;
};

export const ContactBadge: React.FC<ContactBadgeProps> = ({
    did,
    contacts,
    size = "md",
    onEdit,
    fallbackName,
    fallbackPictureUrl,
    onAddContact,
    onViewCard,
}) => {
    const { t } = useTranslation();
    if (!did) {
        return (
            <span className={`font-mono ${size === "sm" ? "text-[10px]" : "text-xs"} text-theme-light italic`}>
                {t("common.anonymous", "Anonymous")}
            </span>
        );
    }

    const contact =
        contacts.find((c) => isExactSameUserId(c.did, did)) ||
        contacts.find((c) => isSameRootIdentity(c.did, did));

    const sizeConfig = {
        sm: {
            avatar: 22,
            text: "text-[10px]",
            name: "text-xs",
            padding: "px-2.5 py-1.5",
            gap: "gap-2",
        },
        md: {
            avatar: 28,
            text: "text-xs",
            name: "text-sm",
            padding: "px-3 py-2",
            gap: "gap-2.5",
        },
        lg: {
            avatar: 36,
            text: "text-[11px]",
            name: "text-base",
            padding: "px-4 py-3",
            gap: "gap-3",
        },
    };

    const config = sizeConfig[size];

    if (contact) {
        const contactName =
            contact.profile.firstName || contact.profile.lastName
                ? `${contact.profile.firstName || ""} ${contact.profile.lastName || ""}`.trim()
                : contact.profile.organization || fallbackName || t("common.anonymous", "Anonymous");

        return (
            <div
                className={`flex items-center ${config.gap} ${config.padding} bg-theme-primary/5 border border-theme-primary/10 rounded-2xl shadow-inner-soft`}
            >
                <UserAvatar
                    size={config.avatar}
                    rounded="xl"
                    pictureUrl={contact.profile.pictureUrl || fallbackPictureUrl}
                    name={contactName}
                />
                <div className="flex-1 min-w-0 flex flex-col">
                    <p
                        className={`${config.name} font-black text-theme-secondary truncate tracking-tight`}
                        title={contactName}
                    >
                        {contactName}
                    </p>
                    <p
                        className={`${config.text} text-theme-light font-mono opacity-60 truncate`}
                        title={contact.did}
                    >
                        {truncateDid(contact.did)}
                    </p>
                </div>
                {onViewCard && (
                    <button
                        type="button"
                        onClick={() => onViewCard(contact.did)}
                        className="shrink-0 p-1.5 rounded-xl hover:bg-theme-primary/10 text-theme-light hover:text-theme-primary transition-all cursor-pointer"
                        title={t("marketplace.viewPersonCard", "Visitenkarte ansehen")}
                        aria-label={t("marketplace.viewPersonCard", "Visitenkarte ansehen")}
                    >
                        <IdCard size={15} />
                    </button>
                )}
                {onEdit && (
                    <button
                        type="button"
                        onClick={onEdit}
                        className="flex-shrink-0 p-1.5 rounded-xl hover:bg-theme-primary/10 text-theme-light hover:text-theme-primary transition-all cursor-pointer"
                        title={t("contacts.editContact", "Edit contact")}
                        aria-label={t("contacts.editContact", "Edit contact")}
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                strokeWidth={2}
                                d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z"
                            />
                        </svg>
                    </button>
                )}
            </div>
        );
    }

    const displayFallback = fallbackName?.trim() || null;

    return (
        <div className={`flex items-center ${config.gap} ${config.padding} bg-slate-50 border border-slate-200 rounded-2xl`}>
            <UserAvatar
                size={config.avatar}
                rounded="xl"
                pictureUrl={fallbackPictureUrl}
                name={displayFallback || did}
            />
            <div className="flex-1 min-w-0 flex flex-col">
                {displayFallback ? (
                    <>
                        <p className={`${config.name} font-bold text-theme-secondary truncate`} title={displayFallback}>
                            {displayFallback}
                        </p>
                        <p className={`${config.text} text-theme-light font-mono truncate opacity-70`} title={did}>
                            {truncateDid(did)}
                        </p>
                    </>
                ) : (
                    <p className={`${config.text} text-theme-light font-mono font-bold truncate`} title={did}>
                        {truncateDid(did)}
                    </p>
                )}
            </div>
            {(onViewCard || onAddContact) && (
                <div className="flex items-center gap-1 shrink-0">
                    {onViewCard && (
                        <button
                            type="button"
                            onClick={() => onViewCard(did)}
                            className="inline-flex items-center gap-1 px-2 py-1 rounded-xl text-[11px] font-semibold bg-white border border-slate-200 text-theme-secondary hover:border-theme-primary/40 hover:text-theme-primary transition-colors cursor-pointer"
                            title={t("marketplace.viewPersonCard", "Visitenkarte ansehen")}
                        >
                            <IdCard size={13} />
                            <span className="hidden sm:inline">{t("marketplace.cardShortBtn", "Karte")}</span>
                        </button>
                    )}
                    {onAddContact && (
                        <button
                            type="button"
                            onClick={() => onAddContact(did, displayFallback)}
                            className="inline-flex items-center gap-1 px-2 py-1 rounded-xl text-[11px] font-semibold bg-theme-primary/10 text-theme-primary hover:bg-theme-primary/20 transition-colors cursor-pointer"
                            title={t("contacts.addContactBtn", "Als Kontakt hinzufügen")}
                        >
                            <UserPlus size={13} />
                            <span>{t("contacts.addShortBtn", "Kontakt")}</span>
                        </button>
                    )}
                </div>
            )}
        </div>
    );
};
