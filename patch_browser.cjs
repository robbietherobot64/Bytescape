const fs = require('fs');
let content = fs.readFileSync('bytescape/src/utils/openInApp.ts', 'utf-8');
content = content.replace('const USE_SYSTEM_BROWSER = true;', 'const USE_SYSTEM_BROWSER = false;');
fs.writeFileSync('bytescape/src/utils/openInApp.ts', content);
