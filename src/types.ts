export type ConnectionStatus = "Disconnected" | "Connected" | { Error: string };

export type DskRuntimeSnapshot = {
  on_air: boolean;
  in_transition: boolean;
  remaining_frames: number;
};

export type DskPropertiesSnapshot = {
  tie: boolean;
  rate: number;
};

export type SourceItemDto = {
  source_index: number;
  source_id: number;
  source_name: string;
};

export type AtemSnapshotDto = {
  initialisation_complete: boolean;
  mes_count: number;
  aux_count: number;
  program_sources: string[];
  preview_sources: string[];
  available_sources: string[];
  program_source_ids: number[];
  preview_source_ids: number[];
  available_source_items: SourceItemDto[];
  tally_by_source: Record<string, string[]>;
  dsk_keys: number[];
  dsk_sources: Record<string, [string, string]>;
  dsk_state: Record<string, DskRuntimeSnapshot>;
  dsk_properties: Record<string, DskPropertiesSnapshot>;
  transition_position: number[];
  transition_in_progress: boolean[];
  ftb_fully_black: boolean[];
  ftb_in_transition: boolean[];
  ftb_frames_remaining: number[];
  ftb_rate: number[];
};

export type SnapshotEvent = {
  snapshot: AtemSnapshotDto;
  status: ConnectionStatus;
};
