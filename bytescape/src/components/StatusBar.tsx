/**
 * StatusBar
 *
 * Lightweight transient feedback strip for async UI operations.
 *
 * Cross-system note:
 * - Messages shown here may originate from both lens-only UI actions and engine command
 *   results surfaced by parent components.
 */
import React from 'react';

/** Props for the exported `StatusBar` component. */
interface StatusBarProps {
    /** Human-readable status message. Empty string hides the component. */
    status: string;
    /** True while a related operation is actively running. */
    isBusy: boolean;
    /** Optional callback to clear non-busy status messages. */
    onDismiss?: () => void;
}

// ============================================================
// Status Presentation Component
// ============================================================

/**
 * Exported status strip used by screens that need persistent inline feedback.
 */
export const StatusBar: React.FC<StatusBarProps> = ({ status, isBusy, onDismiss }) => {
    // Hidden state: no message to render.
    if (!status) return null;

    return (
        <div className={`status-bar ${isBusy ? 'busy' : 'idle'}`}>
            <div className="status-content">
                {isBusy && <span className="status-spinner">⏳</span>}
                <span className="status-text">{status}</span>
            </div>
            {/* Dismiss affordance is intentionally disabled while busy to avoid ambiguous state. */}
            {!isBusy && onDismiss && (
                <button className="status-dismiss" onClick={onDismiss}>
                    ✕
                </button>
            )}
        </div>
    );
};
