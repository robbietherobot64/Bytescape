const fs = require('fs');
let content = fs.readFileSync('bytescape/src/screens/FeedScreen.tsx', 'utf-8');

content = content.replace(
    /selectedFeedUri,\n\s*archiveSourceFilter,/g,
    `selectedFeedUris,
        selectedHashtagsFilter,
        archiveSourceFilter,`
);

fs.writeFileSync('bytescape/src/screens/FeedScreen.tsx', content);
