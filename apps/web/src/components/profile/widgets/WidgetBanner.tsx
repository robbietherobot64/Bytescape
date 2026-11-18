import { useRef, type ChangeEvent } from 'react';
import type { ProfileData, Widget } from '../../../utils/api';

interface Props {
  widget: Widget;
  profile: ProfileData;
  editing: boolean;
  editForm?: Record<string, unknown>;
  onEditFormChange?: (form: Record<string, unknown>) => void;
}

export default function WidgetBanner({ widget, profile, editing, editForm, onEditFormChange }: Props) {
  const headerInputRef = useRef<HTMLInputElement>(null);
  const headerUrl = editing && editForm ? (editForm.header_url as string || profile.header_url || '') : profile.header_url || '';

  const handleHeaderUpload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file || !onEditFormChange) return;
    try {
      const { profileApi } = await import('../../../utils/api');
      const data = await profileApi.uploadHeader(file);
      onEditFormChange({ ...editForm, header_url: data.header_url });
    } catch {}
  };

  return (
    <div
      className="profile-banner"
      style={headerUrl ? { backgroundImage: `url(${headerUrl})` } : undefined}
    >
      {profile.is_own_profile && editing && (
        <div className="profile-banner-edit" onClick={() => headerInputRef.current?.click()}>
          Change header
        </div>
      )}
      <input ref={headerInputRef} type="file" accept="image/*" style={{ display: 'none' }} onChange={handleHeaderUpload} />
    </div>
  );
}
