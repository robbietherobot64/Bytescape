import { useEffect, useState } from 'react';
import { isStoryQuoteUri, storiesApi, storyMediaUrl } from '../../utils/api';
import type { EmbedInfo, StoryItem } from '../../utils/api';

/**
 * Feed card for a reshared story (a quote embed whose target is a
 * com.bytescape.story record). Links to the story viewer.
 */
export default function StoryPreviewCard({
  embeds,
  onOpenStory,
}: {
  embeds: EmbedInfo[];
  onOpenStory: (uri: string) => void;
}) {
  const quote = embeds.find((e) => e.kind === 'quote' && isStoryQuoteUri(e.url));
  const [item, setItem] = useState<StoryItem | null>(null);

  useEffect(() => {
    if (!quote) return;
    storiesApi.item(quote.url).then(setItem).catch(() => setItem(null));
  }, [quote?.url]);

  if (!quote) return null;

  return (
    <button className="story-preview-card" onClick={() => onOpenStory(quote.url)}>
      <span className="story-preview-label">📷 shared a story</span>
      {item?.kind === 'status' ? (
        <span
          className="story-preview-text"
          style={{ background: item.style?.bg?.color || '#0a2a0a' }}
        >
          {(item.text || '').slice(0, 80)}
        </span>
      ) : item?.media?.[0] ? (
        item.media[0].type === 'video' ? (
          <video src={storyMediaUrl(item.author_did, item.media[0])} className="story-preview-media" muted />
        ) : (
          <img src={storyMediaUrl(item.author_did, item.media[0])} alt="" className="story-preview-media" />
        )
      ) : (
        <span className="story-preview-text">{item === null ? 'View story…' : 'Story unavailable'}</span>
      )}
      {item?.text && item.kind === 'story' && <span className="story-preview-caption">{item.text.slice(0, 80)}</span>}
    </button>
  );
}
