const fs = require('fs');
const content = fs.readFileSync('/home/korendian/Bytescape/bytescape/src/App.tsx', 'utf-8');

const oldStr = `onOpenChat={async (peerNodeId, peerName) => {
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

const newStr = `onOpenChat={async (peerNodeId, peerName) => {
              const { setSelectedConvoId, setPendingConvoMeta, messages } = useChatStore.getState();
              
              if (peerNodeId.startsWith('did:plc:') || peerNodeId.startsWith('did:web:')) {
                try {
                  // Bypass backend command if possible by finding an existing chat in local state
                  const existingConvo = Array.from(messages.values()).find(
                    msg => msg.protocol === 'bluesky' && (msg.sender_did === peerNodeId || msg.recipient_did === peerNodeId || msg.convo_id?.includes(peerNodeId))
                  );
                  
                  let convoId;
                  if (existingConvo && existingConvo.convo_id) {
                     convoId = existingConvo.convo_id;
                  } else {
                     convoId = await invoke<string>('get_or_create_bsky_convo', { memberDid: peerNodeId });
                  }
                  
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

let newContent = content.replaceAll(oldStr, newStr);
fs.writeFileSync('/home/korendian/Bytescape/bytescape/src/App.tsx', newContent);
console.log("App patched");
