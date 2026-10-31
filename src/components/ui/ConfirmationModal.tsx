import React from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { ModalShell } from './ModalShell';

export interface ConfirmationModalProps {
    isOpen: boolean;
    title: string;
    description: React.ReactNode;
    confirmVariant?: "primary" | "secondary" | "outline" | "danger";
    confirmText?: string;
    cancelText?: string;
    onConfirm: () => void;
    onCancel: () => void;
    isProcessing?: boolean;
    confirmDisabled?: boolean;
}

export function ConfirmationModal({ 
    isOpen, 
    title, 
    description, 
    confirmVariant = "primary",
    confirmText, 
    cancelText, 
    onConfirm, 
    onCancel,
    isProcessing = false,
    confirmDisabled = false
}: ConfirmationModalProps) {
    const { t } = useTranslation();

    return (
        <ModalShell
            isOpen={isOpen}
            onClose={isProcessing ? undefined : onCancel}
            maxWidth="md"
            title={<h2 className="text-xl font-bold text-theme-primary mb-2">{title}</h2>}
            description={description}
            footer={
                <div className="grid grid-cols-2 gap-3">
                    <Button
                        variant="secondary"
                        onClick={onCancel}
                        disabled={isProcessing}
                        className="w-full"
                    >
                        {cancelText || t('common.cancel')}
                    </Button>
                    <Button
                        variant={confirmVariant}
                        onClick={onConfirm}
                        disabled={isProcessing || confirmDisabled}
                        className="w-full"
                    >
                        {isProcessing ? t('common.processing') : (confirmText || t('common.confirm'))}
                    </Button>
                </div>
            }
        />
    );
}