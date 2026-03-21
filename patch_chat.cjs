const fs = require('fs');

// App.tsx
let appContent = fs.readFileSync('bytescape/src/App.tsx', 'utf-8');
appContent = appContent.replace(
`                  let convoId;
                  if (existingConvo && existingConvo.convo_id) {
                     convoId = existingConvo.convo_id;
                  } else {
                     convoId = await invoke<string>('get_or_create_bsky_convo', { memberDid: peerNodeId });
                  }
                  
                  setSelectedConvoId(convoId);`,
`                  let convoId;
                  if (existingConvo && existingConvo.convo_id) {
                     convoId = existingConvo.convo_id;
                  } else {
                     convoId = \`pending_bsky_\${peerNodeId}\`;
                  }
                  
                  setSelectedConvoId(convoId);`
);
fs.writeFileSync('bytescape/src/App.tsx', appContent);

// ChatScreen.tsx
let chatContent = fs.readFileSync('bytescape/src/screens/ChatScreen.tsx', 'utf-8');
chatContent = chatContent.replace(
`                const msgSource = activeSource || pendingConvoMeta?.protocol || 'bluesky';`,
`                const msgSource = activeSource || pendingConvoMeta?.protocol || 'bluesky';
                let realConvoId = selectedConvoId;
                if (selectedConvoId.startsWith('pending_bsky_')) {
                    const targetDid = pendingConvoMeta?.did || selectedConvoId.slice('pending_bsky_'.length);
                    realConvoId = await invoke<string>('get_or_create_bsky_convo', { memberDid: targetDid });
                    useChatStore.getState().setSelectedConvoId(realConvoId);
                }
`
).replace(
`                const optimisticMsg: VaultMessage = {
                    id: \`temp-\${now}\`,
                    convo_id: selectedConvoId,`,
`                const optimisticMsg: VaultMessage = {
                    id: \`temp-\${now}\`,
                    convo_id: realConvoId,`
).replace(
`                    await invoke('send_message', {
                        protocol: msgSource,
                        convoId: selectedConvoId,
                        text: text,
                    });`,
`                    await invoke('send_message', {
                        protocol: msgSource,
                        convoId: realConvoId,
                        text: text,
                    });`
).replace(
`                        await invoke<any>('send_bsky_dm', {
                            convoId: selectedConvoId,`,
`                        await invoke<any>('send_bsky_dm', {
                            convoId: realConvoId,`
);
fs.writeFileSync('bytescape/src/screens/ChatScreen.tsx', chatContent);
console.log("Patched chat optimistic bsky");
