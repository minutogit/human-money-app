// src/components/profile/MyProfileCard.tsx
import React, { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Edit3, Share2, Tag } from "lucide-react";
import { FrontendMyMarketProfile } from "../../types";
import { marketService } from "../../services/marketService";
import { useSession, SessionContextType } from "../../context/SessionContext";
import { MY_PROFILE_UPDATED_EVENT, openOwnProfileEditor } from "../../utils/syncEvents";
import { Button } from "../ui/Button";
import { UserAvatar } from "../ui/UserAvatar";

export interface MyProfileCardProps {
    myProfile?: FrontendMyMarketProfile | null;
    onEdit?: () => void;
    onShare?: () => void;
    onTogglePublished?: (published: boolean) => Promise<void>;
    showSkills?: boolean;
    compact?: boolean;
}

export const MyProfileCard: React.FC<MyProfileCardProps> = ({
    myProfile: externalProfile,
    onEdit,
    onShare,
    onTogglePublished,
    showSkills = true,
    compact = false,
}) => {
    const { t } = useTranslation();
    let session: SessionContextType | null = null;
    try {
        // eslint-disable-next-line react-hooks/rules-of-hooks
        session = useSession();
    } catch {
        session = null;
    }
    const [internalProfile, setInternalProfile] = useState<FrontendMyMarketProfile | null>(null);
    const [loadFailed, setLoadFailed] = useState(false);
    const [isTogglingPublish, setIsTogglingPublish] = useState(false);

    const loadOwnProfile = useCallback(async () => {
        if (externalProfile !== undefined) return;
        if (typeof marketService?.getMyMarketProfile !== "function") return;
        try {
            const p = await marketService.getMyMarketProfile();
            setInternalProfile(p);
            setLoadFailed(false);
        } catch {
            setLoadFailed(true);
        }
    }, [externalProfile]);

    useEffect(() => {
        if (externalProfile !== undefined) return;
        void loadOwnProfile();

        const handleProfileUpdated = (e: Event) => {
            const custom = e as CustomEvent<FrontendMyMarketProfile | undefined>;
            if (custom.detail?.profile) {
                setInternalProfile(custom.detail);
            } else {
                void loadOwnProfile();
            }
        };

        window.addEventListener(MY_PROFILE_UPDATED_EVENT, handleProfileUpdated);
        return () => {
            window.removeEventListener(MY_PROFILE_UPDATED_EVENT, handleProfileUpdated);
        };
    }, [externalProfile, loadOwnProfile]);

    const effectiveProfile = externalProfile !== undefined ? externalProfile : internalProfile;

    const handleEditClick = () => {
        if (onEdit) {
            onEdit();
        } else {
            openOwnProfileEditor();
        }
    };

    const handlePublishToggle = async () => {
        if (!effectiveProfile) return;
        const nextPublished = !effectiveProfile.published;
        setIsTogglingPublish(true);
        try {
            if (onTogglePublished) {
                await onTogglePublished(nextPublished);
            } else if (session?.protectAction) {
                const updated = await session.protectAction(
                    async (pwd) => {
                        return await marketService.setCardPublished(nextPublished, false, pwd || undefined);
                    },
                    { requireFullAccess: session.accessMode === "receiveOnly" },
                );
                if (updated) {
                    setInternalProfile(updated);
                }
            }
        } finally {
            setIsTogglingPublish(false);
        }
    };

    if (!effectiveProfile) {
        return (
            <div className="p-4 rounded-2xl bg-bg-app border border-theme-subtle flex items-center justify-between gap-3">
                <div className="flex items-center gap-3 min-w-0">
                    <UserAvatar
                        size={42}
                        pictureUrl={session?.userAvatarUrl}
                        name={session?.displayProfileName || session?.profileName}
                    />
                    <div className="min-w-0">
                        <p className="text-sm font-bold text-theme-secondary truncate">
                            {session?.displayProfileName || session?.profileName || t("marketplace.anonymousUser", "Mein Profil")}
                        </p>
                        <p className="text-xs text-theme-light">
                            {t("profile.localProfileHint", "Profil & Visitenkarte einrichten")}
                        </p>
                    </div>
                </div>
                <Button
                    type="button"
                    size="sm"
                    variant="primary"
                    onClick={handleEditClick}
                    disabled={loadFailed || (externalProfile === undefined && typeof marketService?.getMyMarketProfile === "function")}
                    aria-label={t("marketplace.editProfileCardBtn", "Visitenkarte bearbeiten")}
                    className="flex items-center gap-1.5 shrink-0"
                >
                    <Edit3 size={14} />
                    <span>{t("marketplace.editCardBtn", "Bearbeiten")}</span>
                </Button>
            </div>
        );
    }

    const displayName =
        [effectiveProfile.profile.firstName, effectiveProfile.profile.lastName]
            .filter(Boolean)
            .join(" ") ||
        effectiveProfile.profile.organization ||
        session?.profileName ||
        t("marketplace.anonymousUser", "Mein Profil");

    const subtitleParts = [
        effectiveProfile.profile.organization,
        effectiveProfile.profile.address?.city,
        effectiveProfile.skills.length > 0
            ? t("marketplace.skillsCount", "{{count}} Fähigkeit(en)", {
                  count: effectiveProfile.skills.length,
              })
            : null,
    ].filter(Boolean);

    return (
        <div
            className={`${
                compact ? "p-4 rounded-2xl bg-bg-app/60" : "p-5 sm:p-6 rounded-3xl bg-white shadow-xs"
            } border border-theme-subtle space-y-4`}
            data-testid={effectiveProfile.published ? "my-published-card-summary" : "my-local-card-summary"}
        >
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                <div className="flex items-center gap-3.5 min-w-0">
                    <UserAvatar
                        size={48}
                        rounded="2xl"
                        pictureUrl={effectiveProfile.profile.pictureUrl}
                        name={displayName}
                    />
                    <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                            <h4 className="text-sm font-bold text-theme-secondary truncate">
                                {displayName}
                            </h4>
                            {effectiveProfile.published ? (
                                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-50 text-emerald-700 border border-emerald-200">
                                    <span className="w-1.5 h-1.5 rounded-full bg-emerald-500" />
                                    <span>{t("marketplace.publishedBadge", "Veröffentlicht")}</span>
                                </span>
                            ) : (
                                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-slate-100 text-slate-600 border border-slate-200">
                                    <span className="w-1.5 h-1.5 rounded-full bg-slate-400" />
                                    <span>{t("marketplace.localDraftBadge", "Nur lokal")}</span>
                                </span>
                            )}
                        </div>
                        <p className="text-xs text-theme-light truncate mt-0.5">
                            {subtitleParts.length > 0
                                ? subtitleParts.join(" · ")
                                : t("marketplace.noCardDetailsYet", "Noch keine Fähigkeiten oder Ort eingetragen")}
                        </p>
                    </div>
                </div>

                <div className="flex flex-wrap items-center gap-2 self-end sm:self-auto">
                    <label
                        className="inline-flex items-center gap-2 px-3 py-1.5 rounded-xl bg-white border border-theme-subtle cursor-pointer text-xs font-semibold text-theme-secondary select-none hover:border-theme-primary/40 transition-colors"
                        title={t(
                            "marketplace.publishCardSwitchHint",
                            "Aus: Nur lokal auf diesem Gerät gespeichert. · An: Signiert, teilbar und gemäß Sichtbarkeit im Netzwerk auffindbar."
                        )}
                    >
                        <input
                            type="checkbox"
                            checked={effectiveProfile.published}
                            disabled={isTogglingPublish}
                            onChange={handlePublishToggle}
                            data-testid="my-card-publish-checkbox"
                            className="rounded accent-theme-primary w-4 h-4 cursor-pointer"
                        />
                        <span>{t("marketplace.publishedToggle", "Veröffentlicht")}</span>
                    </label>

                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={handleEditClick}
                        aria-label={t("marketplace.editProfileCardBtn", "Visitenkarte bearbeiten")}
                        className="flex items-center gap-1.5"
                    >
                        <Edit3 size={14} />
                        <span>{t("marketplace.editCardBtn", "Bearbeiten")}</span>
                    </Button>

                    {effectiveProfile.published && onShare && (
                        <Button
                            type="button"
                            size="sm"
                            variant="primary"
                            onClick={onShare}
                            className="flex items-center gap-1.5"
                        >
                            <Share2 size={14} />
                            <span>{t("marketplace.shareCardBtn", "Teilen")}</span>
                        </Button>
                    )}
                </div>
            </div>

            {!effectiveProfile.published && (
                <p className="text-[11px] text-theme-light leading-relaxed">
                    {t(
                        "marketplace.unpublishedCardHint",
                        "Aktiviere „Veröffentlicht“, damit deine Visitenkarte signiert wird, geteilt werden kann und über Sync-Dienste auffindbar ist."
                    )}
                </p>
            )}

            {showSkills && effectiveProfile.skills.length > 0 && (
                <div className="pt-3 border-t border-theme-subtle/50 flex flex-wrap gap-2">
                    {effectiveProfile.skills.map((s, idx) => (
                        <span
                            key={idx}
                            className="inline-flex items-center gap-1 px-2.5 py-1 rounded-xl bg-bg-app border border-theme-subtle text-xs text-theme-secondary font-medium"
                        >
                            <Tag size={11} className="text-theme-primary" />
                            <span>{s.title}</span>
                            {s.pricing && (
                                <span className="text-[10px] text-theme-light">({s.pricing})</span>
                            )}
                        </span>
                    ))}
                </div>
            )}
        </div>
    );
};

export default MyProfileCard;
