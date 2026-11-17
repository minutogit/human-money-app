// src/components/ui/UserAvatar.tsx
import React, { useState, useEffect } from "react";
import { User } from "lucide-react";

export interface UserAvatarProps {
    pictureUrl?: string | null;
    name?: string | null;
    size?: number;
    className?: string;
    rounded?: "full" | "xl" | "2xl";
}

const ROUNDED_CLASSES: Record<NonNullable<UserAvatarProps["rounded"]>, string> = {
    full: "rounded-full",
    xl: "rounded-xl",
    "2xl": "rounded-2xl",
};

export const UserAvatar: React.FC<UserAvatarProps> = ({
    pictureUrl,
    name,
    size = 36,
    className = "",
    rounded = "full",
}) => {
    const [imgError, setImgError] = useState(false);

    useEffect(() => {
        setImgError(false);
    }, [pictureUrl]);

    const trimmedPic = pictureUrl?.trim();
    const showImg = Boolean(trimmedPic) && !imgError;
    const roundedCls = ROUNDED_CLASSES[rounded];
    const iconSize = Math.max(10, Math.round(size * 0.52));

    return (
        <div
            style={{ width: size, height: size }}
            className={`inline-flex items-center justify-center shrink-0 overflow-hidden bg-theme-primary/10 text-theme-primary border border-theme-primary/15 select-none ${roundedCls} ${className}`}
            aria-hidden="true"
            title={name && !name.startsWith("did:") ? name : undefined}
        >
            {showImg ? (
                <img
                    src={trimmedPic}
                    alt=""
                    className="w-full h-full object-cover"
                    onError={() => setImgError(true)}
                />
            ) : (
                <User size={iconSize} strokeWidth={2} />
            )}
        </div>
    );
};

export default UserAvatar;
