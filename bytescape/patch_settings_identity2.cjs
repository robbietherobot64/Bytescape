const fs = require('fs');
let content = fs.readFileSync('/home/korendian/Bytescape/bytescape/src/screens/SettingsScreen.tsx', 'utf-8');

const targetRegex = /<h3>🪪 Identity<\/h3>[\s\S]*?(?=<\/div>\s*<\/div>\s*{\/\* Appearance \/ Theme \*\/})/;
const match = content.match(targetRegex);
if(match) {
    console.log("Matched!");
} else {
    console.log("Not matched!");
}
