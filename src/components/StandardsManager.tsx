import { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { standardsService } from '../services/standardsService';
import { VoucherStandardInfo, VoucherStandardDefinition } from '../types';
import { Button } from './ui/Button';
import { Card } from './ui/Card';
import { ConfirmationModal } from './ui/ConfirmationModal';
import { ExportStandardModal } from './ExportStandardModal';
import { ImportStandardModal } from './ImportStandardModal';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { FileCode, Download, Trash2, Plus, ShieldCheck, CheckCircle2 } from 'lucide-react';
import { logger } from '../utils/log';
import { translateError } from '../utils/errorHelper';

export function StandardsManager() {
    const { t } = useTranslation();
    const [standards, setStandards] = useState<VoucherStandardInfo[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState('');
    const [successMsg, setSuccessMsg] = useState('');

    // Modal States
    const [exportTarget, setExportTarget] = useState<{ id: string; name: string } | null>(null);
    const [deleteTarget, setDeleteTarget] = useState<VoucherStandardInfo | null>(null);
    const [isDeleting, setIsDeleting] = useState(false);
    const [importModalData, setImportModalData] = useState<{
        containerBytes: number[];
        definition?: VoucherStandardDefinition;
    } | null>(null);

    const loadStandards = async () => {
        setIsLoading(true);
        setError('');
        try {
            const list = await standardsService.getStandards();
            setStandards(list);
        } catch (e) {
            setError(translateError(e, t));
        } finally {
            setIsLoading(false);
        }
    };

    useEffect(() => {
        loadStandards();
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    const handleImportClick = async () => {
        setError('');
        setSuccessMsg('');
        try {
            const selectedPath = await open({
                multiple: false,
                filters: [{ name: t('standards.formatDesc'), extensions: ['standard'] }]
            });

            if (typeof selectedPath === 'string') {
                const fileUint8Array = await readFile(selectedPath);
                const bytes = Array.from(fileUint8Array);
                try {
                    const def = await standardsService.inspectStandardFile(bytes, null);
                    setImportModalData({ containerBytes: bytes, definition: def });
                } catch {
                    setImportModalData({ containerBytes: bytes });
                }
            }
        } catch (e) {
            setError(translateError(e, t));
        }
    };

    const handleDeleteConfirm = async () => {
        if (!deleteTarget) return;
        setIsDeleting(true);
        setError('');
        setSuccessMsg('');
        try {
            await standardsService.deleteStandard(deleteTarget.id);
            logger.info(`Deleted standard ${deleteTarget.id}`);
            setSuccessMsg(`Standard '${deleteTarget.displayName}' deleted successfully.`);
            setDeleteTarget(null);
            await loadStandards();
        } catch (e) {
            const translated = translateError(e, t);
            if (translated.includes("in use") || translated.includes("VoucherStandardInUse")) {
                setError(t('standards.inUseError'));
            } else {
                setError(translated);
            }
        } finally {
            setIsDeleting(false);
        }
    };

    return (
        <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
            {/* Header Actions */}
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 p-6 bg-gradient-to-br from-theme-primary/5 via-theme-primary/10 to-transparent rounded-3xl border border-theme-primary/15 shadow-sm">
                <div>
                    <h3 className="text-lg font-black text-theme-primary tracking-tight">{t('standards.title')}</h3>
                    <p className="text-xs text-theme-light font-medium mt-0.5">{t('standards.description')}</p>
                </div>
                <Button
                    onClick={handleImportClick}
                    className="flex items-center gap-2 px-5 py-3 shadow-md hover:shadow-lg transition-all"
                >
                    <Plus size={16} />
                    <span>{t('standards.import')}</span>
                </Button>
            </div>

            {/* Status Messages */}
            {error && (
                <div className="p-4 bg-rose-500/10 border border-rose-500/20 text-rose-600 rounded-2xl text-xs font-bold animate-shake">
                    {error}
                </div>
            )}
            {successMsg && (
                <div className="p-4 bg-emerald-500/10 border border-emerald-500/20 text-emerald-600 rounded-2xl text-xs font-bold animate-fade-in flex items-center gap-2">
                    <CheckCircle2 size={16} />
                    <span>{successMsg}</span>
                </div>
            )}

            {/* Standards List */}
            {isLoading ? (
                <div className="py-12 text-center text-xs font-black uppercase text-theme-light tracking-widest animate-pulse">
                    {t('common.loading')}
                </div>
            ) : standards.length === 0 ? (
                <div className="py-12 text-center bg-white/40 rounded-3xl border border-theme-subtle">
                    <FileCode size={36} className="mx-auto text-theme-light mb-3 opacity-40" />
                    <p className="text-sm font-bold text-theme-secondary">No voucher standards found.</p>
                </div>
            ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                    {standards.map((std) => (
                        <Card key={std.id} className="relative overflow-hidden flex flex-col justify-between p-6">
                            <div className="space-y-4">
                                <div className="flex items-start justify-between gap-3">
                                    <div className="flex items-center gap-3">
                                        <div className="p-3 bg-theme-primary/10 text-theme-primary rounded-2xl">
                                            <ShieldCheck size={24} />
                                        </div>
                                        <div>
                                            <h4 className="text-base font-black text-theme-primary tracking-tight">
                                                {std.displayName}
                                            </h4>
                                            <p className="text-[10px] font-mono text-theme-light truncate max-w-[180px]">
                                                {std.id}
                                            </p>
                                        </div>
                                    </div>
                                    <span className="text-[10px] font-black uppercase tracking-wider bg-theme-primary/10 text-theme-primary px-2.5 py-1 rounded-full border border-theme-primary/20">
                                        TOML
                                    </span>
                                </div>
                            </div>

                            {/* Actions */}
                            <div className="flex items-center gap-2 pt-6 mt-4 border-t border-slate-100">
                                <Button
                                    variant="secondary"
                                    onClick={() => setExportTarget({ id: std.id, name: std.displayName })}
                                    className="flex-1 flex items-center justify-center gap-2 text-xs py-2.5"
                                >
                                    <Download size={14} />
                                    <span>{t('standards.export')}</span>
                                </Button>

                                <button
                                    onClick={() => setDeleteTarget(std)}
                                    className="p-2.5 rounded-xl border border-rose-200 text-rose-500 hover:bg-rose-50 hover:border-rose-300 transition-all"
                                    title={t('standards.delete')}
                                >
                                    <Trash2 size={16} />
                                </button>
                            </div>
                        </Card>
                    ))}
                </div>
            )}

            {/* Export Modal */}
            {exportTarget && (
                <ExportStandardModal
                    isOpen={true}
                    standardId={exportTarget.id}
                    standardName={exportTarget.name}
                    onClose={() => setExportTarget(null)}
                    onSuccess={() => {
                        setSuccessMsg(`Standard exported successfully.`);
                        setExportTarget(null);
                    }}
                />
            )}

            {/* Import Modal */}
            {importModalData && (
                <ImportStandardModal
                    isOpen={true}
                    containerBytes={importModalData.containerBytes}
                    initialDefinition={importModalData.definition}
                    onClose={() => setImportModalData(null)}
                    onSuccess={(importedId) => {
                        setSuccessMsg(`${t('standards.importSuccess')} (${importedId})`);
                        setImportModalData(null);
                        loadStandards();
                    }}
                />
            )}

            {/* Delete Confirmation Modal */}
            {deleteTarget && (
                <ConfirmationModal
                    isOpen={true}
                    title={t('standards.delete')}
                    description={
                        <div className="space-y-3 pt-2 text-left">
                            <p className="text-xs font-bold text-theme-primary">{t('standards.deleteConfirm')}</p>
                            <div className="p-3 bg-slate-50 rounded-xl border border-slate-200 text-xs">
                                <span className="font-bold text-theme-primary block">{deleteTarget.displayName}</span>
                                <span className="text-[10px] font-mono text-theme-light">{deleteTarget.id}</span>
                            </div>
                        </div>
                    }
                    confirmText={t('standards.delete')}
                    onConfirm={handleDeleteConfirm}
                    onCancel={() => setDeleteTarget(null)}
                    isProcessing={isDeleting}
                    confirmVariant="danger"
                />
            )}
        </div>
    );
}
