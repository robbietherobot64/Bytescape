import { useState } from 'react';
import { INTEREST_CATEGORIES, getTotalInterestCount } from '../utils/interests';
import { saveInterests } from './FeedColumn';

const TOTAL_INTERESTS = getTotalInterestCount();

interface Props {
  onComplete: () => void;
  onSkip: () => void;
}

export default function InterestStep({ onComplete, onSkip }: Props) {
  const [selectedInterests, setSelectedInterests] = useState<string[]>([]);
  const [expandedCat, setExpandedCat] = useState<string | null>(INTEREST_CATEGORIES[0]?.id || null);

  const handleSave = async () => {
    if (selectedInterests.length < 3) return;
    await saveInterests(selectedInterests);
    onComplete();
  };

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 10003,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'rgba(0, 0, 0, 0.8)',
      }}
    >
      <div
        style={{
          background: 'var(--bg-secondary, #1a1a2e)',
          border: '1px solid var(--accent)',
          borderRadius: 8,
          padding: 20,
          maxWidth: 520,
          width: '90%',
          maxHeight: '80vh',
          overflow: 'hidden',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        <div style={{ fontFamily: 'var(--font-retro)', fontSize: 20, color: 'var(--accent)', marginBottom: 8 }}>
          Pick your interests
        </div>
        <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 16 }}>
          Your feed will be populated with content matching your selections. Pick any that interest you.
        </p>

        <div style={{ overflowY: 'auto', flex: 1, marginBottom: 16 }}>
          {INTEREST_CATEGORIES.map(cat => (
            <div key={cat.id} style={{ marginBottom: 4 }}>
              <div
                onClick={() => setExpandedCat(expandedCat === cat.id ? null : cat.id)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  padding: '8px 12px',
                  borderRadius: 4,
                  cursor: 'pointer',
                  background: expandedCat === cat.id ? 'rgba(0, 255, 65, 0.05)' : 'transparent',
                  borderBottom: '1px solid var(--border-color)',
                }}
              >
                <span style={{ fontFamily: 'var(--font-retro)', fontSize: 14, flex: 1 }}>
                  {cat.label}
                </span>
                <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>
                  {selectedInterests.filter(id => cat.interests.some(i => i.id === id)).length}/{cat.interests.length}
                </span>
                <span style={{
                  color: 'var(--text-muted)',
                  fontSize: 10,
                  transform: expandedCat === cat.id ? 'rotate(90deg)' : 'none',
                  transition: 'transform 0.2s',
                }}>
                  ▶
                </span>
              </div>
              {expandedCat === cat.id && (
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, padding: '8px 12px' }}>
                  {cat.interests.map(interest => {
                    const isSelected = selectedInterests.includes(interest.id);
                    return (
                      <span
                        key={interest.id}
                        onClick={() => setSelectedInterests(prev =>
                          prev.includes(interest.id)
                            ? prev.filter(id => id !== interest.id)
                            : [...prev, interest.id]
                        )}
                        className={`chip ${isSelected ? 'selected' : ''}`}
                        style={{ cursor: 'pointer' }}
                      >
                        {interest.emoji} {interest.label}
                      </span>
                    );
                  })}
                </div>
              )}
            </div>
          ))}
        </div>

        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', borderTop: '1px solid var(--border-color)', paddingTop: 12 }}>
          <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>
            {selectedInterests.length < 3
              ? `Pick at least 3 (${selectedInterests.length}/3)`
              : `${selectedInterests.length} of ${TOTAL_INTERESTS} selected`}
          </span>
          <div style={{ display: 'flex', gap: 8 }}>
            <button className="btn-link" onClick={onSkip} style={{ color: 'var(--text-muted)', fontSize: 12 }}>
              Skip
            </button>
            <button
              className="btn"
              onClick={handleSave}
              disabled={selectedInterests.length < 3}
              style={{
                background: selectedInterests.length >= 3 ? 'var(--accent)' : undefined,
                color: selectedInterests.length >= 3 ? '#000' : undefined,
                border: selectedInterests.length >= 3 ? 'none' : undefined,
                fontWeight: 600,
                fontSize: 12,
                padding: '8px 14px',
                opacity: selectedInterests.length >= 3 ? 1 : 0.4,
              }}
            >
              {selectedInterests.length < 3 ? 'Pick at least 3' : `Save ${selectedInterests.length} interests`}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
