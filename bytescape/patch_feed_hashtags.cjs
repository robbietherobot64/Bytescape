const fs = require('fs');
let content = fs.readFileSync('/home/korendian/Bytescape/bytescape/src/hooks/useUnifiedFeed.ts', 'utf-8');

const target1 = `const [feedsData, response] = await Promise.all([
                    invoke<GeneratorView[]>('list_saved_feeds').catch((): GeneratorView[] => []),
                    invoke<{ items: FeedEntry[]; next_cursor: string | null }>('get_timeline_unified', {
                        tags: [],`;

const repl1 = `const [feedsData, tags, response] = await Promise.all([
                    invoke<GeneratorView[]>('list_saved_feeds').catch((): GeneratorView[] => []),
                    invoke<string[]>('list_followed_hashtags').catch((): string[] => []),
                    invoke<{ items: FeedEntry[]; next_cursor: string | null }>('get_timeline_unified', {
                        tags: [],`;

content = content.replace(target1, repl1);

const target2 = `                    if (isStale()) return;
                    setSavedFeeds(feedsData);
                    setCachedSavedFeeds(feedsData);`;

const repl2 = `                    if (isStale()) return;
                    setSavedFeeds(feedsData);
                    setCachedSavedFeeds(feedsData);
                    setFollowedHashtags(tags);`;

content = content.replace(target2, repl2);

fs.writeFileSync('/home/korendian/Bytescape/bytescape/src/hooks/useUnifiedFeed.ts', content);
console.log("Patched hashtags in UseUnifiedFeed");
