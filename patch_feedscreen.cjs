const fs = require('fs');
let content = fs.readFileSync('backup_feedscreen.txt', 'utf-8');

content = content.replace(
    /const \[selectedFeedUri, setSelectedFeedUri\] = useState<string>\(FEED_FILTER_ALL\);/,
    `const [selectedFeedUris, setSelectedFeedUris] = useState<string[]>([]);
    const [selectedHashtagsFilter, setSelectedHashtagsFilter] = useState<string[]>([]);`
);

content = content.replace(
    /selectedFeedUri={selectedFeedUri}\s*setSelectedFeedUri={setSelectedFeedUri}/g,
    `selectedFeedUris={selectedFeedUris}
                                setSelectedFeedUris={setSelectedFeedUris}
                                selectedHashtagsFilter={selectedHashtagsFilter}
                                setSelectedHashtagsFilter={setSelectedHashtagsFilter}`
);

fs.writeFileSync('bytescape/src/screens/FeedScreen.tsx', content);
