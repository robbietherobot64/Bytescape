import React from 'react';

interface StatusBarProps {
    status: string;
    isBusy: boolean;
    onDismiss?: () => void;
}

export const StatusBar: React.FC<StatusBarProps> = ({ status, isBusy, onDismiss }) => {
    if (!status) return null;

    return (
        <div className={`status-bar ${isBusy ? 'busy' : 'idle'}`}>
            <div className="status-content">
                {isBusy && <span className="status-spinner">⏳</span>}
                <span className="status-text">{status}</span>
            </div>
            {!isBusy && onDismiss && (
                <button className="status-dismiss" onClick={onDismiss}>
                    ✕
                </button>
            )}
        </div>
    );
};
