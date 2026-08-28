import { invoke } from "@tauri-apps/api/core";
import { InvokeResponse } from "@/models/response";
import { TerracottaState } from "@/models/terracotta";
import { responseHandler } from "@/utils/response";

export class TerracottaService {
  @responseHandler("terracotta")
  static async getState(): Promise<InvokeResponse<TerracottaState>> {
    return await invoke("terracotta_get_state");
  }

  @responseHandler("terracotta")
  static async download(version?: string): Promise<InvokeResponse<void>> {
    return await invoke("terracotta_download", { version });
  }

  @responseHandler("terracotta")
  static async update(): Promise<InvokeResponse<void>> {
    return await invoke("terracotta_update");
  }

  @responseHandler("terracotta")
  static async start(): Promise<InvokeResponse<void>> {
    return await invoke("terracotta_start");
  }

  @responseHandler("terracotta")
  static async host(
    playerName: string,
    roomCode?: string
  ): Promise<InvokeResponse<void>> {
    return await invoke("terracotta_host", { playerName, roomCode });
  }

  @responseHandler("terracotta")
  static async join(
    playerName: string,
    roomCode: string
  ): Promise<InvokeResponse<void>> {
    return await invoke("terracotta_join", { playerName, roomCode });
  }

  @responseHandler("terracotta")
  static async closeRoom(): Promise<InvokeResponse<void>> {
    return await invoke("terracotta_close_room");
  }

  @responseHandler("terracotta")
  static async stop(): Promise<InvokeResponse<void>> {
    return await invoke("terracotta_stop");
  }
}
