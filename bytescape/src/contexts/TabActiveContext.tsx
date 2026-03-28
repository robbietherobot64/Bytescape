import { createContext, useContext } from 'react';

/**
 * Context that tells child components whether their parent tab is currently
 * the active (visible) tab in the app shell.
 *
 * When a tab is inactive (hidden via display:none), VideoPlayer and other
 * heavy-resource components use this signal to release native resources
 * through React's normal state management — no DOM hacking needed.
 *
 * This solves the GStreamer pipeline leak: previously, VideoPlaybackManager
 * used imperative DOM manipulation (removeAttribute('src'), el.load()) to
 * release pipelines on tab switch, but React would immediately restore the
 * src from its state on the next render, recreating the pipeline.
 */
export const TabActiveContext = createContext<boolean>(true);

/** Returns true if the current component is inside the active tab. */
export function useTabActive(): boolean {
    return useContext(TabActiveContext);
}
