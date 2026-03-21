const fs = require('fs');

let content = fs.readFileSync('backup_useunifiedfeed.txt', 'utf-8');

content = content.replace(
    /selectedFeedUri: string;\n\s*archiveSourceFilter: string;/,
    `selectedFeedUris: string[];
    selectedHashtagsFilter: string[];
    archiveSourceFilter: string;`
);

content = content.replace(
    /selectedFeedUri,\n\s*archiveSourceFilter,/,
    `selectedFeedUris,
    selectedHashtagsFilter,
    archiveSourceFilter,`
);

// We need to change how activeFeedUris is calculated
let oldActiveFeedUris = `                // Respect selectedFeedUri — filter to a single feed if selected
                const activeFeedUris = selectedFeedUri !== FEED_FILTER_ALL && allFeedUris.includes(selectedFeedUri)
                    ? [selectedFeedUri]
                    : allFeedUris;

                if (activeFeedUris.length > 0 || tags.length > 0) {
                    try {
                        const [posts, nextCur] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                            tags,`;

let newActiveFeedUris = `                const activeFeedUris = selectedFeedUris.length > 0
                    ? selectedFeedUris.filter(uri => allFeedUris.includes(uri))
                    : allFeedUris;
                
                const activeTags = selectedHashtagsFilter.length > 0
                    ? selectedHashtagsFilter.filter(tag => tags.includes(tag))
                    : tags;

                if (activeFeedUris.length > 0 || activeTags.length > 0) {
                    try {
                        const [posts, nextCur] = await invoke<[BlueskyPost[], string | null]>('get_unified_feed', {
                            tags: activeTags,`;

content = content.replace(oldActiveFeedUris, newActiveFeedUris);

// useEffect dependencies
content = content.replace(
    /const cacheKey = archiveSourceFilter \? \`\$\{tab\}:\$\{archiveSourceFilter\}\` : tab;/g,
    "const cacheKey = `${tab}:${archiveSourceFilter}:${selectedFeedUris.join(',')}:${selectedHashtagsFilter.join(',')}`;"
);

content = content.replace(
    /\s*clearCache\('subscribed'\);\n\s*\}, \[selectedFeedUri, clearCache\]\);/g,
    `\n        clearCache('subscribed');\n    }, [selectedFeedUris, selectedHashtagsFilter, clearCache]);`
);

content = content.replace(
    /\[auth, activeTab, selectedFeedUri, archiveSourceFilter, fetchFeed\]/g,
    `[auth, activeTab, selectedFeedUris, selectedHashtagsFilter, archiveSourceFilter, fetchFeed]`
);

fs.writeFileSync('bytescape/src/hooks/useUnifiedFeed.ts', content);
console.log("Patched useUnifiedFeed.ts");
