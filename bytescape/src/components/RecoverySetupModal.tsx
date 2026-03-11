import { useState } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { Win98Box } from './Win98Box';
import { IdentityInitResult, RecoveryEscrowBundle } from '../types';

interface RecoverySetupModalProps {
    /** Called when the user finishes (or skips) recovery setup */
    onComplete: () => void;
}

/**
 * RecoverySetupModal — shown after identity + username setup.
 *
 * Two-step flow:
 *   1. User enters a recovery code (≥8 chars) + confirms it
 *   2. The escrow bundle is displayed with a Copy button
 *
 * Calls `identity_init_full_recovery` which re-encrypts the existing
 * Iroh keypair with the user's chosen recovery code.
 */
export function RecoverySetupModal({ onComplete }: RecoverySetupModalProps) {
    const [step, setStep] = useState<'code' | 'bundle'>('code');
    const [code, setCode] = useState('');
    const [codeConfirm, setCodeConfirm] = useState('');
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState('');
    const [bundle, setBundle] = useState<RecoveryEscrowBundle | null>(null);
    const [bundleJson, setBundleJson] = useState('');
    const [copied, setCopied] = useState(false);

    function validateCode(): string | null {
        if (code.length < 8) return 'Recovery code must be at least 8 characters.';
        if (code !== codeConfirm) return 'Codes do not match.';
        return null;
    }

    async function handleCreateEscrow() {
        const validationError = validateCode();
        if (validationError) {
            setError(validationError);
            return;
        }
        setSaving(true);
        setError('');
        try {
            const result = await invoke<IdentityInitResult>('identity_init_full_recovery', {
                recoveryCode: code,
            });
            // Parse the bundle JSON for display metadata
            const parsed = JSON.parse(result.escrow_bundle) as RecoveryEscrowBundle;
            setBundle(parsed);
            setBundleJson(result.escrow_bundle);
            setStep('bundle');
        } catch (e: any) {
            setError(typeof e === 'string' ? e : e.message || 'Failed to create recovery bundle');
        } finally {
            setSaving(false);
        }
    }

    async function handleCopy() {
        try {
            await navigator.clipboard.writeText(bundleJson);
            setCopied(true);
            setTimeout(() => setCopied(false), 2000);
        } catch {
            // Fallback: select the text in the textarea
            const el = document.querySelector('.recovery-bundle-text') as HTMLTextAreaElement;
            if (el) {
                el.select();
                document.execCommand('copy');
                setCopied(true);
                setTimeout(() => setCopied(false), 2000);
            }
        }
    }

    return (
        <div className="identity-setup-overlay">
            <Win98Box variant="window" title="Secure Your Identity" className="recovery-setup-modal">
                <div className="recovery-setup-content">
                    {step === 'code' && (
                        <>
                            <div className="recovery-setup-icon">🔐</div>
                            <h3 className="recovery-setup-heading">
                                Create a Recovery Code
                            </h3>
                            <p className="recovery-setup-description">
                                This code encrypts a backup of your identity key. If you ever lose
                                this device, you can recover your identity on a new one using
                                this code and the escrow bundle it generates.
                            </p>

                            <div className="recovery-input-group">
                                <label className="recovery-label">Recovery Code</label>
                                <input
                                    className="input-field"
                                    type="password"
                                    placeholder="At least 8 characters"
                                    value={code}
                                    onChange={(e) => { setCode(e.target.value); setError(''); }}
                                    onKeyDown={(e) => e.key === 'Enter' && handleCreateEscrow()}
                                    autoFocus
                                />
                            </div>

                            <div className="recovery-input-group">
                                <label className="recovery-label">Confirm Code</label>
                                <input
                                    className="input-field"
                                    type="password"
                                    placeholder="Enter code again"
                                    value={codeConfirm}
                                    onChange={(e) => { setCodeConfirm(e.target.value); setError(''); }}
                                    onKeyDown={(e) => e.key === 'Enter' && handleCreateEscrow()}
                                />
                            </div>

                            <div className="recovery-strength">
                                {code.length > 0 && (
                                    <span className={`recovery-strength-badge ${code.length >= 16 ? 'strong' : code.length >= 8 ? 'ok' : 'weak'}`}>
                                        {code.length >= 16 ? '🟢 Strong' : code.length >= 8 ? '🟡 OK' : '🔴 Too short'}
                                    </span>
                                )}
                            </div>

                            {error && <p className="recovery-error">{error}</p>}

                            <button
                                className="btn btn-primary btn-block"
                                onClick={handleCreateEscrow}
                                disabled={saving || code.length < 8}
                            >
                                {saving ? 'Encrypting...' : 'Generate Recovery Bundle'}
                            </button>

                            <button
                                className="btn btn-ghost btn-block recovery-skip-btn"
                                onClick={onComplete}
                            >
                                Skip for now
                            </button>

                            <p className="recovery-fine-print">
                                ⚠️ Without a recovery code, losing this device means losing your identity permanently.
                            </p>
                        </>
                    )}

                    {step === 'bundle' && bundle && (
                        <>
                            <div className="recovery-setup-icon">✅</div>
                            <h3 className="recovery-setup-heading">
                                Recovery Bundle Created
                            </h3>
                            <p className="recovery-setup-description">
                                Copy this bundle and store it somewhere safe — a password manager,
                                encrypted note, or printed paper backup. You'll need both the
                                bundle <strong>and</strong> your recovery code to restore your identity.
                            </p>

                            <div className="recovery-bundle-meta">
                                <div className="recovery-meta-row">
                                    <span className="recovery-meta-label">DID</span>
                                    <span className="recovery-meta-value" title={bundle.did}>
                                        {bundle.did.slice(0, 20)}...{bundle.did.slice(-8)}
                                    </span>
                                </div>
                                <div className="recovery-meta-row">
                                    <span className="recovery-meta-label">Created</span>
                                    <span className="recovery-meta-value">
                                        {new Date(bundle.created_at * 1000).toLocaleString()}
                                    </span>
                                </div>
                            </div>

                            <textarea
                                className="recovery-bundle-text"
                                value={bundleJson}
                                readOnly
                                rows={6}
                                onClick={(e) => (e.target as HTMLTextAreaElement).select()}
                            />

                            <button
                                className="btn btn-primary btn-block"
                                onClick={handleCopy}
                            >
                                {copied ? '✓ Copied!' : '📋 Copy Bundle to Clipboard'}
                            </button>

                            <button
                                className="btn btn-primary btn-block"
                                onClick={onComplete}
                                style={{ marginTop: 8 }}
                            >
                                Done — I've saved my bundle
                            </button>

                            <p className="recovery-fine-print">
                                🔒 The bundle is encrypted with your recovery code. Nobody — not even Bytescape — can read it without the code.
                            </p>
                        </>
                    )}
                </div>
            </Win98Box>
        </div>
    );
}
