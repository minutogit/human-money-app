// src/components/profile/GlobalProfileEditorModal.tsx
import React, { useState, useEffect, useCallback } from "react";
import { FrontendMyMarketProfile, FrontendSaveMyMarketProfileRequest } from "../../types";
import { marketService } from "../../services/marketService";
import { useSession } from "../../context/SessionContext";
import { OPEN_PROFILE_EDITOR_EVENT } from "../../utils/syncEvents";
import { ProfileCardEditor } from "../marketplace/ProfileCardEditor";
import { logger } from "../../utils/log";
import { stringifyError } from "../../utils/errorHelper";

export const GlobalProfileEditorModal: React.FC = () => {
    const { isAuthenticated, protectAction } = useSession();
    const [isOpen, setIsOpen] = useState(false);
    const [myProfile, setMyProfile] = useState<FrontendMyMarketProfile | null>(null);

    const openEditor = useCallback(async () => {
        if (!isAuthenticated) return;
        try {
            const loaded = await marketService.getMyMarketProfile();
            if (loaded) {
                setMyProfile(loaded);
                setIsOpen(true);
            }
        } catch {
            try {
                const loaded = await protectAction(async (pwd) => {
                    return await marketService.getMyMarketProfile(pwd || undefined);
                });
                if (loaded) {
                    setMyProfile(loaded);
                    setIsOpen(true);
                }
            } catch (err) {
                logger.error("Failed to open global profile editor: " + stringifyError(err));
            }
        }
    }, [isAuthenticated, protectAction]);

    useEffect(() => {
        const handler = () => {
            void openEditor();
        };
        window.addEventListener(OPEN_PROFILE_EDITOR_EVENT, handler);
        return () => {
            window.removeEventListener(OPEN_PROFILE_EDITOR_EVENT, handler);
        };
    }, [openEditor]);

    const handleSave = async (req: FrontendSaveMyMarketProfileRequest) => {
        const updated = await protectAction(
            async (pwd) => {
                return await marketService.saveMyMarketProfile(req, pwd || undefined);
            },
            { requireFullAccess: true }
        );
        if (updated) {
            setMyProfile(updated);
        }
        setIsOpen(false);
    };

    if (!isOpen || !myProfile) {
        return null;
    }

    return (
        <ProfileCardEditor
            isOpen={isOpen}
            onClose={() => setIsOpen(false)}
            myProfile={myProfile}
            onSave={handleSave}
        />
    );
};

export default GlobalProfileEditorModal;
