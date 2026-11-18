export async function generateSigningKey(password: string): Promise<{ publicKeyHex: string; encryptedPrivateKey: string }> {
  const keyPair = await crypto.subtle.generateKey({ name: 'Ed25519', namedCurve: 'Ed25519' } as any, true, ['sign', 'verify']);
  const pubRaw = await crypto.subtle.exportKey('raw', keyPair.publicKey);
  const pubBytes = new Uint8Array(pubRaw);
  const publicKeyHex = Array.from(pubBytes).map(b => b.toString(16).padStart(2, '0')).join('');
  const privRaw = await crypto.subtle.exportKey('pkcs8', keyPair.privateKey);
  const privBytes = new Uint8Array(privRaw);
  const pwKey = await crypto.subtle.importKey('raw', new TextEncoder().encode(password.padEnd(16, 'x').slice(0, 32)), { name: 'AES-GCM' }, false, ['encrypt']);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const encrypted = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, pwKey, privBytes);
  const combined = new Uint8Array(iv.length + encrypted.byteLength);
  combined.set(iv);
  combined.set(new Uint8Array(encrypted), iv.length);
  const b64 = btoa(String.fromCharCode(...combined));
  return { publicKeyHex, encryptedPrivateKey: b64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '') };
}
