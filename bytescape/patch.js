const fs = require('fs');
let content = fs.readFileSync('/home/korendian/Bytescape/bytescape/src/App.tsx', 'utf-8');

const target = `            onOpenChat={async (peerNodeId, peerName) => {
              const { setSelectedConvoId, setPendingConvoMeta } = useChatStore.getState();
              if (peerNodeId.startsWith('did:plc:') || peerNodeId.startsWith('did:web:')) {
                try {
                  const convoId = await invoke<string>('get_or_create_bsky_convo', { memberDid: peerNodeId });
                  setSelectedConvoId(convoId);
                  setPendingConvoMeta({ did: peerNodeId, handle: peerName || peerNodeId, protocol: 'bluesky' });
                } catch (e) {
                  console.error('Failed to get/create convo:', e);
                }
              } else if (peerNodeId.startsWith('http') || (peerName && peerName.includes('@'))) {
                try {
                  const convoId = peerNodeId;
                  setSelectedConvoId(convoId);
                  setPendingConvoMeta({ did: peerNodeId, handle: peerName || peerNodeId, protocol: 'activitypub' });
                } catch (e) {
                  console.error('Failed to get/create AP convo:', e);
                }
              } else {
                setSelectedConvoId(\`p2p:\${peerNodeId}\`);
                setPendingConvoMeta(null);
              }
              openTab("chat", "Chat");
              setActiveSidebar("chat");
            }}`;

const replacement = `            onOpenChat={async (peerNodeId, peerName) => {
              const { messages, setSelectedConvoId, setPendingConvoMeta } = useChatStore.getState();
              
              let existingConvoId: string | null = null;
              if (peerNodeId.startsWith('did:plc:') || peerNodeId.startsWith('did:web:')) {
                  existingConvoId = messages.find(m => m.peer_did === peerNodeId || m.sender_did === peerNodeId)?.convo_id || null;
              } else if (!peerNodeId.startsWith('http') && !(peerName && peerName.includes('@'))) {
                  existingConvoId = \`p2p:\${peerNodeId}\`;
              }

              if (existingConvoId) {
                  setSelectedConvoId(existingConvoId);
                  setPendingConvoMeta(null);
                  openTab("chat", "Chat");
                  setActiveSidebar("chat");
                  return;
              }

              if (peerNodeId.startsWith('did:plc:') || peerNodeId.startsWith('did:web:')) {
                try {
                  const convoId = await invoke<string>('get_or_create_bsky_convo', { memberDid: peerNodeId });
                  setSelectedConvoId(convoId);
                  setPendingConvoMeta({ did: peerNodeId, handle: peerName || peerNodeId, protocol: 'bluesky' });
                } catch (e) {
                  console.error('Failed to get/create convo:', e);
                  alert(\`Failed to start chat: \${e}\`);
                  return;
                }
              } else if (peerNodeId.startsWith('http') || (peerName && peerName.includes('@'))) {
                try {
                  const convoId = peerNodeId;
                  setSelectedConvoId(convoId);
                  setPendingConvoMeta({ did: peerNodeId, handle: peerName || peerNodeId, protocol: 'activitypub' });
                } catch (e) {
                  console.error('Failed to get/create AP convo:', e);
                  return;
                }
              }
              openTab("chat", "Chat");
              setActiveSidebar("chat");
            }}`;

content = content.replace(target, replacement).replace(target, replacement);

fs.writeFileSync('/home/korendian/Bytescape/bytescape/src/App.tsx', content);
