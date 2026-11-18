// @bytescape/events-ui — one Events surface for both hosts.
// Hosts call configureEventsApi once at boot (web passes its token getter;
// the events SPA relies on the family session cookie), then render
// <EventsLayout viewer onRequireSignIn …>.
import "./styles.css";

export { default as EventsLayout } from "./EventsLayout";
export type { EventsViewer } from "./EventsLayout";
export { default as SourcesPanel } from "./SourcesPanel";
export { default as SourceSearch } from "./SourceSearch";
export type {
  SourceCategory,
  SourceCategoryItem,
  SourceSearchResult,
  FeedgenTopic,
} from "./sourceTypes";
export { default as EventsColumn } from "./EventsColumn";
export { default as EventsRightPanel } from "./EventsRightPanel";
export { default as EventDetail } from "./EventDetail";
export { default as CreateEventModal } from "./CreateEventModal";
export { formatEventDate, eventStatusLabel } from "./format";
export { configureEventsApi, eventsApi } from "./api";
export type {
  EventItem,
  EventProvenance,
  EventsPage,
  EventComment,
  RsvpCounts,
  EventAuthor,
  SourcePerson,
  SourceFeed,
  SourceHashtag,
  EventSources,
  SourceTab,
  SourceGroup,
  SourceScope,
  ViewerSession,
  ListParams,
  CreateEventBody,
} from "./api";
