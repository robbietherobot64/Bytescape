const fs = require('fs');
let content = fs.readFileSync('/home/korendian/Bytescape/bytescape/src/hooks/useUnifiedFeed.ts', 'utf-8');

const targetRegex = /catch\s*\(\s*e\s*\)\s*\{\s*console\.error\s*\(\s*'get_unified_feed failed for subscribed tab:',\s*e\s*\);\s*if\s*\(!isStale\(\)\)\s*\{\s*setFeedError\(typeof\s*e\s*===\s*'string'\s*\?\s*e\s*:\s*'Failed to load subscribed feeds'\);\s*\}\s*\}/;

const replacement = `catch (e: any) {
                        console.error('get_unified_feed failed for subscribed tab:', e);
                        if (!isStale()) {
                            let errorMsg = typeof e === 'string' ? e : 'Failed to load subscribed feeds';
                            if (errorMsg.includes('no access token available') || errorMsg.includes('not logged in')) {
                                errorMsg = 'Connect Bluesky to use subscribed feeds';
                            }
                            setFeedError(errorMsg);
                        }
                    }`;

content = content.replace(targetRegex, replacement);
fs.writeFileSync('/home/korendian/Bytescape/bytescape/src/hooks/useUnifiedFeed.ts', content);
console.log("Feed patched");
