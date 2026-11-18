import type { Widget } from '../../../utils/api';

interface Props {
  widget: Widget;
  editing: boolean;
  onDataChange?: (data: Record<string, unknown>) => void;
}

export default function WidgetStatus({ widget, editing, onDataChange }: Props) {
  const text = (widget.data?.text as string) || '';

  if (editing) {
    return (
      <div className="widget-status-editor">
        <div className="profile-edit-field">
          <label className="profile-edit-label">Status / Mood</label>
          <input
            className="profile-edit-input"
            placeholder="What's on your mind?"
            value={text}
            onChange={e => onDataChange?.({ ...widget.data, text: e.target.value })}
          />
        </div>
      </div>
    );
  }

  if (!text) {
    return <div style={{ color: 'var(--text-muted)', fontStyle: 'italic', fontSize: '13px', padding: '8px 0' }}>No status set</div>;
  }

  return (
    <div className="widget-status" style={{ fontSize: '14px', lineHeight: '1.5', padding: '4px 0', fontStyle: 'italic' }}>
      💬 {text}
    </div>
  );
}
