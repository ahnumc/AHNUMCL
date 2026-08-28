export interface TerracottaState {
  installed: boolean;
  installedVersion?: string;
  latestVersion?: string;
  updateAvailable: boolean;
  running: boolean;
  status: string;
  roomCode?: string;
  serverAddress?: string;
  players: TerracottaPlayer[];
  downloadProgress?: number | null;
  downloadStage?: string | null;
}

export interface TerracottaPlayer {
  machineId: string;
  name: string;
  kind?: string;
}
