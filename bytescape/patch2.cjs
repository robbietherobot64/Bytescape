const fs = require('fs');
let content = fs.readFileSync('src/screens/SettingsScreen.tsx', 'utf-8');
content = content.replace(/const \[linkSuccess, setLinkSuccess\] = useState\(""\);/g, 'const [, setLinkSuccess] = useState<string>("");');
fs.writeFileSync('src/screens/SettingsScreen.tsx', content);
