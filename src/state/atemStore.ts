import { listen } from "@tauri-apps/api/event";
import type { AtemSnapshotDto, ConnectionStatus, SnapshotEvent } from "../types";
import { getConnectionStatus, getSnapshot } from "../api/atem";

type AtemUiState = {
  snapshot: AtemSnapshotDto;
  status: ConnectionStatus;
};

const emptySnapshot: AtemSnapshotDto = {
  initialisation_complete: false,
  mes_count: 1,
  aux_count: 0,
  program_sources: [],
  preview_sources: [],
  available_sources: [],
  program_source_ids: [],
  preview_source_ids: [],
  available_source_items: [],
  tally_by_source: {},
  dsk_keys: [],
  dsk_sources: {},
  dsk_state: {},
  dsk_properties: {},
  transition_position: [],
  transition_in_progress: [],
  ftb_fully_black: [],
  ftb_in_transition: [],
  ftb_frames_remaining: [],
  ftb_rate: []
};

let state: AtemUiState = {
  snapshot: emptySnapshot,
  status: "Disconnected"
};

const subscribers = new Set<() => void>();
let initialized = false;

function publish(next: AtemUiState): void {
  state = next;
  subscribers.forEach((subscriber) => subscriber());
}

export function getAtemUiState(): AtemUiState {
  return state;
}

export function subscribeAtemUiState(subscriber: () => void): () => void {
  subscribers.add(subscriber);
  return () => subscribers.delete(subscriber);
}

export async function refreshAtemState(): Promise<void> {
  try {
    const [snapshot, status] = await Promise.all([getSnapshot(), getConnectionStatus()]);
    publish({ snapshot, status });
  } catch (e) {
    publish({
      snapshot: state.snapshot,
      status: { Error: String(e) }
    });
  }
}

async function initAtemStore(): Promise<void> {
  if (initialized) return;
  initialized = true;

  await listen<SnapshotEvent>("atem://snapshot", (event) => {
    publish({
      snapshot: event.payload.snapshot,
      status: event.payload.status
    });
  });

  await refreshAtemState();
}

void initAtemStore();
