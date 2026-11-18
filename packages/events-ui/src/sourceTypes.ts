/** Shared source-panel types. These mirror the main feed's Category/CategoryItem
 * shape so the same panel renders in both surfaces without an app coupling. */

export interface SourceCategoryItem {
  id: string;
  label: string;
  avatar?: string | null;
  source_id: string;
  source_type?: string;
  icon?: string;
}

export interface SourceCategory {
  id: string;
  label: string;
  icon: string;
  items: SourceCategoryItem[];
}

/** A generated topic feed from /api/feedgen/topics. */
export interface FeedgenTopic {
  rkey: string;
  name: string;
  description: string;
  tags: string[];
}

/** One search/add result from the host's global source search. */
export interface SourceSearchResult {
  label: string;
  source_id?: string;
  id?: string;
  description?: string;
  avatar?: string;
  source_type?: string;
  uri?: string;
  tag?: string;
  did?: string;
  platform?: string;
}
