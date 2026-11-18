import React, { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { createPortal } from 'react-dom';
import { X } from 'lucide-react';

export interface ModalShellProps {
    isOpen: boolean;
    onClose?: () => void;
    title?: React.ReactNode;
    description?: React.ReactNode;
    footer?: React.ReactNode;
    maxWidth?: 'sm' | 'md' | 'lg' | 'xl' | '2xl';
    children?: React.ReactNode;
    closeOnEscape?: boolean;
    closeOnBackdrop?: boolean;
    hideCloseButton?: boolean;
    className?: string;
    zIndexClass?: string;
}

const maxWidthMap: Record<NonNullable<ModalShellProps['maxWidth']>, string> = {
    sm: 'max-w-sm',
    md: 'max-w-md',
    lg: 'max-w-lg',
    xl: 'max-w-xl',
    '2xl': 'max-w-2xl',
};

export function ModalShell({
    isOpen,
    onClose,
    title,
    description,
    footer,
    maxWidth = 'md',
    children,
    closeOnEscape = true,
    closeOnBackdrop = true,
    hideCloseButton = false,
    className = '',
    zIndexClass = 'z-[100]',
}: ModalShellProps) {
    const { t } = useTranslation();
    useEffect(() => {
        if (!isOpen || closeOnEscape === false || !onClose) return;

        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape') {
                onClose();
            }
        };

        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [isOpen, closeOnEscape, onClose]);

    if (!isOpen) return null;

    const maxWidthClass = maxWidthMap[maxWidth] ?? maxWidthMap.md;

    const handleBackdropClick = (e: React.MouseEvent<HTMLDivElement>) => {
        if (closeOnBackdrop && onClose && e.target === e.currentTarget) {
            onClose();
        }
    };

    const modalContent = (
        <div
            className={`fixed inset-0 ${zIndexClass} flex items-center justify-center bg-black/50 backdrop-blur-sm p-4`}
            onClick={handleBackdropClick}
        >
            <div
                role="dialog"
                aria-modal="true"
                className={`relative w-full ${maxWidthClass} max-h-[90vh] flex flex-col rounded-3xl bg-bg-card border border-theme-subtle shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-200 ${className}`}
            >
                {onClose && !hideCloseButton && (
                    <button
                        type="button"
                        onClick={onClose}
                        aria-label={t('common.closeDialog', 'Dialog schließen')}
                        className="absolute top-5 right-5 p-1.5 rounded-full text-theme-light hover:text-theme-primary hover:bg-theme-subtle/30 transition-colors z-10 cursor-pointer"
                    >
                        <X size={20} />
                    </button>
                )}

                {(title || description) && (
                    <div className={`px-6 pt-6 pb-3 ${onClose && !hideCloseButton ? 'pr-14' : ''}`}>
                        {title && (
                            typeof title === 'string' ? (
                                <h2 className="text-xl font-bold text-theme-primary mb-2">{title}</h2>
                            ) : (
                                title
                            )
                        )}
                        {description && (
                            <div className="text-sm text-theme-light">{description}</div>
                        )}
                    </div>
                )}

                {children && (
                    <div className="px-6 py-3 overflow-y-auto overflow-x-hidden flex-1">
                        {children}
                    </div>
                )}

                {footer && (
                    <div className="px-6 py-4 border-t border-theme-subtle/60 bg-white/50">
                        {footer}
                    </div>
                )}
            </div>
        </div>
    );

    return typeof document !== 'undefined' && document.body
        ? createPortal(modalContent, document.body)
        : modalContent;
}
