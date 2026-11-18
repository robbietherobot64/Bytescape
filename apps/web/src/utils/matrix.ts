import * as sdk from 'matrix-js-sdk';

let clientInstance: sdk.MatrixClient | null = null;

export async function getMatrixClient(): Promise<sdk.MatrixClient> {
  if (clientInstance) return clientInstance;

  const token = localStorage.getItem('bytescape_token');
  if (!token) throw new Error('Not authenticated');

  const baseUrl = import.meta.env.VITE_API_URL || (window.location.hostname === 'localhost' ? 'http://localhost:8080' : '');
  const client = sdk.createClient({
    baseUrl,
  });

  await client.login("org.matrix.login.jwt", { token });
  await client.startClient({ initialSyncLimit: 10 });

  clientInstance = client;
  return client;
}

export function getSyncedRooms(client: sdk.MatrixClient): sdk.Room[] {
  return client.getRooms().sort((a, b) => {
    const ta = a.getLastActiveTimestamp();
    const tb = b.getLastActiveTimestamp();
    return tb - ta;
  });
}

export function getRoomName(client: sdk.MatrixClient, roomId: string): string {
  const room = client.getRoom(roomId);
  return room?.name || roomId;
}

export function getMessagesForRoom(client: sdk.MatrixClient, roomId: string): any[] {
  const room = client.getRoom(roomId);
  if (!room) return [];
  return room.getLiveTimeline().getEvents();
}

export function disposeMatrixClient() {
  if (clientInstance) {
    clientInstance.stopClient();
    clientInstance = null;
  }
}
