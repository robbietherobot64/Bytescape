import { render, screen } from '@testing-library/react';
import { SourceBadge } from './SourceBadge';

describe('SourceBadge', () => {
  it('renders known source with emoji', () => {
    render(<SourceBadge source="bluesky" />);
    expect(screen.queryByText('🦋 bluesky')).not.toBeNull();
  });

  it('falls back for unknown source', () => {
    render(<SourceBadge source="custom" />);
    expect(screen.queryByText('📝 custom')).not.toBeNull();
  });
});
