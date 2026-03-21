const fs = require('fs');
let content = fs.readFileSync('/home/korendian/Bytescape/bytescape/src/screens/SettingsScreen.tsx', 'utf-8');

const importReplacement = `import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { AuthStatus, LinkedAccount } from '../types';`;

content = content.replace(/import \{ useState \} from 'react';\nimport \{ invoke \} from "@tauri-apps\/api\/core";\nimport \{ save \} from "@tauri-apps\/plugin-dialog";\nimport \{ AuthStatus \} from '\.\.\/types';/, importReplacement);

fs.writeFileSync('/home/korendian/Bytescape/bytescape/src/screens/SettingsScreen.tsx', content);
