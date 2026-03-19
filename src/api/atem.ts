import { invoke } from "@tauri-apps/api/core";
import type { AtemSnapshotDto, ConnectionStatus } from "../types";

export async function connectAtem(ip: string, port: number, reconnect = true): Promise<void> {
  await invoke("connect", { ip, port, reconnect });
}

export async function disconnectAtem(): Promise<void> {
  await invoke("disconnect");
}

export async function getSnapshot(): Promise<AtemSnapshotDto> {
  return invoke<AtemSnapshotDto>("get_snapshot");
}

export async function getConnectionStatus(): Promise<ConnectionStatus> {
  return invoke<ConnectionStatus>("get_connection_status");
}

export async function setProgramInputByIndex(me: number, sourceIndex: number): Promise<void> {
  await invoke("set_program_input_by_index", { me, sourceIndex });
}

export async function setPreviewInputByIndex(me: number, sourceIndex: number): Promise<void> {
  await invoke("set_preview_input_by_index", { me, sourceIndex });
}

export async function cut(me: number): Promise<void> {
  await invoke("cut", { me });
}

export async function autoTransition(me: number): Promise<void> {
  await invoke("auto_transition", { me });
}

export async function setNextTransition(me: number, transition: string): Promise<void> {
  await invoke("set_next_transition", { me, transition });
}

export async function setDskTie(key: number, tie: boolean): Promise<void> {
  await invoke("set_dsk_tie", { key, tie });
}

export async function setDskOnAir(key: number, onAir: boolean): Promise<void> {
  await invoke("set_dsk_on_air", { key, onAir });
}

export async function dskAuto(key: number): Promise<void> {
  await invoke("dsk_auto", { key });
}

export async function toggleAutoBlack(me: number): Promise<void> {
  await invoke("toggle_auto_black", { me });
}
