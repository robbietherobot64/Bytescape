import { useState } from "react";
import { eventsApi } from "./api";

interface Props {
  onClose: () => void;
  onCreated: (uri: string) => void;
}

/** Create form (spec: events/participate — field-level validation errors
 * preserve state; image uploads before submit and travels in `uris`). */
export default function CreateEventModal({ onClose, onCreated }: Props) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [startsAt, setStartsAt] = useState("");
  const [endsAt, setEndsAt] = useState("");
  const [mode, setMode] = useState<"" | "inperson" | "virtual" | "hybrid">("");
  const [location, setLocation] = useState("");
  const [image, setImage] = useState<File | null>(null);
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const toRfc3339 = (localValue: string) =>
    localValue ? new Date(localValue).toISOString() : undefined;

  const submit = async () => {
    if (!name.trim()) {
      setError("name is required");
      return;
    }
    if (startsAt && endsAt && new Date(endsAt) <= new Date(startsAt)) {
      setError("end time must be after start time");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      let pictureUrl = imageUrl;
      if (image && !pictureUrl) {
        pictureUrl = await eventsApi.uploadImage(image);
        setImageUrl(pictureUrl);
      }
      const created = await eventsApi.create({
        name: name.trim(),
        description: description.trim() || undefined,
        starts_at: toRfc3339(startsAt),
        ends_at: toRfc3339(endsAt),
        mode: mode || undefined,
        location: location.trim() || undefined,
        picture_url: pictureUrl ?? undefined,
      });
      onCreated(created.id);
      onClose();
    } catch (e) {
      // Form state is preserved for retry (spec: creation failure not silent).
      setError(e instanceof Error ? e.message : "creation failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="ev-modal-overlay" onClick={onClose}>
      <div className="ev-modal" onClick={(e) => e.stopPropagation()}>
        <h3 className="ev-panel-title">&gt; NEW EVENT</h3>

        <label className="ev-field">
          <span>Name *</span>
          <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Event name" />
        </label>

        <label className="ev-field">
          <span>Description</span>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
            placeholder="What's happening?"
          />
        </label>

        <div className="ev-field-row">
          <label className="ev-field">
            <span>Starts</span>
            <input type="datetime-local" value={startsAt} onChange={(e) => setStartsAt(e.target.value)} />
          </label>
          <label className="ev-field">
            <span>Ends</span>
            <input type="datetime-local" value={endsAt} onChange={(e) => setEndsAt(e.target.value)} />
          </label>
        </div>

        <div className="ev-field-row">
          <label className="ev-field">
            <span>Mode</span>
            <select value={mode} onChange={(e) => setMode(e.target.value as typeof mode)}>
              <option value="">—</option>
              <option value="inperson">In person</option>
              <option value="virtual">Virtual</option>
              <option value="hybrid">Hybrid</option>
            </select>
          </label>
          <label className="ev-field">
            <span>Location</span>
            <input
              value={location}
              onChange={(e) => setLocation(e.target.value)}
              placeholder="venue or city"
            />
          </label>
        </div>

        <label className="ev-field">
          <span>Image</span>
          <input
            type="file"
            accept="image/*"
            onChange={(e) => {
              const f = e.target.files?.[0] ?? null;
              setImage(f);
              setImageUrl(null);
            }}
          />
        </label>
        {image && (
          <img className="ev-modal-preview" src={URL.createObjectURL(image)} alt="" />
        )}

        {error && <p className="ev-error">{error}</p>}

        <div className="ev-modal-actions">
          <button type="button" className="ev-btn" onClick={onClose} disabled={busy}>
            CANCEL
          </button>
          <button type="button" className="ev-btn ev-btn-primary" onClick={submit} disabled={busy}>
            {busy ? "PUBLISHING…" : "> PUBLISH EVENT"}
          </button>
        </div>
      </div>
    </div>
  );
}
